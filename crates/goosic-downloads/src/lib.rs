//! Explicitly downloaded local audio.
//!
//! This crate is the `localDownloadedFile` half of the playback owner rule: files the user
//! already has, played from disk, never through the official player. It does not download
//! anything, and it never touches account cookies — the two paths stay separate on purpose.

pub mod decode;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use goosic_protocol::DownloadedTrack;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LIBRARY_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("{path} could not be read: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} could not be written: {source}")]
    Unwritable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the downloads index is not valid JSON: {0}")]
    Malformed(String),
    #[error("the downloads index was written by a newer version ({0})")]
    TooNew(u32),
    #[error("no downloads location is available on this platform")]
    NoLocation,
    #[error("no downloaded track has the id {0}")]
    UnknownTrack(String),
    #[error("the downloaded file for {0} is missing")]
    FileMissing(String),
    #[error("this file could not be decoded: {0}")]
    Undecodable(String),
    #[error("no previous Goosic downloads were found")]
    NoLegacyDownloads,
}

impl DownloadError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable { .. } | Self::Unwritable { .. } => "downloadsIoError",
            Self::Malformed(_) => "downloadsMalformed",
            Self::TooNew(_) => "downloadsTooNew",
            Self::NoLocation => "downloadsNoLocation",
            Self::UnknownTrack(_) => "downloadUnknown",
            Self::FileMissing(_) => "downloadFileMissing",
            Self::Undecodable(_) => "downloadUndecodable",
            Self::NoLegacyDownloads => "legacyNotFound",
        }
    }
}

/// One downloaded file this library knows about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    /// Where the media actually lives. Imported files are referenced where they are rather than
    /// copied, so an import costs nothing and leaves the previous install intact.
    pub path: PathBuf,
    pub bytes: u64,
    /// Where this record came from, e.g. a previous Goosic install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
}

impl DownloadRecord {
    /// The wire form, with availability resolved now rather than remembered.
    ///
    /// A referenced file can disappear at any time — the previous install may have been removed
    /// — so the shell is told what is true at the moment it asks.
    pub fn to_wire(&self) -> DownloadedTrack {
        DownloadedTrack {
            video_id: self.video_id.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            bytes: self.bytes,
            available: self.path.is_file(),
            imported: self.imported_from.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryDocument {
    version: u32,
    #[serde(default)]
    tracks: Vec<DownloadRecord>,
}

impl Default for LibraryDocument {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            tracks: Vec::new(),
        }
    }
}

/// The index of downloaded tracks, and the cache of decoded audio.
#[derive(Debug)]
pub struct DownloadLibrary {
    index_path: PathBuf,
    cache_directory: PathBuf,
    document: LibraryDocument,
}

impl DownloadLibrary {
    /// The per-user index location for this platform.
    pub fn default_index_path() -> Result<PathBuf, DownloadError> {
        Ok(support_directory()?.join("downloads.json"))
    }

    /// Where decoded audio is cached. Everything here is reproducible from the source files.
    pub fn default_cache_directory() -> Result<PathBuf, DownloadError> {
        Ok(support_directory()?.join("decoded"))
    }

    /// The previous Goosic install's downloaded media directory, if this platform has one.
    pub fn default_legacy_directory() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let candidate = if cfg!(target_os = "macos") {
            Path::new(&home)
                .join("Library/Application Support/com.github.ivasy.ytubic/offline-media/stream")
        } else {
            Path::new(&home).join(".local/share/com.github.ivasy.ytubic/offline-media/stream")
        };
        candidate.is_dir().then_some(candidate)
    }

    pub fn open(index_path: PathBuf, cache_directory: PathBuf) -> Result<Self, DownloadError> {
        let document = match std::fs::read(&index_path) {
            Ok(bytes) => {
                let envelope: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|error| DownloadError::Malformed(error.to_string()))?;
                let version = envelope
                    .get("version")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(LIBRARY_VERSION as u64);
                if version > LIBRARY_VERSION as u64 {
                    return Err(DownloadError::TooNew(version.min(u32::MAX as u64) as u32));
                }
                serde_json::from_value(envelope)
                    .map_err(|error| DownloadError::Malformed(error.to_string()))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                LibraryDocument::default()
            }
            Err(error) => {
                return Err(DownloadError::Unreadable {
                    path: index_path.display().to_string(),
                    source: error,
                })
            }
        };
        Ok(Self {
            index_path,
            cache_directory,
            document,
        })
    }

    pub fn tracks(&self) -> Vec<DownloadedTrack> {
        self.document
            .tracks
            .iter()
            .map(DownloadRecord::to_wire)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.document.tracks.is_empty()
    }

    fn record(&self, video_id: &str) -> Result<&DownloadRecord, DownloadError> {
        self.document
            .tracks
            .iter()
            .find(|track| track.video_id == video_id)
            .ok_or_else(|| DownloadError::UnknownTrack(video_id.to_owned()))
    }

    /// Decodes a downloaded track into playable audio and returns the file to play.
    ///
    /// A previous decode is reused when it is newer than its source, so pressing play twice
    /// costs nothing the second time.
    pub fn prepare(&self, video_id: &str) -> Result<PathBuf, DownloadError> {
        if !is_valid_video_id(video_id) {
            return Err(DownloadError::UnknownTrack(video_id.to_owned()));
        }
        let record = self.record(video_id)?;
        if !record.path.is_file() {
            return Err(DownloadError::FileMissing(video_id.to_owned()));
        }
        let destination = self
            .cache_directory
            .join(format!("{}.wav", sanitize(video_id)));
        if is_fresh(&destination, &record.path) {
            return Ok(destination);
        }
        std::fs::create_dir_all(&self.cache_directory).map_err(|error| {
            DownloadError::Unwritable {
                path: self.cache_directory.display().to_string(),
                source: error,
            }
        })?;
        let audio = decode::decode_opus_webm(&record.path)?;
        decode::write_wav(&audio, &destination)?;
        Ok(destination)
    }

    /// Adds every downloaded track found in a previous Goosic install's media directory.
    ///
    /// Files are referenced where they are: an import copies no audio, and the previous install
    /// is left exactly as it was.
    pub fn import_legacy(&mut self, directory: &Path) -> Result<usize, DownloadError> {
        let entries = std::fs::read_dir(directory).map_err(|error| DownloadError::Unreadable {
            path: directory.display().to_string(),
            source: error,
        })?;

        let mut found: BTreeMap<String, DownloadRecord> = BTreeMap::new();
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // Imports are intentionally read-only and confined to finalized regular files in
            // the legacy directory; a symlink must not make the index point outside that tree.
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("webm") {
                continue;
            }
            let Some(video_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            // Final download names are YouTube video ids. Rejecting every other stem keeps
            // decode-cache names one-to-one and excludes partial/auxiliary legacy files.
            if !is_valid_video_id(video_id) {
                continue;
            }
            if path.with_extension("invalid").is_file() {
                continue;
            }
            let bytes = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            if bytes == 0 {
                continue;
            }
            let (title, artist) = read_sidecar(&path.with_extension("meta.json"))
                .unwrap_or_else(|| (video_id.to_owned(), String::new()));
            found.insert(
                video_id.to_owned(),
                DownloadRecord {
                    video_id: video_id.to_owned(),
                    title,
                    artist,
                    path: path.clone(),
                    bytes,
                    imported_from: Some(directory.display().to_string()),
                },
            );
        }

        if found.is_empty() {
            return Err(DownloadError::NoLegacyDownloads);
        }

        let added = found
            .keys()
            .filter(|id| {
                !self
                    .document
                    .tracks
                    .iter()
                    .any(|track| &&track.video_id == id)
            })
            .count();
        // Re-importing refreshes existing records rather than duplicating them.
        self.document
            .tracks
            .retain(|track| !found.contains_key(&track.video_id));
        self.document.tracks.extend(found.into_values());
        self.document
            .tracks
            .sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
        self.save()?;
        Ok(added)
    }

    fn save(&self) -> Result<(), DownloadError> {
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| DownloadError::Unwritable {
                path: parent.display().to_string(),
                source: error,
            })?;
        }
        let encoded = serde_json::to_vec_pretty(&self.document).map_err(|error| {
            DownloadError::Unwritable {
                path: self.index_path.display().to_string(),
                source: std::io::Error::other(error),
            }
        })?;
        let temporary = self.index_path.with_extension("json.tmp");
        let unwritable = |error: std::io::Error| DownloadError::Unwritable {
            path: self.index_path.display().to_string(),
            source: error,
        };
        std::fs::write(&temporary, &encoded).map_err(unwritable)?;
        std::fs::rename(&temporary, &self.index_path).map_err(unwritable)?;
        Ok(())
    }
}

fn support_directory() -> Result<PathBuf, DownloadError> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| Path::new(&home).join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
    };
    base.map(|base| base.join("goosic"))
        .ok_or(DownloadError::NoLocation)
}

/// A cached decode is usable when it exists and is not older than its source.
fn is_fresh(cached: &Path, source: &Path) -> bool {
    let Ok(cached_time) = std::fs::metadata(cached).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let Ok(source_time) = std::fs::metadata(source).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    cached_time >= source_time
}

/// Keeps an identifier usable as a file name.
///
/// Video ids are already restricted, but the index is a plain JSON file a person can edit, and
/// a cache path must never be able to escape its directory.
fn sanitize(video_id: &str) -> String {
    video_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn is_valid_video_id(video_id: &str) -> bool {
    video_id.len() == 11
        && video_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Reads a legacy `{"title":…,"artist":…}` sidecar.
fn read_sidecar(path: &Path) -> Option<(String, String)> {
    // `metadata` follows symlinks; `symlink_metadata` does not. Only a regular sidecar that is
    // physically inside the enumerated legacy directory may contribute display metadata.
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let title = value.get("title")?.as_str()?.to_owned();
    let artist = value
        .get("artist")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some((title, artist))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(directory: &Path) -> DownloadLibrary {
        DownloadLibrary::open(directory.join("downloads.json"), directory.join("decoded")).unwrap()
    }

    fn legacy_media(directory: &Path) -> PathBuf {
        let media = directory.join("stream");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("abcdefghijk.webm"), b"pretend media bytes").unwrap();
        std::fs::write(
            media.join("abcdefghijk.meta.json"),
            br#"{"title":"Nonsense","artist":"Sabrina Carpenter"}"#,
        )
        .unwrap();
        // A file with no sidecar, and an empty one that should be skipped.
        std::fs::write(media.join("nosidecar12.webm"), b"more bytes").unwrap();
        std::fs::write(media.join("empty000000.webm"), b"").unwrap();
        media
    }

    #[test]
    fn an_absent_index_opens_empty_and_writes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let opened = library(directory.path());
        assert!(opened.is_empty());
        assert!(!directory.path().join("downloads.json").exists());
    }

    #[test]
    fn importing_reads_sidecars_and_skips_empty_files() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        let mut opened = library(directory.path());
        assert_eq!(opened.import_legacy(&media).unwrap(), 2);

        let tracks = opened.tracks();
        assert_eq!(tracks.len(), 2);
        let named = tracks
            .iter()
            .find(|track| track.video_id == "abcdefghijk")
            .unwrap();
        assert_eq!(named.title, "Nonsense");
        assert_eq!(named.artist, "Sabrina Carpenter");
        assert!(named.available);
        assert!(named.imported);

        let unnamed = tracks
            .iter()
            .find(|track| track.video_id == "nosidecar12")
            .unwrap();
        assert_eq!(
            unnamed.title, "nosidecar12",
            "a missing sidecar falls back to the id"
        );
    }

    #[test]
    fn importing_references_files_in_place_and_leaves_them_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        let before: Vec<_> = std::fs::read_dir(&media)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        let mut opened = library(directory.path());
        opened.import_legacy(&media).unwrap();

        let after: Vec<_> = std::fs::read_dir(&media)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(
            before.len(),
            after.len(),
            "the legacy directory is unchanged"
        );
        let index = std::fs::read_to_string(directory.path().join("downloads.json")).unwrap();
        assert!(
            index.contains("stream/abcdefghijk.webm"),
            "files are referenced in place"
        );
    }

    #[test]
    fn re_importing_refreshes_records_rather_than_duplicating_them() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        let mut opened = library(directory.path());
        assert_eq!(opened.import_legacy(&media).unwrap(), 2);
        assert_eq!(
            opened.import_legacy(&media).unwrap(),
            0,
            "nothing is newly added"
        );
        assert_eq!(opened.tracks().len(), 2);
    }

    #[test]
    fn a_missing_source_file_shows_as_unavailable_rather_than_disappearing() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        let mut opened = library(directory.path());
        opened.import_legacy(&media).unwrap();
        std::fs::remove_file(media.join("abcdefghijk.webm")).unwrap();

        let track = opened
            .tracks()
            .into_iter()
            .find(|track| track.video_id == "abcdefghijk")
            .expect("the record is kept");
        assert!(!track.available);
    }

    #[test]
    fn preparing_an_unknown_or_missing_track_reports_which() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        let mut opened = library(directory.path());
        opened.import_legacy(&media).unwrap();

        let unknown = opened.prepare("not-a-track").unwrap_err();
        assert_eq!(unknown.code(), "downloadUnknown");

        std::fs::remove_file(media.join("abcdefghijk.webm")).unwrap();
        let missing = opened.prepare("abcdefghijk").unwrap_err();
        assert_eq!(missing.code(), "downloadFileMissing");
    }

    #[test]
    fn importing_from_a_directory_without_media_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let mut opened = library(directory.path());
        let error = opened.import_legacy(&empty).map(|_| ()).unwrap_err();
        assert_eq!(error.code(), "legacyNotFound");
        assert!(!directory.path().join("downloads.json").exists());
    }

    #[test]
    fn a_cache_name_can_never_escape_the_cache_directory() {
        assert_eq!(sanitize("../../etc/passwd"), "______etc_passwd");
        assert!(!sanitize("../../etc/passwd").contains('/'));
        assert!(!sanitize("../../etc/passwd").contains('.'));
        assert_eq!(sanitize("abcdefghijk"), "abcdefghijk");
        assert_eq!(sanitize("-Kr_C-gq"), "-Kr_C-gq");
    }

    #[test]
    fn import_skips_non_youtube_file_stems() {
        let directory = tempfile::tempdir().unwrap();
        let media = legacy_media(directory.path());
        std::fs::write(media.join("a.b.webm"), b"not a video id").unwrap();
        std::fs::write(media.join("a?b.webm"), b"also not a video id").unwrap();
        let mut opened = library(directory.path());
        opened.import_legacy(&media).unwrap();
        assert!(opened
            .tracks()
            .iter()
            .all(|track| is_valid_video_id(&track.video_id)));
        assert!(opened.prepare("a.b").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn import_does_not_follow_a_symlinked_sidecar() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let media = directory.path().join("stream");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("abcdefghijk.webm"), b"media").unwrap();
        let outside = directory.path().join("outside.json");
        std::fs::write(&outside, br#"{"title":"Outside","artist":"Secret"}"#).unwrap();
        symlink(&outside, media.join("abcdefghijk.meta.json")).unwrap();

        let mut opened = library(directory.path());
        opened.import_legacy(&media).unwrap();
        let track = opened.tracks().into_iter().next().unwrap();
        assert_eq!(track.title, "abcdefghijk");
        assert!(track.artist.is_empty());
    }

    #[test]
    fn a_newer_index_is_refused_rather_than_misread() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("downloads.json");
        std::fs::write(&path, r#"{"version":99,"tracks":[]}"#).unwrap();
        let error = DownloadLibrary::open(path, directory.path().join("decoded"))
            .map(|_| ())
            .unwrap_err();
        assert!(matches!(error, DownloadError::TooNew(99)));
    }
}
