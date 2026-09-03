//! Settings command handling.
//!
//! Preferences live in Rust for the same reason playback authority does: the shell is a
//! renderer, and persistence should not be reimplemented per platform.

use std::path::PathBuf;

use goosic_protocol::{ErrorObject, RequestPayload, ResponseEnvelope, ResponsePayload};
use goosic_settings::{legacy, SettingsError, SettingsStore};

/// The settings store plus the legacy location, resolved once for the process lifetime.
pub struct Settings {
    store: Option<SettingsStore>,
    /// Why the store is unavailable, when it is. Reported instead of silently defaulting.
    unavailable: Option<SettingsError>,
    legacy_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

impl Settings {
    pub fn new() -> Self {
        let (store, unavailable) = match SettingsStore::default_path()
            .and_then(SettingsStore::open)
        {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            store,
            unavailable,
            legacy_path: legacy::default_store_path(),
        }
    }

    /// Opens a store at an explicit path. Used by tests, which must not touch the real file.
    pub fn at(path: PathBuf, legacy_path: Option<PathBuf>) -> Self {
        match SettingsStore::open(path) {
            Ok(store) => Self {
                store: Some(store),
                unavailable: None,
                legacy_path,
            },
            Err(error) => Self {
                store: None,
                unavailable: Some(error),
                legacy_path,
            },
        }
    }

    fn legacy_available(&self) -> bool {
        self.legacy_path.as_ref().is_some_and(|path| path.is_file())
    }

    fn snapshot_response(&self, request_id: String) -> ResponseEnvelope {
        match &self.store {
            Some(store) => ResponseEnvelope::success(
                request_id,
                ResponsePayload {
                    settings: Some(store.snapshot(self.legacy_available())),
                    ..Default::default()
                },
            ),
            None => self.unavailable_response(request_id),
        }
    }

    fn unavailable_response(&self, request_id: String) -> ResponseEnvelope {
        let (code, message) = match &self.unavailable {
            Some(error) => (error.code(), error.to_string()),
            None => ("settingsUnavailable", "settings are not available".into()),
        };
        failure(request_id, code, message)
    }
}

fn failure(request_id: String, code: &str, message: String) -> ResponseEnvelope {
    ResponseEnvelope::failure(
        request_id,
        ErrorObject {
            code: code.to_owned(),
            message,
        },
    )
}

/// Handles every `settings.*` command. Returns `None` when the command is not one of them.
pub fn handle(
    settings: &mut Settings,
    command: &str,
    request_id: &str,
    payload: &RequestPayload,
) -> Option<ResponseEnvelope> {
    let id = request_id.to_owned();
    Some(match command {
        "settings.get" => settings.snapshot_response(id),
        "settings.set" => {
            let Some(patch) = payload.preferences.clone() else {
                return Some(failure(
                    id,
                    "invalidRequest",
                    "settings.set requires preferences".into(),
                ));
            };
            let legacy_available = settings.legacy_available();
            let Some(store) = settings.store.as_mut() else {
                return Some(settings.unavailable_response(id));
            };
            match store.update(patch) {
                Ok(_) => ResponseEnvelope::success(
                    id,
                    ResponsePayload {
                        settings: Some(store.snapshot(legacy_available)),
                        ..Default::default()
                    },
                ),
                Err(error) => failure(id, error.code(), error.to_string()),
            }
        }
        "settings.importLegacy" => {
            let Some(legacy_path) = settings.legacy_path.clone() else {
                return Some(failure(
                    id,
                    "legacyNotFound",
                    "no legacy Goosic store exists on this platform".into(),
                ));
            };
            let legacy_available = legacy_path.is_file();
            let Some(store) = settings.store.as_mut() else {
                return Some(settings.unavailable_response(id));
            };
            match store.import_legacy(&legacy_path) {
                Ok(_) => ResponseEnvelope::success(
                    id,
                    ResponsePayload {
                        message: Some(format!(
                            "Imported preferences from {}",
                            legacy_path.display()
                        )),
                        settings: Some(store.snapshot(legacy_available)),
                        ..Default::default()
                    },
                ),
                Err(error) => failure(id, error.code(), error.to_string()),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::PreferencesPatch;

    fn settings(directory: &std::path::Path) -> Settings {
        Settings::at(directory.join("settings.json"), None)
    }

    #[test]
    fn non_settings_commands_are_not_claimed() {
        let directory = std::env::temp_dir().join(format!("goosic-settings-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = settings(&directory);
        assert!(handle(&mut store, "playback.claim", "1", &RequestPayload::default()).is_none());
        assert!(handle(&mut store, "catalog.search", "1", &RequestPayload::default()).is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn get_returns_defaults_and_set_persists() {
        let directory = std::env::temp_dir().join(format!("goosic-settings-set-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = settings(&directory);

        let initial = handle(&mut store, "settings.get", "1", &RequestPayload::default()).unwrap();
        let snapshot = initial.payload.unwrap().settings.unwrap();
        assert_eq!(snapshot.volume, 1.0);
        assert!(!snapshot.imported_from_legacy);
        assert!(!snapshot.legacy_available);

        let updated = handle(
            &mut store,
            "settings.set",
            "2",
            &RequestPayload {
                preferences: Some(PreferencesPatch {
                    volume: Some(0.3),
                    last_route: Some("explore".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        let snapshot = updated.payload.unwrap().settings.unwrap();
        assert_eq!(snapshot.volume, 0.3);
        assert_eq!(snapshot.last_route, "explore");

        let reopened = settings(&directory);
        assert_eq!(reopened.store.unwrap().preferences().volume, 0.3);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn set_without_preferences_is_rejected() {
        let directory = std::env::temp_dir().join(format!("goosic-settings-empty-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = settings(&directory);
        let response = handle(&mut store, "settings.set", "1", &RequestPayload::default()).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "invalidRequest");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn importing_without_a_legacy_store_says_so() {
        let directory = std::env::temp_dir().join(format!("goosic-settings-nolegacy-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut store = settings(&directory);
        let response =
            handle(&mut store, "settings.importLegacy", "1", &RequestPayload::default()).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "legacyNotFound");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
