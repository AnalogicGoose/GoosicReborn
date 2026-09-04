//! Lyrics lookup.
//!
//! Lyrics come from [LRCLIB](https://lrclib.net), an open database that needs no account and no
//! key. The previous Goosic also carried Genius and Musixmatch sources; neither is migrated
//! here, because Genius requires scraping rendered HTML and Musixmatch requires a user token —
//! and a token is a credential, which this migration does not carry over.
//!
//! Like the catalog, this crate answers a read-only question and can never affect playback
//! ownership.

pub mod parse;

use std::sync::Mutex;
use std::time::Duration;

use goosic_protocol::{LyricsDocument, LyricsQuery};
use serde::Deserialize;
use thiserror::Error;

pub use parse::{parse_lrc, parse_plain};

const BASE_URL: &str = "https://lrclib.net/api";
/// LRCLIB asks clients to identify themselves.
const USER_AGENT: &str = concat!(
    "GoosicReborn/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/osgamerxd/GoosicReborn)"
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Lyrics documents are small; anything larger is not lyrics.
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
/// Keeps one lyrics document inside a protocol frame.
const MAX_LINES_ON_THE_WIRE: usize = 900;

#[derive(Debug, Error)]
pub enum LyricsError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("lyrics request failed: {0}")]
    Network(String),
    #[error("lyrics upstream returned HTTP {0}")]
    Upstream(u16),
    #[error("lyrics response could not be decoded: {0}")]
    Decode(String),
    #[error("no lyrics were found for this track")]
    NotFound,
}

impl LyricsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalidRequest",
            Self::Network(_) => "lyricsUnavailable",
            Self::Upstream(_) => "lyricsUpstreamError",
            Self::Decode(_) => "lyricsDecodeError",
            Self::NotFound => "lyricsNotFound",
        }
    }
}

/// One LRCLIB record.
///
/// The field names are the ones the API actually sends. Without the rename every field silently
/// decoded as absent and every lookup reported "not found".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibRecord {
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    synced_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
}

/// Parses a `m:ss` or `h:mm:ss` duration into whole seconds.
///
/// The catalog carries durations as display text, and the lyrics database matches on length, so
/// this is what keeps a three-minute single from matching a ten-minute live version.
pub fn duration_seconds(text: &str) -> Option<u32> {
    let mut total: u32 = 0;
    let mut parts = 0;
    for part in text.split(':') {
        let value: u32 = part.trim().parse().ok()?;
        total = total.checked_mul(60)?.checked_add(value)?;
        parts += 1;
    }
    (2..=3).contains(&parts).then_some(total)
}

/// Live lyrics lookups. Held by the service for its process lifetime.
pub struct LyricsClient {
    agent: ureq::Agent,
    /// The last answer, keyed by the query that produced it. Scrubbing a track re-asks for the
    /// same lyrics constantly, and the answer cannot change between those calls.
    cached: Mutex<Option<(String, LyricsDocument)>>,
}

impl Default for LyricsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsClient {
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .user_agent(USER_AGENT)
                .build()
                .new_agent(),
            cached: Mutex::new(None),
        }
    }

    /// A stable key for a query, so repeats hit the cache.
    fn cache_key(query: &LyricsQuery) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            query.title.trim().to_lowercase(),
            query.artist.trim().to_lowercase(),
            query.album.trim().to_lowercase(),
            query.duration_seconds.unwrap_or(0)
        )
    }

    pub fn lookup(&self, query: &LyricsQuery) -> Result<LyricsDocument, LyricsError> {
        let title = query.title.trim();
        if title.is_empty() {
            return Err(LyricsError::InvalidRequest(
                "lyrics lookup requires a track title".into(),
            ));
        }
        let key = Self::cache_key(query);
        if let Some((cached_key, document)) = self.cached.lock().ok().and_then(|slot| slot.clone()) {
            if cached_key == key {
                return Ok(document);
            }
        }

        let record = self.fetch(query, title)?;
        let document = Self::document(record).ok_or(LyricsError::NotFound)?;
        if let Ok(mut slot) = self.cached.lock() {
            *slot = Some((key, document.clone()));
        }
        Ok(document)
    }

    fn fetch(&self, query: &LyricsQuery, title: &str) -> Result<LrclibRecord, LyricsError> {
        // The exact endpoint matches on title, artist, album, and length together. It is tried
        // first because it returns the right recording; search is the fallback when it misses.
        if !query.artist.trim().is_empty() {
            let mut request = self
                .agent
                .get(&format!("{BASE_URL}/get"))
                .query("track_name", title)
                .query("artist_name", query.artist.trim());
            if !query.album.trim().is_empty() {
                request = request.query("album_name", query.album.trim());
            }
            if let Some(duration) = query.duration_seconds {
                request = request.query("duration", &duration.to_string());
            }
            match self.send(request) {
                Ok(Some(record)) => return Ok(record),
                Ok(None) => {}
                Err(LyricsError::Upstream(404)) => {}
                Err(error) => return Err(error),
            }
        }

        let mut request = self
            .agent
            .get(&format!("{BASE_URL}/search"))
            .query("track_name", title);
        if !query.artist.trim().is_empty() {
            request = request.query("artist_name", query.artist.trim());
        }
        let mut response = request.call().map_err(Self::transport_error)?;
        let records: Vec<LrclibRecord> = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_json()
            .map_err(|error| LyricsError::Decode(error.to_string()))?;
        // Prefer a result that actually has synced lyrics; a plain-text match is a worse answer
        // than a timed one from slightly further down the list.
        records
            .into_iter()
            .find(|record| {
                record
                    .synced_lyrics
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
            })
            .ok_or(LyricsError::NotFound)
    }

    fn send(
        &self,
        request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    ) -> Result<Option<LrclibRecord>, LyricsError> {
        let mut response = request.call().map_err(Self::transport_error)?;
        let record: LrclibRecord = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_json()
            .map_err(|error| LyricsError::Decode(error.to_string()))?;
        Ok(Some(record))
    }

    fn transport_error(error: ureq::Error) -> LyricsError {
        match error {
            ureq::Error::StatusCode(status) => LyricsError::Upstream(status),
            other => LyricsError::Network(other.to_string()),
        }
    }

    /// Turns a record into the document the shell renders, preferring synced lyrics.
    fn document(record: LrclibRecord) -> Option<LyricsDocument> {
        if record.instrumental {
            return Some(LyricsDocument {
                source: "LRCLIB".into(),
                synced: false,
                lines: vec![goosic_protocol::LyricsLine {
                    at_ms: -1,
                    text: "♪ Instrumental".into(),
                }],
                truncated: false,
            });
        }

        if let Some(synced) = record.synced_lyrics.as_deref().map(str::trim) {
            if !synced.is_empty() {
                let lines = parse::parse_lrc(synced);
                if !lines.is_empty() {
                    return Some(Self::clamp(lines, true));
                }
            }
        }
        let plain = record.plain_lyrics.as_deref().map(str::trim).unwrap_or("");
        if plain.is_empty() {
            return None;
        }
        let lines = parse::parse_plain(plain);
        (!lines.is_empty()).then(|| Self::clamp(lines, false))
    }

    fn clamp(mut lines: Vec<goosic_protocol::LyricsLine>, synced: bool) -> LyricsDocument {
        let truncated = lines.len() > MAX_LINES_ON_THE_WIRE;
        if truncated {
            lines.truncate(MAX_LINES_ON_THE_WIRE);
        }
        LyricsDocument {
            source: "LRCLIB".into(),
            synced,
            lines,
            truncated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_durations_become_seconds() {
        assert_eq!(duration_seconds("3:42"), Some(222));
        assert_eq!(duration_seconds("0:09"), Some(9));
        assert_eq!(duration_seconds("1:02:03"), Some(3_723));
    }

    #[test]
    fn nonsense_durations_are_refused() {
        assert_eq!(duration_seconds("222"), None);
        assert_eq!(duration_seconds(""), None);
        assert_eq!(duration_seconds("a:b"), None);
        assert_eq!(duration_seconds("1:2:3:4"), None);
    }

    #[test]
    fn a_missing_title_is_refused_before_any_network_call() {
        let client = LyricsClient::new();
        let error = client
            .lookup(&LyricsQuery {
                title: "   ".into(),
                ..Default::default()
            })
            .map(|_| ())
            .unwrap_err();
        assert!(matches!(error, LyricsError::InvalidRequest(_)));
    }

    #[test]
    fn synced_lyrics_are_preferred_over_plain_ones() {
        let document = LyricsClient::document(LrclibRecord {
            plain_lyrics: Some("Untimed words".into()),
            synced_lyrics: Some("[00:01.00]Timed words".into()),
            instrumental: false,
        })
        .expect("a document");
        assert!(document.synced);
        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].text, "Timed words");
    }

    #[test]
    fn plain_lyrics_are_used_when_there_are_no_synced_ones() {
        let document = LyricsClient::document(LrclibRecord {
            plain_lyrics: Some("First\nSecond".into()),
            synced_lyrics: None,
            instrumental: false,
        })
        .expect("a document");
        assert!(!document.synced);
        assert_eq!(document.lines.len(), 2);
    }

    #[test]
    fn synced_lyrics_that_parse_to_nothing_fall_back_to_plain() {
        let document = LyricsClient::document(LrclibRecord {
            plain_lyrics: Some("Real words".into()),
            synced_lyrics: Some("[ar:only metadata]".into()),
            instrumental: false,
        })
        .expect("a document");
        assert!(!document.synced);
        assert_eq!(document.lines[0].text, "Real words");
    }

    #[test]
    fn an_instrumental_says_so_rather_than_showing_nothing() {
        let document = LyricsClient::document(LrclibRecord {
            plain_lyrics: None,
            synced_lyrics: None,
            instrumental: true,
        })
        .expect("a document");
        assert_eq!(document.lines.len(), 1);
        assert!(document.lines[0].text.contains("Instrumental"));
    }

    #[test]
    fn a_record_with_no_lyrics_at_all_produces_nothing() {
        assert!(LyricsClient::document(LrclibRecord {
            plain_lyrics: Some("   ".into()),
            synced_lyrics: Some("".into()),
            instrumental: false,
        })
        .is_none());
    }

    #[test]
    fn a_very_long_document_is_clamped_and_says_so() {
        let synced: String = (0..MAX_LINES_ON_THE_WIRE + 100)
            .map(|index| format!("[{:02}:{:02}.00]line {index}\n", index / 60, index % 60))
            .collect();
        let document = LyricsClient::document(LrclibRecord {
            plain_lyrics: None,
            synced_lyrics: Some(synced),
            instrumental: false,
        })
        .expect("a document");
        assert_eq!(document.lines.len(), MAX_LINES_ON_THE_WIRE);
        assert!(document.truncated);
    }

    #[test]
    fn cache_keys_ignore_case_and_padding_but_not_the_track() {
        let base = LyricsQuery {
            title: " Afterglow ".into(),
            artist: "Signal Fires".into(),
            album: String::new(),
            duration_seconds: Some(222),
        };
        let same = LyricsQuery {
            title: "afterglow".into(),
            artist: "signal fires".into(),
            ..base.clone()
        };
        let different = LyricsQuery {
            title: "Another song".into(),
            ..base.clone()
        };
        assert_eq!(LyricsClient::cache_key(&base), LyricsClient::cache_key(&same));
        assert_ne!(
            LyricsClient::cache_key(&base),
            LyricsClient::cache_key(&different)
        );
    }

    #[test]
    fn a_record_decodes_from_the_field_names_lrclib_actually_sends() {
        let wire = r#"{
            "id": 1, "trackName": "Afterglow", "artistName": "Signal Fires",
            "albumName": "Night Windows", "duration": 222.0, "instrumental": false,
            "plainLyrics": "Untimed", "syncedLyrics": "[00:01.00]Timed"
        }"#;
        let record: LrclibRecord = serde_json::from_str(wire).expect("record decodes");
        assert_eq!(record.synced_lyrics.as_deref(), Some("[00:01.00]Timed"));
        assert_eq!(record.plain_lyrics.as_deref(), Some("Untimed"));
        assert!(!record.instrumental);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(LyricsError::NotFound.code(), "lyricsNotFound");
        assert_eq!(LyricsError::Upstream(503).code(), "lyricsUpstreamError");
        assert_eq!(
            LyricsError::Network("boom".into()).code(),
            "lyricsUnavailable"
        );
    }
}
