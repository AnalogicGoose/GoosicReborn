//! The shell against the real service: start it, and hear it answer on a main loop.
//!
//! Needs `goosic-service` built by the root workspace first — `cargo build -p goosic-service` at
//! the repository root — or `GOOSIC_SERVICE_PATH` pointing at one. It fails rather than skips when
//! neither exists, because a test that quietly does nothing looks exactly like one that passed.
//! It needs no display: the answer is awaited on a plain GLib main context, and GTK is never
//! initialised.

use std::path::{Path, PathBuf};

use goosic_linux::bridge;
use goosic_protocol::{
    conformance::exchange_cases, RequestEnvelope, RequestPayload, ResponseEnvelope,
};
use goosic_shell_support::{ServiceClientBuilder, TransportError};
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

#[test]
fn the_shell_client_observes_the_lease_exchange_fixtures() {
    for name in ["claim-then-release", "owner-conflict", "stale-generation"] {
        let exchange = exchange_cases()
            .into_iter()
            .find(|case| case.name == name)
            .unwrap();
        let client = ServiceClientBuilder::new(service_path())
            .spawn()
            .expect("service starts");
        let context = glib::MainContext::new();
        let initial = context
            .block_on(bridge::request(&client, "hello", RequestPayload::default()))
            .expect("initial state")
            .payload
            .and_then(|payload| payload.state)
            .expect("service reports its lease");
        let base = initial.generation;
        for step in exchange.steps {
            let mut request: RequestEnvelope = serde_json::from_str(&step.request).unwrap();
            request.payload.generation = request
                .payload
                .generation
                .map(|generation| generation + base);
            let mut expected: ResponseEnvelope = serde_json::from_str(&step.response).unwrap();
            if let Some(payload) = expected.payload.as_mut() {
                payload.generation = payload.generation.map(|generation| generation + base);
                if let Some(state) = payload.state.as_mut() {
                    state.generation += base;
                    state.account_id = initial.account_id.clone();
                }
            }
            let result =
                context.block_on(bridge::request(&client, &request.command, request.payload));
            if expected.ok {
                let actual =
                    result.unwrap_or_else(|error| panic!("{}: {}: {error:?}", name, step.why));
                assert_eq!(actual.payload, expected.payload, "{}: {}", name, step.why);
            } else {
                let error = result.unwrap_err();
                let expected = expected.error.expect("refusal reason");
                assert_eq!(
                    error,
                    TransportError::Remote {
                        code: expected.code,
                        message: expected.message
                    },
                    "{}: {}",
                    name,
                    step.why
                );
            }
        }
    }
}
