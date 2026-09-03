//! Reads this machine's real legacy Goosic store, if one exists.
//!
//! `#[ignore]`d because it depends on the machine. It writes only to a temporary settings file,
//! never to the real one, and never modifies the legacy store.
//!
//! ```sh
//! cargo test -p goosic-settings -- --ignored --nocapture
//! ```

use goosic_settings::{legacy, SettingsStore};

#[test]
#[ignore = "depends on a legacy Goosic install being present on this machine"]
fn the_real_legacy_store_imports_without_carrying_secrets() {
    let Some(database) = legacy::default_store_path() else {
        println!("no legacy store location on this platform");
        return;
    };
    if !database.is_file() {
        println!("no legacy store at {}", database.display());
        return;
    }
    println!("legacy store: {}", database.display());

    let directory = tempfile::tempdir().unwrap();
    let mut store = SettingsStore::open(directory.path().join("settings.json")).unwrap();
    let preferences = store.import_legacy(&database).unwrap().clone();
    println!(
        "imported theme={} volume={} muted={} autoplay={}",
        preferences.theme, preferences.volume, preferences.muted, preferences.autoplay
    );
    println!(
        "legacy documents kept: {:?}",
        store.document().legacy.keys().collect::<Vec<_>>()
    );

    let written = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
    for secret in legacy::CREDENTIAL_FIELDS.iter().chain(legacy::CREDENTIAL_KEYS.iter()) {
        assert!(
            !written.contains(secret),
            "the settings file must never contain `{secret}`"
        );
    }
    println!("settings file is {} bytes and carries no credential fields", written.len());
}
