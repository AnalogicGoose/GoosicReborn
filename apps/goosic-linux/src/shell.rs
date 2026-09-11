//! The shell's state for the life of the application, and the window it shows.
//!
//! The service connection belongs to the application rather than to a window, because closing the
//! window will not end the conversation once the shell can play in the background.

use core::error;
use std::rc::Rc;

use goosic_protocol::{RequestPayload, ResponseEnvelope};
use goosic_shell_support::{ServiceClient, TransportError, client};
use gtk::glib;

use crate::{bridge, service, shell};

pub struct Shell {
    pub window: gtk::ApplicationWindow,
    status: gtk::Label,
    client: Option<ServiceClient>,
}

impl Shell {
    /// Builds the window, starts the service and asks it to say hello.
    pub fn start(app: &gtk::Application) -> Rc<Shell> {
        let status = gtk::Label::builder()
        .label("Connecting to goosic-service...")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Goosic")
            .default_width(1100)
            .default_height(720)
            .child(&status)
            .build();

        let client = match service::launch() {
            Ok(client) => Some(client),
            Err(error) => {
                eprintln!("goosic: {error}");
                status.set_label(&format!("Could not start goosic-service.\n{error}"));
                None
            }
        };

        let shell = Rc::new(Shell {
            window,
            status,
            client,
        });
        shell.connect();
        shell
    }

    /// The first exchange is `hello`, before anything else is asked, so a service that is missing
    /// or speaks another protocol version fails while the window still says "Connecting".
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

    fn show_connection(&self, result: Result<ResponseEnvelope, TransportError>) {
        match result {
            Ok(response) => {
                let message = response
                    .payload
                    .and_then(|payload| payload.message)
                    .unwrap_or_else(|| "goosic-service ready".to_owned());
                eprintln!("goosic: connected — {message}");
                self.status.set_label(&format!("Connected. {message}."));
            }
            Err(error) => {
                let (code, message) = error.describe();
                eprintln!("goosic: could not connect ({code}): {message}");
                self.status
                    .set_label(&format!("Could not reach goosic-service.\n{message}"));
            }
        }
    }
}