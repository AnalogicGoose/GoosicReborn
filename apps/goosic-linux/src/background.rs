//! Running with the window closed.
//!
//! Closing Goosic's window hides it and keeps the music playing; quitting is explicit — Ctrl+Q, the
//! status icon's menu, or a media panel's Quit. The first time the window goes away the desktop is
//! told so, because a player that keeps sounding after its window disappears is otherwise a player
//! nobody can find. While music is audible the session is asked not to suspend, and asked again as
//! soon as it is not, so a paused Goosic never keeps a laptop awake. Inside a Flatpak the Background
//! portal is asked for permission to keep running, which is what stops the sandbox from counting a
//! windowless app as finished.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

pub struct Background {
    /// Keeps the application running while no window is shown.
    _hold: gio::ApplicationHoldGuard,
    announced: Cell<bool>,
    /// The session's inhibition cookie while music is audible, or 0.
    inhibit: Cell<u32>,
    app: glib::WeakRef<gtk::Application>,
    window: RefCell<Option<glib::WeakRef<gtk::ApplicationWindow>>>,
}

impl Background {
    pub fn start(app: &gtk::Application, window: &gtk::ApplicationWindow) -> Rc<Background> {
        let quit = gio::SimpleAction::new("quit", None);
        {
            let app = app.downgrade();
            quit.connect_activate(move |_, _| {
                if let Some(app) = app.upgrade() {
                    app.quit();
                }
            });
        }
        app.add_action(&quit);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
        app.set_accels_for_action("window.close", &["<Control>w"]);

        let background = Rc::new(Background {
            _hold: app.hold(),
            announced: Cell::new(false),
            inhibit: Cell::new(0),
            app: app.downgrade(),
            window: RefCell::new(Some(window.downgrade())),
        });
        let weak = Rc::downgrade(&background);
        window.connect_close_request(move |window| {
            window.set_visible(false);
            if let Some(background) = weak.upgrade() {
                background.announce_once();
            }
            glib::Propagation::Stop
        });
        request_background_permission();
        background
    }

    fn announce_once(&self) {
        if self.announced.replace(true) {
            return;
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let notification = gio::Notification::new("Goosic is still running");
        notification.set_body(Some(
            "Closing the window keeps the music playing. Open Goosic again to bring it back, or \
             quit from the status icon or with Ctrl+Q.",
        ));
        app.send_notification(Some("background"), &notification);
    }

    /// Keeps the session awake while music plays, and lets it sleep as soon as it does not.
    pub fn set_audible(&self, audible: bool) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let cookie = self.inhibit.get();
        if audible && cookie == 0 {
            let window = self
                .window
                .borrow()
                .as_ref()
                .and_then(glib::WeakRef::upgrade);
            let cookie = app.inhibit(
                window.as_ref(),
                gtk::ApplicationInhibitFlags::SUSPEND,
                Some("Playing music"),
            );
            self.inhibit.set(cookie);
        } else if !audible && cookie != 0 {
            app.uninhibit(cookie);
            self.inhibit.set(0);
        }
    }
}

/// Asks the Background portal to let Goosic keep running with no window. Outside a Flatpak there is
/// nothing to ask: a native process is not ended for having closed its window.
fn request_background_permission() {
    if !Path::new("/.flatpak-info").exists() {
        return;
    }
    let Ok(connection) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        return;
    };
    let options = glib::VariantDict::new(None);
    options.insert_value(
        "reason",
        &"Goosic keeps playing when its window is closed.".to_variant(),
    );
    options.insert_value("autostart", &false.to_variant());
    let parameters = glib::Variant::tuple_from_iter(["".to_variant(), options.end()]);
    connection.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |_| {},
    );
}
