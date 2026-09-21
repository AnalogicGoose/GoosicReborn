//! Replaying the exchange fixtures against the real service.
//!
//! `goosic-protocol` owns the conversations because it owns the wire, but it cannot run them:
//! it does not know what a service is. This is the other half. Each exchange starts from an
//! authority that has just been constructed, and every request must produce byte for byte the
//! response recorded beside it — which is what makes the fixtures a description a second
//! implementation can be held to rather than a description of whatever Rust happens to do.

use goosic_catalog::Catalog;
use goosic_core::PlaybackAuthority;
use goosic_lyrics::LyricsClient;
use goosic_protocol::conformance::exchange_cases;
use goosic_protocol::RequestEnvelope;
use goosic_service::{accounts, downloads, handle_request, settings};

/// A throwaway directory per exchange, so replaying one never reads or writes real user data
/// and two exchanges cannot see each other's state.
fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "goosic-exchange-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        name.replace('/', "-")
    ))
}

#[test]
fn every_scripted_conversation_produces_the_recorded_answers() {
    let exchanges = exchange_cases();
    assert!(!exchanges.is_empty(), "the exchange fixtures are empty");

    for exchange in exchanges {
        let mut authority = PlaybackAuthority::new();
        let catalog = Catalog::new();
        let base = scratch(&exchange.name);
        let mut settings = settings::Settings::at(base.join("settings.json"), None);
        let mut downloads =
            downloads::Downloads::at(base.join("downloads.json"), base.join("decoded"), None);
        let mut accounts = accounts::Accounts::at(base.join("accounts.json"));
        let lyrics = LyricsClient::new();

        for (position, step) in exchange.steps.iter().enumerate() {
            let request: RequestEnvelope = serde_json::from_str(&step.request)
                .unwrap_or_else(|error| panic!("{}: request did not decode: {error}", exchange.name));

            let response = handle_request(
                &mut authority,
                &catalog,
                &mut settings,
                &mut downloads,
                &mut accounts,
                &lyrics,
                request
            );
            let actual = serde_json::to_string(&response).expect("response did not encode");

            assert_eq!(
                actual, step.response,
                "\n{} step {}: the service answered differently.\nwhy this step exists: {}\nwhy this exchange exists: {}\n",
                exchange.name,
                position + 1,
                step.why,
                exchange.why
            );
        }
    }
}