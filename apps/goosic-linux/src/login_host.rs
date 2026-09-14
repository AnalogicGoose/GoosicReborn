//! The sign-in window.
//!
//! A window of its own, holding a renderer whose cookies land in a staging directory rather than in
//! the profile the account will play under. Nothing is promoted until Rust has accepted the
//! account, so an abandoned or failed sign-in leaves a directory that is deleted rather than a
//! profile that half exists. Where the window may navigate and when a sign-in has finished are the
//! shared rules in `goosic-shell-support`: a second copy would be a second answer about where a
//! password may be typed.
//!
//! There is deliberately no bridge. A sign-in is a browser surface, not a playback channel, and all
//! that crosses from the page is the bounded metadata the completion script returns.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use goosic_shell_support::login::{
    self, LoginResult, PollingDecision, COMPLETION_SCRIPT, COMPLETION_TIMEOUT,
};
use gtk::prelude::*;
use gtk::{gio, glib};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::LoadEvent;

use crate::web_profile;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

const SIGN_IN_PAGE: &str =
    "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmusic.youtube.com";

/// Where the window tells the shell how the sign-in ended.
pub struct LoginHandlers {
    /// The account signed in. Its staged storage is still there, for the shell to promote or
    /// discard once Rust has answered.
    pub on_completed: Box<dyn Fn(LoginResult)>,
    /// The window was closed or gave up. Its staged storage is already gone.
    pub on_cancelled: Box<dyn Fn()>,
}

pub struct LoginHost {
    window: RefCell<Option<gtk::Window>>,
    view: RefCell<Option<webkit6::WebView>>,
    account_id: Uuid,
    profile_id: Uuid,
    /// Bumped by every finished load and by closing, so a poll from an earlier page cannot act.
    token: Cell<u64>,
    deadline: Cell<Option<Instant>>,
    poll: RefCell<Option<glib::SourceId>>,
    closing: Cell<bool>,
    completed: Cell<bool>,
    /// A check is still running in the page. The check may open the account menu and wait for it,
    /// and a second one started meanwhile would click the menu shut again.
    evaluating: Cell<bool>,
    /// Whether a failing check has been reported, so a broken one is said once, not every tick.
    failure_reported: Cell<bool>,
    handlers: LoginHandlers,
}

impl LoginHost {
    pub fn open(parent: &impl IsA<gtk::Window>, handlers: LoginHandlers) -> Rc<LoginHost> {
        // Both identities are made here, before the page opens, and never derived from anything the
        // provider says. They are distinct by construction.
        let account_id = Uuid::new_v4();
        let profile_id = loop {
            let id = Uuid::new_v4();
            if id != account_id {
                break id;
            }
        };

        let view = web_profile::staging_view(profile_id);
        // `about:blank` is where a fresh view starts, and it carries no origin to be redirected to.
        web_profile::guard_navigation(&view, |uri| {
            uri == "about:blank" || login::is_allowed_login_url(uri)
        });
        view.set_hexpand(true);
        view.set_vexpand(true);
        let window = gtk::Window::builder()
            .title("Sign in to YouTube Music")
            .default_width(720)
            .default_height(640)
            .transient_for(parent)
            .child(&view)
            .build();
        // Belongs to the application like the main window, so the application tracks it and does
        // not treat it as a stray.
        window.set_application(parent.as_ref().application().as_ref());

        let host = Rc::new(LoginHost {
            window: RefCell::new(Some(window.clone())),
            view: RefCell::new(Some(view.clone())),
            account_id,
            profile_id,
            token: Cell::new(0),
            deadline: Cell::new(None),
            poll: RefCell::new(None),
            closing: Cell::new(false),
            completed: Cell::new(false),
            evaluating: Cell::new(false),
            failure_reported: Cell::new(false),
            handlers,
        });

        let weak = Rc::downgrade(&host);
        view.connect_load_changed(move |_, event| {
            if event == LoadEvent::Finished {
                if let Some(host) = weak.upgrade() {
                    host.watch_for_completion();
                }
            }
        });
        // YouTube Music moves between its pages without loading a document, so a finished load is
        // not the only navigation there is: without this the deadline ran out while the user was
        // still looking around a signed-in page.
        let weak = Rc::downgrade(&host);
        view.connect_uri_notify(move |_| {
            if let Some(host) = weak.upgrade() {
                host.watch_for_completion();
            }
        });
        let weak = Rc::downgrade(&host);
        window.connect_close_request(move |_| {
            if let Some(host) = weak.upgrade() {
                if !host.closing.get() {
                    // GTK destroys the window once this returns; destroying it here as well would
                    // tear a widget down inside its own handler.
                    host.window.borrow_mut().take();
                    host.cancel();
                }
            }
            glib::Propagation::Proceed
        });

        window.present();
        view.load_uri(SIGN_IN_PAGE);
        host
    }

    /// Closes the window and stops watching. The staged storage is left for the caller.
    pub fn close(&self) {
        if self.closing.replace(true) {
            return;
        }
        self.stop_polling();
        self.token.set(self.token.get() + 1);
        let view = self.view.borrow_mut().take();
        if let Some(view) = view {
            view.stop_loading();
        }
        let window = self.window.borrow_mut().take();
        if let Some(window) = window {
            window.destroy();
        }
    }

    fn cancel(&self) {
        self.close();
        web_profile::discard_staging(self.profile_id);
        (self.handlers.on_cancelled)();
    }

    /// Each navigation restarts the watch and its deadline: a second-factor prompt is a page of its
    /// own, and it can take a minute of attention.
    fn watch_for_completion(self: &Rc<Self>) {
        if self.closing.get() || self.completed.get() {
            return;
        }
        self.stop_polling();
        let token = self.token.get() + 1;
        self.token.set(token);
        self.deadline.set(Some(Instant::now() + COMPLETION_TIMEOUT));
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(POLL_INTERVAL, move || match weak.upgrade() {
            Some(host) => host.poll_once(token),
            None => glib::ControlFlow::Break,
        });
        *self.poll.borrow_mut() = Some(source);
    }

    fn stop_polling(&self) {
        let source = self.poll.borrow_mut().take();
        if let Some(source) = source {
            source.remove();
        }
    }

    /// Asks the live document whether the account menu is really there. Arriving at
    /// `music.youtube.com` is not enough on its own — the signed-out shell lives at the same
    /// address — so the origin is checked again on every tick, not once.
    fn poll_once(self: &Rc<Self>, token: u64) -> glib::ControlFlow {
        if self.closing.get() || token != self.token.get() {
            return glib::ControlFlow::Break;
        }
        let deadline = self.deadline.get().unwrap_or_else(Instant::now);
        if Instant::now() >= deadline {
            // Returning Break removes this source, so its id must not be removed again.
            self.poll.borrow_mut().take();
            // Without this the window simply vanishes, which looks the same as a crash.
            eprintln!(
                "goosic login: gave up waiting for a completed sign-in after {}s",
                COMPLETION_TIMEOUT.as_secs()
            );
            self.cancel();
            return glib::ControlFlow::Break;
        }
        let view = self.view.borrow().clone();
        let Some(view) = view else {
            return glib::ControlFlow::Break;
        };
        let on_completion_origin = view
            .uri()
            .as_deref()
            .is_some_and(login::is_exact_completion_origin);
        if on_completion_origin && !self.evaluating.replace(true) {
            let weak = Rc::downgrade(self);
            // The shared check is the body of an async function — it opens the account menu and
            // waits for it — so it is called as one, in the page's own world where `ytcfg` lives.
            view.call_async_javascript_function(
                COMPLETION_SCRIPT,
                None,
                None,
                None,
                None::<&gio::Cancellable>,
                move |result| {
                    let Some(host) = weak.upgrade() else {
                        return;
                    };
                    host.evaluating.set(false);
                    match result {
                        Ok(value) if value.is_string() => {
                            host.consider(token, deadline, value.to_str().as_str());
                        }
                        Ok(_) => {}
                        Err(error) => {
                            // The page's exception, never the page's URL or content.
                            if !host.failure_reported.replace(true) {
                                eprintln!("goosic login: the completion check failed: {error}");
                            }
                        }
                    }
                },
            );
        }
        glib::ControlFlow::Continue
    }

    fn consider(&self, token: u64, deadline: Instant, report: &str) {
        if report.is_empty() || self.closing.get() || self.completed.get() {
            return;
        }
        let uri = self.view.borrow().as_ref().and_then(|view| view.uri());
        let Some(uri) = uri else {
            return;
        };
        let summary = login::sanitize_metadata(report.as_bytes());
        let decision = PollingDecision::decide(
            token,
            self.token.get(),
            Instant::now(),
            deadline,
            login::is_exact_completion_origin(&uri),
            summary.as_ref(),
        );
        if decision != PollingDecision::Accept {
            return;
        }
        let Some(result) =
            login::make_result(self.account_id, self.profile_id, report.as_bytes(), &uri)
        else {
            return;
        };
        self.completed.set(true);
        // The staged storage stays: it becomes the account's profile once Rust accepts it.
        self.close();
        (self.handlers.on_completed)(result);
    }
}
