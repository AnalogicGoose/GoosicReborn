//! The shared shell client against the real service binary.
//!
//! `goosic-shell-support` proves its concurrency against a stub whose timing it controls. This is
//! the other half: the same client, the real child process, and a conversation whose answers only
//! the real authority can give. It sends only commands that change nothing on disk, because the
//! binary reads the user's real settings and account files at startup.

use goosic_protocol::{Owner, RequestPayload};
use goosic_shell_support::{ServiceClientBuilder, TransportError};

#[test]
fn a_shell_can_hold_a_conversation_with_the_real_service() {
    let client = ServiceClientBuilder::new(env!("CARGO_BIN_EXE_goosic-service"))
        .request_id_prefix("client-test")
        .spawn()
        .expect("the service binary should start");

    let hello = client.request("hello", RequestPayload::default()).expect("hello failed");
    assert_eq!(
        hello.payload.as_ref().and_then(|payload| payload.message.as_deref()),
        Some("goosic-service ready")
    );

    // The starting generation is not always zero: restoring a saved account moves it. A shell
    // reads it rather than assuming it, and so does this test.
    let state = client.request("state.get", RequestPayload::default()).expect("state.get failed");
    let generation = state.payload.and_then(|payload| payload.state).expect("no state").generation;

    let claimed = client
        .request(
            "playback.claim",
            RequestPayload {
                owner: Some(Owner::OfficialWebView),
                generation: Some(generation),
                ..Default::default()
            },
        )
        .expect("claim failed");
    let leased = claimed.payload.and_then(|payload| payload.generation).expect("no generation");
    assert_eq!(leased, generation + 1, "a claim moves the generation");

    // A stale sample is the authority answering, and the conversation carries on after it.
    let stale = client
        .request(
            "playback.sample",
            RequestPayload {
                owner: Some(Owner::OfficialWebView),
                generation: Some(generation),
                sequence: Some(1),
                ..Default::default()
            },
        )
        .expect_err("a stale generation must be refused");
    assert!(matches!(&stale, TransportError::Remote { code, .. } if code == "generationMismatch"));
    assert!(!stale.invalidates_connection());

    client
        .request(
            "playback.release",
            RequestPayload {
                owner: Some(Owner::OfficialWebView),
                generation: Some(leased),
                ..Default::default()
            },
        )
        .expect("release failed");
    assert_eq!(client.closed_reason(), None);
}
