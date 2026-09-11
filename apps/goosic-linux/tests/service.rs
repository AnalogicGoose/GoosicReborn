//! The shell against the real service: start it, and hear it answer on a main loop.
//!
//! Needs `goosic-service` built by the root workspace first — `cargo build -p goosic-service` at
//! the repository root — or `GOOSIC_SERVICE_PATH` pointing at one. It fails rather than skips when
//! neither exists, because a test that quietly does nothing looks exactly like one that passed.
//! It needs no display: the answer is awaited on a plain GLib main context, and GTK is never
//! initialised.

use std::path::{Path, PathBuf};

use goosic_linux::bridge;
use goosic_protocol::RequestPayload;
use goosic_shell_support::ServiceClientBuilder;
use gtk::glib;

fn service_path() -> PathBuf {
    std::env::var_os("GOOSIC_SERVICE_PATH")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/goosic-service")
        })
}

#[test]
fn the_shell_hears_the_service_answer_on_a_main_loop() {
    let path = service_path();
    assert!(
        path.is_file(),
        "no service at {} — run `cargo build -p goosic-service` at the repository root, \
        or set GOOSIC_SERVICE_PATH",
        path.display()
    );
    let client = ServiceClientBuilder::new(path)
        .spawn()
        .expect("the service should start");
    let context = glib::MainContext::new();
    let hello = context
        .block_on(bridge::request(&client, "hello", RequestPayload::default()))
        .expect("hello was refused");
    assert_eq!(
        hello.payload.and_then(|payload| payload.message).as_deref(),
        Some("goosic-service ready")
    );
}