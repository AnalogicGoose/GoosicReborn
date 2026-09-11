//! The shell's state for the life of the application, and the window it shows.
//!
//! The service connection belongs to the application rather than to a window, because closing the
//! window will not end the conversation once the shell can play in the background. Navigation and
//! loaded pages live in a `Browser`, which decides everything without GTK; this file only carries
//! answers from the service into it and redraws.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use goosic_protocol::{RequestPayload, ResponseEnvelope};
use goosic_shell_support::navigation::{CatalogKey, EntityReference, Route, SearchFilter};
use goosic_shell_support::{ServiceClient, TransportError};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::pages::Browser;
use crate::{bridge, service, ui};

pub struct Shell {
    pub window: gtk::ApplicationWindow,
    client: Option<ServiceClient>,
    browser: RefCell<Browser>,
    rows: gio::ListStore,
    scroller: gtk::ScrolledWindow,
    search_bar: gtk::Box,
    connection: gtk::Label,
    status: gtk::Label,
}

impl Shell {
    /// Builds the window, starts the service and asks it to say hello.
    pub fn start(app: &gtk::Application) -> Rc<Shell> {
        let launched = service::launch();
        let shell = Rc::new_cyclic(|weak: &Weak<Shell>| {
            let actions = Rc::new(ui::Actions {
                open: Box::new(forward(weak, Shell::open)),
                retry: Box::new(forward_unit(weak, Shell::retry)),
                back: Box::new(forward_unit(weak, Shell::back)),
            });
            let (scroller, rows) = ui::page_list(actions);
            let search_bar = ui::search_bar(
                forward(weak, Shell::submit_search),
                forward(weak, Shell::select_filter),
            );
            let sidebar = ui::sidebar(forward(weak, Shell::navigate));

            let client = match launched {
                Ok(client) => Some(client),
                Err(error) => {
                    eprintln!("goosic: {error}");
                    sidebar.connection.set_label("○ Service offline");
                    sidebar
                        .status
                        .set_label(&format!("Could not start goosic-service.\n{error}"));
                    None
                }
            };

            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_hexpand(true);
            content.append(&search_bar);
            content.append(&scroller);
            let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            root.append(&sidebar.root);
            root.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            root.append(&content);

            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("Goosic")
                .default_width(1100)
                .default_height(720)
                .child(&root)
                .build();

            Shell {
                window,
                client,
                browser: RefCell::new(Browser::new()),
                rows,
                scroller,
                search_bar,
                connection: sidebar.connection,
                status: sidebar.status,
            }
        });
        shell.render();
        shell.connect();
        shell
    }

    /// The first exchange is `hello`, before anything else is asked, so a service that is missing
    /// or speaks another protocol version fails while the sidebar still says "Connecting".
    fn connect(self: &Rc<Self>) {
        let Some(client) = &self.client else {
            return;
        };
        let answer = bridge::request(client, "hello", RequestPayload::default());
        let shell = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = answer.await;
            if let Some(shell) = shell.upgrade() {
                shell.show_connection(result);
            }
        });
    }

    fn show_connection(self: &Rc<Self>, result: Result<ResponseEnvelope, TransportError>) {
        match result {
            Ok(response) => {
                let message = response
                    .payload
                    .and_then(|payload| payload.message)
                    .unwrap_or_else(|| "goosic-service ready".to_owned());
                eprintln!("goosic: connected — {message}");
                self.connection.set_label("● Rust service connected");
                self.status.set_label("");
                self.load_current(false);
            }
            Err(error) => {
                let (code, message) = error.describe();
                eprintln!("goosic: could not connect ({code}): {message}");
                self.connection.set_label("○ Service offline");
                self.status
                    .set_label(&format!("Could not reach goosic-service.\n{message}"));
            }
        }
    }

    fn navigate(self: &Rc<Self>, route: Route) {
        self.browser.borrow_mut().navigate(route);
        self.show_new_page();
    }

    fn open(self: &Rc<Self>, entity: EntityReference) {
        self.browser.borrow_mut().open(entity);
        self.show_new_page();
    }

    fn back(self: &Rc<Self>) {
        self.browser.borrow_mut().back();
        self.show_new_page();
    }

    fn retry(self: &Rc<Self>) {
        self.load_current(true);
    }

    fn submit_search(self: &Rc<Self>, text: String) {
        self.browser.borrow_mut().submit_search(&text);
        self.show_new_page();
    }

    fn select_filter(self: &Rc<Self>, filter: SearchFilter) {
        let changed = self.browser.borrow_mut().select_filter(filter);
        if changed {
            self.show_new_page();
        }
    }

    fn show_new_page(self: &Rc<Self>) {
        self.render();
        self.scroller.vadjustment().set_value(0.0);
        self.load_current(false);
    }

    fn load_current(self: &Rc<Self>, force: bool) {
        let key = self.browser.borrow().current_key();
        if let Some(key) = key {
            self.load(key, force);
        }
    }

    fn load(self: &Rc<Self>, key: CatalogKey, force: bool) {
        let Some(client) = &self.client else {
            self.browser.borrow_mut().fail_offline(key);
            self.render();
            return;
        };
        let request = self.browser.borrow_mut().begin(&key, force);
        let Some((command, payload)) = request else {
            return;
        };
        self.render();
        let answer = bridge::request(client, command, payload);
        let shell = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = answer.await;
            let Some(shell) = shell.upgrade() else {
                return;
            };
            shell.browser.borrow_mut().finish(key.clone(), result);
            // An answer for a page the user has left is kept for when they come back, not drawn
            // over the page they are on now.
            let current = shell.browser.borrow().current_key();
            if current.as_ref() == Some(&key) {
                shell.render();
            }
        });
    }

    fn render(&self) {
        let (rows, searching) = {
            let browser = self.browser.borrow();
            (browser.rows(), browser.shows_search_bar())
        };
        ui::set_rows(&self.rows, rows);
        self.search_bar.set_visible(searching);
    }
}

/// Turns a method into a callback that holds the shell weakly, so no widget's closure keeps the
/// shell alive on its own.
fn forward<A: 'static>(weak: &Weak<Shell>, method: fn(&Rc<Shell>, A)) -> impl Fn(A) + 'static {
    let weak = weak.clone();
    move |argument| {
        if let Some(shell) = weak.upgrade() {
            method(&shell, argument);
        }
    }
}

fn forward_unit(weak: &Weak<Shell>, method: fn(&Rc<Shell>)) -> impl Fn() + 'static {
    let weak = weak.clone();
    move || {
        if let Some(shell) = weak.upgrade() {
            method(&shell);
        }
    }
}
