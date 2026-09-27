//! Account-scoped catalog reads inside the active account's WebKit profile.
//!
//! The shared GPL program performs the authenticated request in the page. Only normalized music
//! metadata returns to this shell; neither cookies nor InnerTube headers enter the service wire.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use goosic_protocol::CatalogPage;
use gtk::prelude::*;
use gtk::{gio, glib};
use serde_json::{json, Value};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::LoadEvent;

use crate::{official_host, web_profile};

const PROGRAM: &str =
    include_str!("../../goosic-swift/Sources/GoosicSwift/Resources/PersonalCatalog.js");
const PAGE: &str = "https://music.youtube.com/";
const REQUEST_TIMEOUT: u32 = 45;

type Completion = Box<dyn FnOnce(Result<Value, String>)>;

struct Pending {
    epoch: u64,
    expression: String,
    completion: Rc<RefCell<Option<Completion>>>,
}

pub struct PersonalHost {
    container: gtk::Box,
    view: RefCell<Option<webkit6::WebView>>,
    profile: Cell<Option<Uuid>>,
    epoch: Cell<u64>,
    ready: Cell<bool>,
    failed: Cell<bool>,
    pending: RefCell<Vec<Pending>>,
}

impl PersonalHost {
    pub fn new() -> Rc<Self> {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_size_request(1, 1);
        container.set_hexpand(false);
        container.set_vexpand(false);
        container.set_halign(gtk::Align::Start);
        container.set_valign(gtk::Align::Start);
        container.set_overflow(gtk::Overflow::Hidden);
        container.set_can_target(false);
        container.set_focusable(false);
        Rc::new(Self {
            container,
            view: RefCell::new(None),
            profile: Cell::new(None),
            epoch: Cell::new(0),
            ready: Cell::new(false),
            failed: Cell::new(false),
            pending: RefCell::new(Vec::new()),
        })
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    pub fn bind(self: &Rc<Self>, profile: Option<Uuid>) {
        if self.profile.get() == profile {
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.profile.set(profile);
        self.ready.set(false);
        self.failed.set(false);
        self.fail_pending("The active account changed while its library was loading.");
        if let Some(old) = self.view.borrow_mut().take() {
            old.stop_loading();
            self.container.remove(&old);
        }
        let Some(profile) = profile else { return };
        let view = web_profile::profile_view(profile);
        view.set_size_request(1, 1);
        view.set_hexpand(false);
        view.set_vexpand(false);
        view.set_is_muted(true);
        web_profile::guard_navigation(&view, official_host::is_official_page);
        let epoch = self.epoch.get();
        let weak = Rc::downgrade(self);
        view.connect_load_changed(move |view, event| {
            if event != LoadEvent::Finished {
                return;
            }
            let Some(host) = weak.upgrade() else { return };
            if host.epoch.get() != epoch
                || !view
                    .uri()
                    .as_deref()
                    .is_some_and(|uri| uri.starts_with(PAGE))
            {
                return;
            }
            host.ready.set(true);
            host.failed.set(false);
            let requests = std::mem::take(&mut *host.pending.borrow_mut());
            for request in requests {
                host.evaluate(request);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_load_failed(move |_, _, _, _| {
            if let Some(host) = weak.upgrade() {
                if host.epoch.get() == epoch {
                    host.failed.set(true);
                    host.fail_pending("YouTube Music did not load for this account.");
                }
            }
            false
        });
        self.container.append(&view);
        *self.view.borrow_mut() = Some(view.clone());
        view.load_uri(PAGE);
    }

    pub fn browse(
        self: &Rc<Self>,
        browse_id: &str,
        title: &str,
        continuation: Option<&str>,
        shape: &str,
        done: impl FnOnce(Result<CatalogPage, String>) + 'static,
    ) {
        self.run(
            "browse",
            json!([browse_id, title, continuation, shape]),
            move |answer| {
                done(answer.and_then(|value| {
                    serde_json::from_value(value)
                        .map_err(|_| "The account returned an unreadable catalog page.".to_owned())
                }))
            },
        );
    }

    pub fn mutate(
        self: &Rc<Self>,
        operation: &str,
        arguments: Value,
        done: impl FnOnce(Result<Value, String>) + 'static,
    ) {
        self.run("mutate", json!([operation, arguments]), done);
    }

    fn run(
        self: &Rc<Self>,
        function: &'static str,
        arguments: Value,
        done: impl FnOnce(Result<Value, String>) + 'static,
    ) {
        if self.profile.get().is_none() {
            done(Err("Sign in to load your personal library.".to_owned()));
            return;
        }
        // Function names are fixed here. Arguments cross as a JSON literal, never as source.
        // A persisted account record can outlive Google's cookie. Refuse the read instead of
        // returning a guest Home under that account's name. Only a boolean stays in this page;
        // the cookie itself never returns to Rust or enters the service protocol.
        let expression = format!(
            "if (!/(?:^|;\\s*)(?:__Secure-3PAPISID|SAPISID)=/.test(document.cookie)) \
             throw new Error('Account session expired');\n{PROGRAM}\nconst __args = {arguments};\n\
             return await GoosicPersonalCatalog.{function}(...__args);"
        );
        let completion: Rc<RefCell<Option<Completion>>> =
            Rc::new(RefCell::new(Some(Box::new(done))));
        let timeout = completion.clone();
        glib::timeout_add_seconds_local_once(REQUEST_TIMEOUT, move || {
            if let Some(done) = timeout.borrow_mut().take() {
                done(Err(
                    "YouTube Music did not answer in time. Try again.".to_owned()
                ));
            }
        });
        let request = Pending {
            epoch: self.epoch.get(),
            expression,
            completion,
        };
        if self.ready.get() {
            self.evaluate(request);
        } else {
            self.pending.borrow_mut().push(request);
            if self.failed.replace(false) {
                if let Some(view) = self.view.borrow().as_ref() {
                    view.load_uri(PAGE);
                }
            }
        }
    }

    fn evaluate(self: &Rc<Self>, request: Pending) {
        if request.epoch != self.epoch.get() {
            return;
        }
        let Some(view) = self.view.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        view.call_async_javascript_function(
            &request.expression,
            None,
            None,
            None,
            None::<&gio::Cancellable>,
            move |answer| {
                let result = match answer {
                    Ok(value) => value
                        .to_json(0)
                        .ok_or_else(|| "The account returned no catalog answer.".to_owned())
                        .and_then(|json| decode_result(&json)),
                    Err(error) => {
                        eprintln!("goosic: personal catalog JavaScript failed: {error}");
                        Err("The account could not read its YouTube Music page.".to_owned())
                    }
                };
                if let Some(done) = request.completion.borrow_mut().take() {
                    if weak
                        .upgrade()
                        .is_some_and(|host| host.epoch.get() == request.epoch)
                    {
                        done(result);
                    } else {
                        done(Err(
                            "The active account changed while its library was loading.".to_owned(),
                        ));
                    }
                }
            },
        );
    }

    fn fail_pending(&self, message: &str) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for request in pending {
            if let Some(done) = request.completion.borrow_mut().take() {
                done(Err(message.to_owned()));
            }
        }
    }
}

fn decode_result(json: &str) -> Result<Value, String> {
    let outer: Value = serde_json::from_str(json)
        .map_err(|_| "The account returned an unreadable answer.".to_owned())?;
    match outer {
        Value::String(inner) => serde_json::from_str(&inner)
            .map_err(|_| "The account returned an unreadable catalog answer.".to_owned()),
        value => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_program_answer_is_decoded_once_without_transporting_credentials() {
        assert_eq!(
            decode_result("\"{\\\"title\\\":\\\"Home\\\"}\"").unwrap()["title"],
            "Home"
        );
        assert!(PROGRAM.contains("GoosicPersonalCatalog"));
        assert!(!PROGRAM.contains("window.GoosicPersonalCatalog"));
    }

    #[test]
    #[ignore = "requires a signed-in profile, a display, and live YouTube Music"]
    fn live_account_reads_home_and_library() {
        let profile = std::env::var("GOOSIC_LIVE_PROFILE")
            .expect("set GOOSIC_LIVE_PROFILE to an existing signed-in profile UUID")
            .parse::<Uuid>()
            .expect("profile UUID");
        gtk::init().expect("display");
        let host = PersonalHost::new();
        let window = gtk::Window::new();
        window.set_child(Some(host.widget()));
        window.present();
        host.bind(Some(profile));
        let loop_ = glib::MainLoop::new(None, false);
        let answer = Rc::new(RefCell::new(None));
        let answer_slot = answer.clone();
        let done = loop_.clone();
        let reader = host.clone();
        host.browse("FEmusic_home", "Home", None, "shelves", move |home| {
            let home = match home {
                Ok(page) => page,
                Err(message) => {
                    *answer_slot.borrow_mut() = Some(Err(message));
                    done.quit();
                    return;
                }
            };
            let songs_reader = reader.clone();
            reader.browse(
                "FEmusic_liked_playlists",
                "Playlists",
                None,
                "shelves",
                move |library| {
                    let library = match library {
                        Ok(page) => page,
                        Err(message) => {
                            *answer_slot.borrow_mut() = Some(Err(message));
                            done.quit();
                            return;
                        }
                    };
                    let owned_reader = songs_reader.clone();
                    songs_reader.browse("VLLM", "Songs", None, "tracks", move |songs| {
                        let songs = match songs {
                            Ok(page) => page,
                            Err(message) => {
                                *answer_slot.borrow_mut() = Some(Err(message));
                                done.quit();
                                return;
                            }
                        };
                        owned_reader.mutate("listUserPlaylists", json!({}), move |owned| {
                            *answer_slot.borrow_mut() =
                                Some(owned.map(|owned| (home, library, songs, owned)));
                            done.quit();
                        });
                    });
                },
            );
        });
        let timeout = loop_.clone();
        glib::timeout_add_seconds_local_once(100, move || timeout.quit());
        loop_.run();
        let (home, library, songs, owned) = answer
            .borrow_mut()
            .take()
            .expect("account answered")
            .expect("signed-in catalog");
        assert_eq!(home.id, "personal:FEmusic_home");
        assert!(!home.shelves.is_empty(), "Home has personalized shelves");
        assert_eq!(library.id, "personal:FEmusic_liked_playlists");
        assert_eq!(songs.id, "personal:VLLM");
        assert!(
            owned["value"].is_array(),
            "the playlist picker receives a list"
        );
        window.close();
    }
}
