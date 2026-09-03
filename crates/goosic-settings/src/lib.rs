//! Durable preferences for the Goosic shell.
//!
//! The store is deliberately small and typed: the shell asks Rust what the preferences are and
//! tells Rust when they change, so persistence works the same way on every platform and the
//! shell stays a renderer.
//!
//! Legacy values are kept verbatim alongside the typed preferences. Nothing the previous app
//! stored is renamed or discarded, so an import stays reversible — with one exception, stated
//! plainly: credentials are never carried over. See [`legacy`].

pub mod legacy;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub use legacy::LegacyPreferences;

/// The document format version. A newer document is refused rather than misread.
pub const DOCUMENT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings file could not be read: {0}")]
    Unreadable(#[source] std::io::Error),
    #[error("settings file could not be written: {0}")]
    Unwritable(#[source] std::io::Error),
    #[error("settings file is not valid JSON: {0}")]
    Malformed(String),
    #[error("settings file was written by a newer version ({0})")]
    TooNew(u32),
    #[error("no settings location is available on this platform")]
    NoLocation,
    #[error("legacy store could not be read: {0}")]
    LegacyUnreadable(#[source] std::io::Error),
    #[error("legacy store could not be queried: {0}")]
    LegacyQuery(String),
    #[error("no legacy Goosic preferences were found")]
    NoLegacyStore,
}

impl SettingsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable(_) | Self::Unwritable(_) => "settingsIoError",
            Self::Malformed(_) => "settingsMalformed",
            Self::TooNew(_) => "settingsTooNew",
            Self::NoLocation => "settingsNoLocation",
            Self::LegacyUnreadable(_) | Self::LegacyQuery(_) => "legacyUnreadable",
            Self::NoLegacyStore => "legacyNotFound",
        }
    }
}

/// The preferences this build actually acts on.
///
/// Anything the previous app stored that has no home here yet is preserved in
/// [`SettingsDocument::legacy`] rather than being invented into this struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// `system`, `light`, or `dark`.
    pub theme: String,
    pub volume: f64,
    pub muted: bool,
    pub autoplay: bool,
    pub last_route: String,
    pub queue_visible: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            volume: 1.0,
            muted: false,
            autoplay: true,
            last_route: "home".into(),
            queue_visible: false,
        }
    }
}

impl Preferences {
    /// Brings a decoded document back into range.
    ///
    /// A settings file is user-editable and may also come from an older build, so values are
    /// corrected here rather than trusted.
    pub fn sanitized(mut self) -> Self {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if self.last_route.trim().is_empty() {
            self.last_route = "home".into();
        }
        self
    }
}

pub use goosic_protocol::PreferencesPatch;

/// Applies a patch to preferences. Absent fields are left as they are.
fn apply_patch(patch: PreferencesPatch, preferences: &mut Preferences) {
    if let Some(theme) = patch.theme {
        preferences.theme = theme;
    }
    if let Some(volume) = patch.volume {
        preferences.volume = volume;
    }
    if let Some(muted) = patch.muted {
        preferences.muted = muted;
    }
    if let Some(autoplay) = patch.autoplay {
        preferences.autoplay = autoplay;
    }
    if let Some(route) = patch.last_route {
        preferences.last_route = route;
    }
    if let Some(visible) = patch.queue_visible {
        preferences.queue_visible = visible;
    }
}

/// What is written to disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDocument {
    pub version: u32,
    #[serde(default)]
    pub preferences: Preferences,
    /// Legacy preference documents, under their original key names, exactly as stored — minus
    /// credentials. Keeping them makes the import reversible and means a later build can adopt
    /// a setting this one has no home for.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub legacy: serde_json::Map<String, Value>,
    /// Where the legacy values came from, if they were ever imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            preferences: Preferences::default(),
            legacy: serde_json::Map::new(),
            imported_from: None,
        }
    }
}

/// The settings file, and the in-memory document it holds.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    document: SettingsDocument,
}

impl SettingsStore {
    /// The per-user settings file for this platform.
    pub fn default_path() -> Result<PathBuf, SettingsError> {
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| Path::new(&home).join("Library/Application Support"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| {
                std::env::var_os("HOME").map(|home| Path::new(&home).join(".config"))
            })
        };
        base.map(|base| base.join("goosic").join("settings.json"))
            .ok_or(SettingsError::NoLocation)
    }

    /// Opens the store at `path`, creating nothing until something is saved.
    pub fn open(path: PathBuf) -> Result<Self, SettingsError> {
        let document = match std::fs::read(&path) {
            Ok(bytes) => {
                // The version is read from a permissive envelope first. A document from a newer
                // build may have a shape this one cannot decode at all, and "too new" is a far
                // more useful thing to report than "malformed".
                let envelope: Value = serde_json::from_slice(&bytes)
                    .map_err(|error| SettingsError::Malformed(error.to_string()))?;
                let version = envelope
                    .get("version")
                    .and_then(Value::as_u64)
                    .unwrap_or(DOCUMENT_VERSION as u64);
                if version > DOCUMENT_VERSION as u64 {
                    return Err(SettingsError::TooNew(version.min(u32::MAX as u64) as u32));
                }
                let document: SettingsDocument = serde_json::from_value(envelope)
                    .map_err(|error| SettingsError::Malformed(error.to_string()))?;
                SettingsDocument {
                    preferences: document.preferences.sanitized(),
                    ..document
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SettingsDocument::default()
            }
            Err(error) => return Err(SettingsError::Unreadable(error)),
        };
        Ok(Self { path, document })
    }

    pub fn document(&self) -> &SettingsDocument {
        &self.document
    }

    pub fn preferences(&self) -> &Preferences {
        &self.document.preferences
    }

    pub fn has_imported(&self) -> bool {
        self.document.imported_from.is_some()
    }

    /// The preferences as the protocol carries them.
    pub fn snapshot(&self, legacy_available: bool) -> goosic_protocol::SettingsSnapshot {
        let preferences = &self.document.preferences;
        goosic_protocol::SettingsSnapshot {
            theme: preferences.theme.clone(),
            volume: preferences.volume,
            muted: preferences.muted,
            autoplay: preferences.autoplay,
            last_route: preferences.last_route.clone(),
            queue_visible: preferences.queue_visible,
            imported_from_legacy: self.has_imported(),
            legacy_available,
        }
    }

    /// Applies a patch and writes the result.
    pub fn update(&mut self, patch: PreferencesPatch) -> Result<&Preferences, SettingsError> {
        apply_patch(patch, &mut self.document.preferences);
        self.document.preferences = self.document.preferences.clone().sanitized();
        self.save()?;
        Ok(&self.document.preferences)
    }

    /// Imports preferences from a legacy store.
    ///
    /// Legacy documents are kept verbatim under their original key names, and the values this
    /// build understands are adopted. Nothing is written unless the whole import succeeds, and
    /// the legacy store itself is never modified.
    pub fn import_legacy(&mut self, database: &Path) -> Result<&Preferences, SettingsError> {
        let legacy = LegacyPreferences::read(database)?;
        if legacy.is_empty() {
            return Err(SettingsError::NoLegacyStore);
        }

        let mut preferences = self.document.preferences.clone();
        if let Some(theme) = legacy.string("ytm-theme") {
            preferences.theme = theme.to_owned();
        }
        if let Some(playback) = legacy.state("ytm-playback") {
            if let Some(volume) = playback.get("volume").and_then(Value::as_f64) {
                preferences.volume = volume;
            }
            if let Some(muted) = playback.get("muted").and_then(Value::as_bool) {
                preferences.muted = muted;
            }
            // The legacy "auto radio" setting is the same intent as autoplay: keep going when a
            // track finishes.
            if let Some(auto) = playback.get("autoRadio").and_then(Value::as_bool) {
                preferences.autoplay = auto;
            }
        }

        let mut kept = serde_json::Map::new();
        for (key, value) in &legacy.values {
            kept.insert(key.clone(), value.clone());
        }

        self.document.preferences = preferences.sanitized();
        self.document.legacy = kept;
        self.document.imported_from = Some(database.display().to_string());
        self.save()?;
        Ok(&self.document.preferences)
    }

    /// Writes the document atomically, so an interrupted write cannot truncate the settings.
    fn save(&self) -> Result<(), SettingsError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(SettingsError::Unwritable)?;
        }
        let encoded =
            serde_json::to_vec_pretty(&self.document).map_err(|error| SettingsError::Unwritable(std::io::Error::other(error)))?;
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, &encoded).map_err(SettingsError::Unwritable)?;
        std::fs::rename(&temporary, &self.path).map_err(SettingsError::Unwritable)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(directory: &Path) -> SettingsStore {
        SettingsStore::open(directory.join("settings.json")).unwrap()
    }

    #[test]
    fn a_missing_file_opens_as_defaults_and_writes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opened = SettingsStore::open(path.clone()).unwrap();
        assert_eq!(opened.preferences(), &Preferences::default());
        assert!(!path.exists(), "opening must not create the file");
    }

    #[test]
    fn an_update_round_trips_through_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let mut first = store(directory.path());
        first
            .update(PreferencesPatch {
                volume: Some(0.25),
                muted: Some(true),
                last_route: Some("charts".into()),
                ..Default::default()
            })
            .unwrap();

        let second = store(directory.path());
        assert_eq!(second.preferences().volume, 0.25);
        assert!(second.preferences().muted);
        assert_eq!(second.preferences().last_route, "charts");
        assert!(second.preferences().autoplay, "untouched fields survive");
    }

    #[test]
    fn out_of_range_values_are_corrected_rather_than_trusted() {
        let directory = tempfile::tempdir().unwrap();
        let mut opened = store(directory.path());
        let corrected = opened
            .update(PreferencesPatch {
                volume: Some(9.0),
                theme: Some("neon".into()),
                last_route: Some("   ".into()),
                ..Default::default()
            })
            .unwrap()
            .clone();
        assert_eq!(corrected.volume, 1.0);
        assert_eq!(corrected.theme, "system");
        assert_eq!(corrected.last_route, "home");
    }

    #[test]
    fn a_hand_edited_file_with_an_impossible_volume_is_corrected_on_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"version":1,"preferences":{"theme":"dark","volume":-4.0,"muted":false,"autoplay":true,"lastRoute":"home","queueVisible":false}}"#,
        )
        .unwrap();
        let opened = SettingsStore::open(path).unwrap();
        assert_eq!(opened.preferences().volume, 0.0);
        assert_eq!(opened.preferences().theme, "dark");
    }

    #[test]
    fn a_newer_document_is_refused_rather_than_misread() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"version":99,"preferences":{}}"#).unwrap();
        let error = SettingsStore::open(path).map(|_| ()).unwrap_err();
        assert!(matches!(error, SettingsError::TooNew(99)));
    }

    #[test]
    fn a_corrupt_file_is_reported_and_not_silently_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "not json at all").unwrap();
        let error = SettingsStore::open(path.clone()).unwrap_err();
        assert_eq!(error.code(), "settingsMalformed");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json at all");
    }

    fn legacy_database(directory: &Path) -> PathBuf {
        let path = directory.join("localstorage.sqlite3");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB)", [])
            .unwrap();
        let utf16 = |text: &str| -> Vec<u8> {
            text.encode_utf16().flat_map(u16::to_le_bytes).collect()
        };
        for (key, value) in [
            ("ytm-theme", utf16("dark")),
            (
                "ytm-playback",
                utf16(r#"{"state":{"volume":0.4,"muted":true,"autoRadio":false},"version":2}"#),
            ),
            (
                "ytm-layout",
                utf16(r#"{"state":{"mode":"bottom","floatingPinned":false},"version":0}"#),
            ),
            (
                "ytm-settings",
                utf16(r#"{"state":{"closeAction":"tray","lastfmSessionKey":"secret"},"version":3}"#),
            ),
        ] {
            connection
                .execute("INSERT INTO ItemTable (key, value) VALUES (?1, ?2)", (key, value))
                .unwrap();
        }
        path
    }

    #[test]
    fn importing_adopts_understood_values_and_keeps_the_rest_verbatim() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = legacy_database(directory.path());
        let mut opened = store(directory.path());
        let imported = opened.import_legacy(&legacy).unwrap().clone();

        assert_eq!(imported.theme, "dark");
        assert_eq!(imported.volume, 0.4);
        assert!(imported.muted);
        assert!(!imported.autoplay, "legacy auto-radio maps onto autoplay");

        let kept = &opened.document().legacy;
        assert_eq!(kept["ytm-layout"]["state"]["mode"], "bottom");
        assert!(opened.has_imported());
    }

    #[test]
    fn an_import_never_carries_a_credential_into_the_settings_file() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = legacy_database(directory.path());
        let mut opened = store(directory.path());
        opened.import_legacy(&legacy).unwrap();

        let written = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
        assert!(written.contains("closeAction"), "preferences are kept");
        assert!(
            !written.contains("lastfmSessionKey") && !written.contains("secret"),
            "a session key must never reach the settings file: {written}"
        );
    }

    #[test]
    fn importing_from_a_store_without_goosic_keys_reports_that_rather_than_writing_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.sqlite3");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB)", [])
            .unwrap();
        let mut opened = store(directory.path());
        let error = opened.import_legacy(&path).map(|_| ()).unwrap_err();
        assert!(matches!(error, SettingsError::NoLegacyStore));
        assert!(!directory.path().join("settings.json").exists());
    }
}
