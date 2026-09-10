use std::cell::OnceCell;

use goosic_linux::shell::Shell;
use goosic_linux::APP_ID;
use gtk::glib;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    // The shell outlives any one activation. Launching Goosic again while it runs activates this
    // instance, which presents the window it already has rather than building a second one.
    let shell = OnceCell::new();
    app.connect_activate(move |app| shell.get_or_init(|| Shell::start(app)).window.present());
    app.run()
}