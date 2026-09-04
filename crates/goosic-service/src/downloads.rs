//! Download command handling.
//!
//! Downloads are the `localDownloadedFile` half of the ownership rule. This module only reads
//! and decodes files the user already has; it never downloads anything and never touches
//! account cookies, so the local path and the official path stay separate.

use std::path::PathBuf;

use goosic_downloads::{DownloadError, DownloadLibrary};
use goosic_protocol::{
    ErrorObject, Owner, PlaybackState, RequestPayload, ResponseEnvelope, ResponsePayload,
};

/// The download index, plus where a previous install's media lives.
pub struct Downloads {
    library: Option<DownloadLibrary>,
    unavailable: Option<DownloadError>,
    legacy_directory: Option<PathBuf>,
}

impl Default for Downloads {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloads {
    pub fn new() -> Self {
        let opened = DownloadLibrary::default_index_path().and_then(|index| {
            DownloadLibrary::default_cache_directory()
                .and_then(|cache| DownloadLibrary::open(index, cache))
        });
        let (library, unavailable) = match opened {
            Ok(library) => (Some(library), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            library,
            unavailable,
            legacy_directory: DownloadLibrary::default_legacy_directory(),
        }
    }

    /// Opens a library at explicit paths, so tests never touch the real index.
    pub fn at(index: PathBuf, cache: PathBuf, legacy_directory: Option<PathBuf>) -> Self {
        match DownloadLibrary::open(index, cache) {
            Ok(library) => Self {
                library: Some(library),
                unavailable: None,
                legacy_directory,
            },
            Err(error) => Self {
                library: None,
                unavailable: Some(error),
                legacy_directory,
            },
        }
    }

    fn unavailable_response(&self, request_id: String) -> ResponseEnvelope {
        let (code, message) = match &self.unavailable {
            Some(error) => (error.code(), error.to_string()),
            None => ("downloadsUnavailable", "downloads are not available".into()),
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

/// Handles every `downloads.*` command. Returns `None` when the command is not one of them.
pub fn handle(
    downloads: &mut Downloads,
    playback: &PlaybackState,
    command: &str,
    request_id: &str,
    payload: &RequestPayload,
) -> Option<ResponseEnvelope> {
    let id = request_id.to_owned();
    Some(match command {
        "downloads.list" => {
            let Some(library) = downloads.library.as_ref() else {
                return Some(downloads.unavailable_response(id));
            };
            ResponseEnvelope::success(
                id,
                ResponsePayload {
                    downloads: Some(library.tracks()),
                    ..Default::default()
                },
            )
        }
        "downloads.importLegacy" => {
            let Some(directory) = downloads.legacy_directory.clone() else {
                return Some(failure(
                    id,
                    "legacyNotFound",
                    "no previous Goosic downloads exist on this machine".into(),
                ));
            };
            let Some(library) = downloads.library.as_mut() else {
                return Some(downloads.unavailable_response(id));
            };
            match library.import_legacy(&directory) {
                Ok(added) => ResponseEnvelope::success(
                    id,
                    ResponsePayload {
                        message: Some(format!(
                            "Added {added} track(s) from {}",
                            directory.display()
                        )),
                        downloads: Some(library.tracks()),
                        ..Default::default()
                    },
                ),
                Err(error) => failure(id, error.code(), error.to_string()),
            }
        }
        "downloads.prepare" => {
            let Some(video_id) = payload
                .catalog_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                return Some(failure(
                    id,
                    "invalidRequest",
                    "downloads.prepare requires catalogId".into(),
                ));
            };
            if playback.owner != Owner::LocalDownloadedFile {
                return Some(failure(
                    id,
                    "ownerMismatch",
                    "downloads.prepare requires the localDownloadedFile playback lease".into(),
                ));
            }
            if payload.generation != Some(playback.generation) {
                return Some(failure(
                    id,
                    "generationMismatch",
                    "downloads.prepare requires the active playback generation".into(),
                ));
            }
            let Some(library) = downloads.library.as_ref() else {
                return Some(downloads.unavailable_response(id));
            };
            match library.prepare(video_id) {
                Ok(path) => ResponseEnvelope::success(
                    id,
                    ResponsePayload {
                        local_file: Some(path.display().to_string()),
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

    fn state() -> PlaybackState {
        PlaybackState {
            account_id: None,
            owner: Owner::None,
            generation: 0,
            sample_sequence: 0,
        }
    }

    fn downloads(directory: &std::path::Path, legacy: Option<PathBuf>) -> Downloads {
        Downloads::at(
            directory.join("downloads.json"),
            directory.join("decoded"),
            legacy,
        )
    }

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("goosic-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn non_download_commands_are_not_claimed() {
        let directory = scratch("claim");
        let mut store = downloads(&directory, None);
        assert!(handle(
            &mut store,
            &state(),
            "playback.claim",
            "1",
            &RequestPayload::default()
        )
        .is_none());
        assert!(handle(
            &mut store,
            &state(),
            "settings.get",
            "1",
            &RequestPayload::default()
        )
        .is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn listing_an_empty_library_succeeds_with_no_tracks() {
        let directory = scratch("list");
        let mut store = downloads(&directory, None);
        let response = handle(
            &mut store,
            &state(),
            "downloads.list",
            "1",
            &RequestPayload::default(),
        )
        .unwrap();
        assert!(response.ok);
        assert_eq!(response.payload.unwrap().downloads.unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn preparing_without_an_id_is_rejected() {
        let directory = scratch("prepare");
        let mut store = downloads(&directory, None);
        let response = handle(
            &mut store,
            &state(),
            "downloads.prepare",
            "1",
            &RequestPayload::default(),
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "invalidRequest");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn preparing_requires_the_local_owner_and_current_generation() {
        let directory = scratch("prepare-owner");
        let mut store = downloads(&directory, None);
        let response = handle(
            &mut store,
            &state(),
            "downloads.prepare",
            "1",
            &RequestPayload {
                catalog_id: Some("abcdefghijk".into()),
                generation: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "ownerMismatch");

        let response = handle(
            &mut store,
            &PlaybackState {
                account_id: None,
                owner: Owner::LocalDownloadedFile,
                generation: 4,
                sample_sequence: 0,
            },
            "downloads.prepare",
            "2",
            &RequestPayload {
                catalog_id: Some("abcdefghijk".into()),
                generation: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "generationMismatch");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn importing_without_a_legacy_directory_says_so() {
        let directory = scratch("import");
        let mut store = downloads(&directory, None);
        let response = handle(
            &mut store,
            &state(),
            "downloads.importLegacy",
            "1",
            &RequestPayload::default(),
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "legacyNotFound");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn importing_lists_what_it_added() {
        let directory = scratch("added");
        let media = directory.join("stream");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("abcdefghijk.webm"), b"bytes").unwrap();
        std::fs::write(
            media.join("abcdefghijk.meta.json"),
            br#"{"title":"Nonsense","artist":"Sabrina Carpenter"}"#,
        )
        .unwrap();

        let mut store = downloads(&directory, Some(media));
        let response = handle(
            &mut store,
            &state(),
            "downloads.importLegacy",
            "1",
            &RequestPayload::default(),
        )
        .unwrap();
        assert!(response.ok);
        let payload = response.payload.unwrap();
        assert!(payload.message.unwrap().contains("Added 1"));
        assert_eq!(payload.downloads.unwrap()[0].title, "Nonsense");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
