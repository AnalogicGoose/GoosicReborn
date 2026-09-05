//! Reading the previous Goosic's stored preferences.
//!
//! The legacy app kept its preferences in the web view's `localStorage`, which on macOS and
//! Linux is a WebKit SQLite database. Nothing here writes to that store: the files are copied
//! before they are opened, so a failed or partial import cannot damage the old app's state.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::SettingsError;

/// The preference keys the migration carries over. Everything else in the legacy store is
/// either cache, or something this build has no home for yet.
pub const MIGRATED_KEYS: [&str; 6] = [
    "ytm-theme",
    "ytm-settings",
    "ytm-layout",
    "ytm-track-source",
    "ytm-playback",
    "ytm:lyrics-source",
];

/// Fields that are credentials rather than preferences. They are stripped from imported values
/// and never written to the new store, whatever the legacy document contained.
///
/// The legacy `ytm-settings` document holds a Last.fm session key inline, and the store also
/// holds a Musixmatch token under its own key. Neither belongs in a settings file.
pub const CREDENTIAL_FIELDS: [&str; 6] = [
    "lastfmSessionKey",
    "sessionKey",
    "token",
    "accessToken",
    "refreshToken",
    "cookie",
];

/// Keys that are never read at all, because their whole purpose is to hold a secret.
pub const CREDENTIAL_KEYS: [&str; 3] = [
    "musixmatch-user-token",
    "ytm-visitor-data",
    "goosic-premium-entitlements-v1",
];

/// The default location of the legacy web view's local storage on this platform.
///
/// Windows is absent on purpose: WebView2 stores local storage in a LevelDB directory, not a
/// SQLite database, and no reader for it exists here yet.
pub fn default_store_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let root =
            Path::new(&home).join("Library/WebKit/com.github.ivasy.ytubic/WebsiteData/Default");
        return first_local_storage_database(&root);
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME")?;
        let root = Path::new(&home).join(".local/share/com.github.ivasy.ytubic");
        return first_local_storage_database(&root);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Finds `LocalStorage/localstorage.sqlite3` under an origin-hashed directory tree.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn first_local_storage_database(root: &Path) -> Option<PathBuf> {
    fn search(directory: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 6 {
            return None;
        }
        let candidate = directory.join("LocalStorage/localstorage.sqlite3");
        if candidate.is_file() {
            return Some(candidate);
        }
        for entry in std::fs::read_dir(directory).ok()?.flatten() {
            if entry.file_type().ok()?.is_dir() {
                if let Some(found) = search(&entry.path(), depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    search(root, 0)
}

/// Removes credential-shaped fields from an imported value, at any depth.
fn strip_credentials(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !CREDENTIAL_FIELDS.contains(&key.as_str()));
            for child in map.values_mut() {
                strip_credentials(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_credentials(item);
            }
        }
        _ => {}
    }
}

/// Decodes one stored value.
///
/// WebKit stores `localStorage` strings as UTF-16LE blobs. Values are JSON documents in most
/// cases and bare strings in a few (`ytm-theme` is just `dark`), so both are accepted.
fn decode(raw: &[u8]) -> Option<Value> {
    let text = if raw.len() >= 2 && raw.len() % 2 == 0 && raw[1] == 0 {
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).ok()?
    } else {
        String::from_utf8(raw.to_vec()).ok()?
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_owned())))
}

/// Everything the migration is willing to read out of a legacy store.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LegacyPreferences {
    pub values: BTreeMap<String, Value>,
}

impl LegacyPreferences {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Reads a legacy store without modifying it.
    ///
    /// The database and any write-ahead log are copied first, because opening a WAL database in
    /// place can create or update sidecar files next to the original.
    pub fn read(database: &Path) -> Result<Self, SettingsError> {
        let staging = tempdir()?;
        let copy = staging.join("localstorage.sqlite3");
        std::fs::copy(database, &copy).map_err(SettingsError::LegacyUnreadable)?;
        for suffix in ["-wal", "-shm"] {
            let sidecar = with_suffix(database, suffix);
            if sidecar.is_file() {
                let _ = std::fs::copy(&sidecar, with_suffix(&copy, suffix));
            }
        }

        let result = Self::read_copied(&copy);
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    fn read_copied(database: &Path) -> Result<Self, SettingsError> {
        let connection = rusqlite::Connection::open(database)
            .map_err(|error| SettingsError::LegacyQuery(error.to_string()))?;
        let mut statement = connection
            .prepare("SELECT key, value FROM ItemTable")
            .map_err(|error| SettingsError::LegacyQuery(error.to_string()))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|error| SettingsError::LegacyQuery(error.to_string()))?;

        let mut values = BTreeMap::new();
        for row in rows.flatten() {
            let (key, raw) = row;
            if CREDENTIAL_KEYS.contains(&key.as_str()) || !MIGRATED_KEYS.contains(&key.as_str()) {
                continue;
            }
            let Some(mut value) = decode(&raw) else {
                continue;
            };
            strip_credentials(&mut value);
            values.insert(key, value);
        }
        Ok(Self { values })
    }

    /// Unwraps the legacy store-persistence envelope, `{"state": {...}, "version": n}`.
    pub fn state(&self, key: &str) -> Option<&Value> {
        let value = self.values.get(key)?;
        value.get("state").or(Some(value))
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.values.get(key)?.as_str()
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Distinguishes staging directories created within one process.
///
/// A timestamp alone is not enough: two threads can read the clock in the same tick, and
/// `create_dir_all` succeeds on a directory that already exists — so both would share one
/// staging directory and the first to finish would delete the other's copy mid-read.
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Creates a private staging directory for one read.
///
/// `create_dir` rather than `create_dir_all`, so an unexpected collision is an error instead of
/// two callers silently sharing a directory.
fn tempdir() -> Result<PathBuf, SettingsError> {
    let base = std::env::temp_dir().join(format!(
        "goosic-legacy-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default(),
        STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&base).map_err(SettingsError::LegacyUnreadable)?;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn legacy_database(directory: &Path) -> PathBuf {
        let path = directory.join("localstorage.sqlite3");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB)", [])
            .unwrap();
        let rows: [(&str, Vec<u8>); 5] = [
            ("ytm-theme", utf16("dark")),
            (
                "ytm-playback",
                utf16(r#"{"state":{"volume":0.4,"muted":false,"autoRadio":true},"version":2}"#),
            ),
            (
                "ytm-settings",
                utf16(
                    r#"{"state":{"closeAction":"tray","lastfmSessionKey":"secret-session"},"version":3}"#,
                ),
            ),
            ("musixmatch-user-token", utf16("secret-token")),
            ("ytubic-query-cache", utf16(r#"{"huge":true}"#)),
        ];
        for (key, value) in rows {
            connection
                .execute(
                    "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                    (key, value),
                )
                .unwrap();
        }
        path
    }

    #[test]
    fn utf16_and_bare_string_values_both_decode() {
        assert_eq!(decode(&utf16("dark")), Some(json!("dark")));
        assert_eq!(decode(&utf16(r#"{"a":1}"#)), Some(json!({"a": 1})));
        assert_eq!(decode(b"plain"), Some(json!("plain")));
        assert_eq!(decode(&[]), None);
    }

    #[test]
    fn credential_fields_are_stripped_at_any_depth() {
        let mut value = json!({
            "state": {"closeAction": "tray", "lastfmSessionKey": "secret"},
            "nested": [{"token": "secret", "keep": 1}]
        });
        strip_credentials(&mut value);
        assert_eq!(value["state"]["closeAction"], "tray");
        assert!(value["state"].get("lastfmSessionKey").is_none());
        assert!(value["nested"][0].get("token").is_none());
        assert_eq!(value["nested"][0]["keep"], 1);
    }

    #[test]
    fn reading_a_store_takes_preferences_and_leaves_secrets_and_caches_behind() {
        let directory = tempfile::tempdir().unwrap();
        let database = legacy_database(directory.path());
        let legacy = LegacyPreferences::read(&database).unwrap();

        assert_eq!(legacy.string("ytm-theme"), Some("dark"));
        assert_eq!(legacy.state("ytm-playback").unwrap()["volume"], 0.4);
        assert_eq!(legacy.state("ytm-settings").unwrap()["closeAction"], "tray");

        assert!(legacy.values.contains_key("ytm-settings"));
        assert!(
            legacy
                .state("ytm-settings")
                .unwrap()
                .get("lastfmSessionKey")
                .is_none(),
            "a session key must never be carried into the new store"
        );
        assert!(!legacy.values.contains_key("musixmatch-user-token"));
        assert!(!legacy.values.contains_key("ytubic-query-cache"));
    }

    #[test]
    fn reading_does_not_modify_the_legacy_store() {
        let directory = tempfile::tempdir().unwrap();
        let database = legacy_database(directory.path());
        let before = std::fs::read(&database).unwrap();
        let modified_before = std::fs::metadata(&database).unwrap().modified().unwrap();

        LegacyPreferences::read(&database).unwrap();

        assert_eq!(std::fs::read(&database).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&database).unwrap().modified().unwrap(),
            modified_before
        );
    }

    #[test]
    fn concurrent_reads_do_not_share_a_staging_directory() {
        // Two threads reading at once used to be able to land on the same staging path, and the
        // first to finish deleted the other's copy out from under it.
        let directory = tempfile::tempdir().unwrap();
        let database = legacy_database(directory.path());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let database = database.clone();
                std::thread::spawn(move || LegacyPreferences::read(&database))
            })
            .collect();
        for handle in handles {
            let legacy = handle.join().expect("no thread panicked").expect("read succeeds");
            assert_eq!(legacy.string("ytm-theme"), Some("dark"));
        }
    }

    #[test]
    fn each_read_gets_its_own_staging_directory() {
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();
        assert_ne!(first, second);
        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }

    #[test]
    fn a_missing_store_is_an_error_rather_than_a_silent_empty_import() {
        let directory = tempfile::tempdir().unwrap();
        let error = LegacyPreferences::read(&directory.path().join("absent.sqlite3")).unwrap_err();
        assert!(matches!(error, SettingsError::LegacyUnreadable(_)));
    }
}
