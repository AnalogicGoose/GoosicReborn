//! Lyrics lookup.
//!
//! Lyrics come from [LRCLIB](https://lrclib.net), an open database that needs no account and no
//! key. Its Lyricsfile 1.0 field supplies real word timing when contributors have added it;
//! legacy LRC/plain lyrics remain the fallback. The previous Goosic also carried Genius and Musixmatch sources; neither is migrated
//! here, because Genius requires scraping rendered HTML and Musixmatch requires a user token —
//! and a token is a credential, which this migration does not carry over.
//!
//! Like the catalog, this crate answers a read-only question and can never affect playback
//! ownership.

mod lyricsfile;
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
/// The exact endpoint can look a recording up in external sources on a cache miss, which is
/// slow. It gets less time than the whole lookup so that search still has room to answer
/// inside the shell's 20-second request deadline.
const EXACT_TIMEOUT: Duration = Duration::from_secs(6);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(8);
/// Lyrics documents are small; anything larger is not lyrics.
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
/// Keeps one lyrics document inside a protocol frame.
const MAX_LINES_ON_THE_WIRE: usize = 900;
/// A search result whose length is within this many seconds of the playing track is treated as
/// the same recording. Catalogs round and trim lengths differently, so exact equality would
/// discard the right answer.
const LENGTH_MATCH_WINDOW_SECONDS: f64 = 2.0;
/// The most a record's length may differ from the playing track's before its line timing is
/// distrusted. Timing recorded against a live, extended, or video version is seconds off from
/// the first chorus on, and lyrics highlighted at the wrong moment are worse than lyrics that
/// are plainly not following along, so beyond this the text is kept and the timing is dropped.
const SYNC_DRIFT_TOLERANCE_SECONDS: f64 = 3.0;

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
    lyricsfile: Option<String>,
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    synced_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
    /// Length of the recording the lyrics were timed against, in seconds. LRCLIB sends a float.
    #[serde(default)]
    duration: Option<f64>,
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

/// Whether a failed exact lookup should still let search answer.
///
/// A transport failure, a server error, a rate limit, or an unreadable body says nothing about
/// whether lyrics exist. A client error other than 404 means the request itself was wrong, and
/// search would be asked the same wrong question.
fn falls_back_to_search(error: &LyricsError) -> bool {
    match error {
        LyricsError::Network(_) | LyricsError::Decode(_) => true,
        LyricsError::Upstream(status) => *status == 429 || *status >= 500,
        LyricsError::InvalidRequest(_) | LyricsError::NotFound => false,
    }
}

/// How well a record's length fits the playing track, best first.
#[derive(Clone, Copy, PartialEq)]
enum LengthFit {
    /// Within the match window.
    Match,
    /// Outside the window but within the sync tolerance, by this many seconds: close enough that
    /// its timing is still shown, so it beats a record whose length nobody knows.
    Near(f64),
    /// The record does not say how long it is, or the query did not say how long the track is.
    Unknown,
    /// Further off than the sync tolerance, by this many seconds.
    Off(f64),
}

impl LengthFit {
    fn of(record_seconds: Option<f64>, query_seconds: Option<u32>) -> Self {
        match (
            record_seconds.filter(|value| value.is_finite()),
            query_seconds,
        ) {
            (Some(record), Some(query)) => {
                let difference = (record - f64::from(query)).abs();
                if difference <= LENGTH_MATCH_WINDOW_SECONDS {
                    Self::Match
                } else if difference <= SYNC_DRIFT_TOLERANCE_SECONDS {
                    Self::Near(difference)
                } else {
                    Self::Off(difference)
                }
            }
            _ => Self::Unknown,
        }
    }

    /// Whether `self` is strictly better than `other`: a match, then near lengths closest first,
    /// then unknown, then far lengths closest first.
    fn beats(self, other: Self) -> bool {
        match (self, other) {
            (Self::Near(a), Self::Near(b)) | (Self::Off(a), Self::Off(b)) => a < b,
            _ => self.tier() < other.tier(),
        }
    }

    fn tier(self) -> u8 {
        match self {
            Self::Match => 0,
            Self::Near(_) => 1,
            Self::Unknown => 2,
            Self::Off(_) => 3,
        }
    }
}

/// Prefer the recording whose length fits, then usable real word timing, line timing, plain
/// lyrics and instrumentals.
///
/// Search used to accept only synced results, so a song LRCLIB held as plain text was reported
/// as having no lyrics at all. It also ignored length, so the richest timing won even when it
/// belonged to a live or extended version. Among records outside the match window the closest
/// length wins and richness only breaks ties; without a query length richness decides alone.
fn best_search_result(
    records: Vec<LrclibRecord>,
    query_seconds: Option<u32>,
) -> Option<LrclibRecord> {
    records
        .into_iter()
        .filter_map(|record| {
            let document = LyricsClient::document_ref(&record)?;
            let rank = if document.lines.iter().any(|line| !line.words.is_empty()) {
                3
            } else if document.synced {
                2
            } else if !record.instrumental
                && document
                    .lines
                    .first()
                    .is_some_and(|line| line.text != "♪ Instrumental")
            {
                1
            } else {
                0
            };
            Some((LengthFit::of(record.duration, query_seconds), rank, record))
        })
        .fold(
            None,
            |best: Option<(LengthFit, u8, LrclibRecord)>, candidate| {
                let better = match &best {
                    None => true,
                    Some((fit, rank, _)) => {
                        candidate.0.beats(*fit) || (!fit.beats(candidate.0) && candidate.1 > *rank)
                    }
                };
                if better {
                    Some(candidate)
                } else {
                    best
                }
            },
        )
        .map(|(_, _, record)| record)
}

/// Removes timing from a document, keeping its words.
///
/// Plain text from the record is preferred because it keeps the contributor's own line breaks;
/// when there is none, the timed lines are kept and merely stripped of their times.
fn without_timing(document: LyricsDocument, plain: Option<&str>) -> LyricsDocument {
    if let Some(text) = plain.map(str::trim).filter(|text| !text.is_empty()) {
        let lines = parse::parse_plain(text);
        if !lines.is_empty() {
            return LyricsClient::clamp(lines, false);
        }
    }
    LyricsDocument {
        synced: false,
        lines: document
            .lines
            .into_iter()
            .map(|line| goosic_protocol::LyricsLine {
                at_ms: -1,
                end_ms: None,
                words: Vec::new(),
                text: line.text,
            })
            .collect(),
        ..document
    }
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

fn title_without_feature_credit(title: &str) -> Option<&str> {
    let title = title.trim();
    let (opening, closing) = match title.as_bytes().last()? {
        b')' => ('(', ')'),
        b']' => ('[', ']'),
        _ => return None,
    };
    let (base, credit) = title.rsplit_once(opening)?;
    let base = base.trim_end();
    let credit = credit.strip_suffix(closing)?.trim();
    let featured = ["feat. ", "feat ", "ft. ", "ft ", "featuring "]
        .iter()
        .any(|prefix| {
            credit
                .get(..prefix.len())
                .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
        });
    (!base.is_empty() && featured).then_some(base)
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
        if let Some((cached_key, document)) = self.cached.lock().ok().and_then(|slot| slot.clone())
        {
            if cached_key == key {
                return Ok(document);
            }
        }

        let record = self.fetch(query, title)?;
        let document =
            Self::document_for(record, query.duration_seconds).ok_or(LyricsError::NotFound)?;
        if let Ok(mut slot) = self.cached.lock() {
            *slot = Some((key, document.clone()));
        }
        Ok(document)
    }

    /// The document for a record, with timing withheld when the record is for another length.
    fn document_for(record: LrclibRecord, query_seconds: Option<u32>) -> Option<LyricsDocument> {
        let document = Self::document_ref(&record)?;
        let drifted = match (
            record.duration.filter(|value| value.is_finite()),
            query_seconds,
        ) {
            (Some(recorded), Some(query)) => {
                (recorded - f64::from(query)).abs() > SYNC_DRIFT_TOLERANCE_SECONDS
            }
            _ => false,
        };
        if document.synced && drifted {
            return Some(without_timing(document, record.plain_lyrics.as_deref()));
        }
        Some(document)
    }

    fn fetch(&self, query: &LyricsQuery, title: &str) -> Result<LrclibRecord, LyricsError> {
        // The exact endpoint matches on title, artist, album, and length together. It is tried
        // first because it returns the right recording; search is the fallback when it misses.
        let mut exact_failure = None;
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
                request = request.query("duration", duration.to_string());
            }
            let request = request.config().timeout_global(Some(EXACT_TIMEOUT)).build();
            match self.send(request) {
                Ok(Some(record)) => return Ok(record),
                Ok(None) | Err(LyricsError::Upstream(404)) => {}
                // A slow or failing exact lookup used to end the whole request, so the shell
                // said "unavailable" for lyrics that search would have found in a moment.
                Err(error) if falls_back_to_search(&error) => exact_failure = Some(error),
                Err(error) => return Err(error),
            }
        }

        let mut request = self.agent.get(&format!("{BASE_URL}/search")).query(
            "track_name",
            title_without_feature_credit(title).unwrap_or(title),
        );
        if !query.artist.trim().is_empty() {
            request = request.query("artist_name", query.artist.trim());
        }
        let request = request
            .config()
            .timeout_global(Some(SEARCH_TIMEOUT))
            .build();
        let searched = request
            .call()
            .map_err(Self::transport_error)
            .and_then(|mut response| {
                response
                    .body_mut()
                    .with_config()
                    .limit(MAX_RESPONSE_BYTES)
                    .read_json::<Vec<LrclibRecord>>()
                    .map_err(|error| LyricsError::Decode(error.to_string()))
            })
            .and_then(|records| {
                best_search_result(records, query.duration_seconds).ok_or(LyricsError::NotFound)
            });
        match (searched, exact_failure) {
            (Ok(record), _) => Ok(record),
            // "Not found" would be a claim the failed exact lookup cannot back up; report the
            // failure instead, so the shell offers to try again.
            (Err(LyricsError::NotFound), Some(failure)) => Err(failure),
            (Err(error), _) => Err(error),
        }
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
    #[cfg(test)]
    fn document(record: LrclibRecord) -> Option<LyricsDocument> {
        Self::document_ref(&record)
    }

    fn document_ref(record: &LrclibRecord) -> Option<LyricsDocument> {
        if let Some(source) = record.lyricsfile.as_deref() {
            if let Some(document) = lyricsfile::parse(source) {
                return Some(document);
            }
        }
        if record.instrumental {
            return Some(LyricsDocument {
                source: "LRCLIB".into(),
                synced: false,
                lines: vec![goosic_protocol::LyricsLine {
                    end_ms: None,
                    words: Vec::new(),
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
        let original = lines.len();
        // Reserve space for the response envelope below every native client's 256 KiB limit.
        let mut bytes = 128;
        let retained = lines
            .iter()
            .take(MAX_LINES_ON_THE_WIRE)
            .take_while(|line| {
                bytes +=
                    serde_json::to_vec(line).map_or(usize::MAX / 2, |encoded| encoded.len() + 1);
                bytes <= 200 * 1024
            })
            .count();
        let truncated = retained < original;
        lines.truncate(retained);
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
    fn lyrics_search_only_removes_trailing_feature_credits() {
        for title in [
            "Song (feat. Artist)",
            "Song [FT. Artist]",
            "Song (featuring Artist)",
        ] {
            assert_eq!(title_without_feature_credit(title), Some("Song"));
        }
        assert_eq!(
            title_without_feature_credit("Song (Live) (feat. Artist)"),
            Some("Song (Live)")
        );
        for title in [
            "Song (Live)",
            "Song [Remix]",
            "(Don't Fear) The Reaper",
            "Song (feat. Artist) (Live)",
            "(feat. Artist)",
            "Song (日本語)",
            "",
        ] {
            assert_eq!(title_without_feature_credit(title), None, "{title}");
        }
    }

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
            lyricsfile: None,
            plain_lyrics: Some("Untimed words".into()),
            synced_lyrics: Some("[00:01.00]Timed words".into()),
            instrumental: false,
            duration: None,
        })
        .expect("a document");
        assert!(document.synced);
        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].text, "Timed words");
    }

    #[test]
    fn plain_lyrics_are_used_when_there_are_no_synced_ones() {
        let document = LyricsClient::document(LrclibRecord {
            lyricsfile: None,
            plain_lyrics: Some("First\nSecond".into()),
            synced_lyrics: None,
            instrumental: false,
            duration: None,
        })
        .expect("a document");
        assert!(!document.synced);
        assert_eq!(document.lines.len(), 2);
    }

    #[test]
    fn synced_lyrics_that_parse_to_nothing_fall_back_to_plain() {
        let document = LyricsClient::document(LrclibRecord {
            lyricsfile: None,
            plain_lyrics: Some("Real words".into()),
            synced_lyrics: Some("[ar:only metadata]".into()),
            instrumental: false,
            duration: None,
        })
        .expect("a document");
        assert!(!document.synced);
        assert_eq!(document.lines[0].text, "Real words");
    }

    #[test]
    fn an_instrumental_says_so_rather_than_showing_nothing() {
        let document = LyricsClient::document(LrclibRecord {
            lyricsfile: None,
            plain_lyrics: None,
            synced_lyrics: None,
            instrumental: true,
            duration: None,
        })
        .expect("a document");
        assert_eq!(document.lines.len(), 1);
        assert!(document.lines[0].text.contains("Instrumental"));
    }

    #[test]
    fn a_record_with_no_lyrics_at_all_produces_nothing() {
        assert!(LyricsClient::document(LrclibRecord {
            lyricsfile: None,
            plain_lyrics: Some("   ".into()),
            synced_lyrics: Some("".into()),
            instrumental: false,
            duration: None,
        })
        .is_none());
    }

    #[test]
    fn a_very_long_document_is_clamped_and_says_so() {
        let synced: String = (0..MAX_LINES_ON_THE_WIRE + 100)
            .map(|index| format!("[{:02}:{:02}.00]line {index}\n", index / 60, index % 60))
            .collect();
        let document = LyricsClient::document(LrclibRecord {
            lyricsfile: None,
            plain_lyrics: None,
            synced_lyrics: Some(synced),
            instrumental: false,
            duration: None,
        })
        .expect("a document");
        assert_eq!(document.lines.len(), MAX_LINES_ON_THE_WIRE);
        assert!(document.truncated);
    }

    #[test]
    fn lyricsfile_precedes_legacy_fields_and_search_prefers_real_words() {
        let rich = || LrclibRecord {
            lyricsfile: Some(lyricsfile::tests::WORDS.into()),
            synced_lyrics: Some("[00:01.00]Legacy".into()),
            plain_lyrics: None,
            instrumental: false,
            duration: None,
        };
        let document = LyricsClient::document(rich()).unwrap();
        assert_eq!(document.lines[0].text, "Hello world");
        assert!(!document.lines[0].words.is_empty());
        let chosen = best_search_result(
            vec![record(Some("[00:01.00]Legacy"), None, false), rich()],
            None,
        )
        .unwrap();
        assert!(chosen.lyricsfile.is_some());
    }

    #[test]
    fn broken_lyricsfile_uses_legacy_fallback_without_poisoning_search() {
        let mut broken = record(Some("[00:01.00]Legacy"), Some("Plain"), false);
        broken.lyricsfile = Some("version: '2.0'".into());
        assert_eq!(
            LyricsClient::document_ref(&broken).unwrap().lines[0].text,
            "Legacy"
        );
        let mut empty = record(None, None, false);
        empty.lyricsfile = Some("invalid: yaml".into());
        assert!(best_search_result(vec![empty, broken], None).is_some());
    }

    #[test]
    fn word_rich_documents_fit_the_native_frame_limit_and_report_truncation() {
        let mut line = lyricsfile::parse(lyricsfile::tests::WORDS)
            .unwrap()
            .lines
            .remove(0);
        line.text = "x".repeat(512);
        line.words = (0..128)
            .map(|index| goosic_protocol::LyricsWord {
                at_ms: index * 1000,
                end_ms: Some(index * 1000 + 900),
                text: "xxxx".into(),
            })
            .collect();
        let document = LyricsClient::clamp(vec![line; 900], true);
        assert!(document.truncated);
        assert!(serde_json::to_vec(&document).unwrap().len() < 201 * 1024);
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
        assert_eq!(
            LyricsClient::cache_key(&base),
            LyricsClient::cache_key(&same)
        );
        assert_ne!(
            LyricsClient::cache_key(&base),
            LyricsClient::cache_key(&different)
        );
    }

    fn record(synced: Option<&str>, plain: Option<&str>, instrumental: bool) -> LrclibRecord {
        LrclibRecord {
            lyricsfile: None,
            synced_lyrics: synced.map(str::to_owned),
            plain_lyrics: plain.map(str::to_owned),
            instrumental,
            duration: None,
        }
    }

    #[test]
    fn search_prefers_synced_then_plain_then_instrumental() {
        let chosen = best_search_result(
            vec![
                record(None, Some("Untimed"), false),
                record(Some("[00:01.00]Timed"), None, false),
            ],
            None,
        )
        .expect("a result");
        assert_eq!(chosen.synced_lyrics.as_deref(), Some("[00:01.00]Timed"));

        let chosen = best_search_result(
            vec![
                record(None, None, true),
                record(Some("   "), Some("Untimed"), false),
            ],
            None,
        )
        .expect("a result");
        assert_eq!(chosen.plain_lyrics.as_deref(), Some("Untimed"));

        let chosen = best_search_result(vec![record(None, None, true)], None).expect("a result");
        assert!(chosen.instrumental);
    }

    fn with_duration(mut record: LrclibRecord, seconds: Option<f64>) -> LrclibRecord {
        record.duration = seconds;
        record
    }

    #[test]
    fn a_known_length_within_the_sync_tolerance_beats_an_unknown_one() {
        let unknown = with_duration(record(None, Some("Unknown"), false), None);
        let near = with_duration(record(Some("[00:01.00]Near"), None, false), Some(224.5));
        let chosen = best_search_result(vec![unknown, near], Some(222)).unwrap();
        assert!(chosen.synced_lyrics.is_some());

        // Beyond the tolerance its timing would be dropped anyway, so unknown still wins.
        let unknown = with_duration(record(None, Some("Unknown"), false), None);
        let far = with_duration(record(Some("[00:01.00]Far"), None, false), Some(230.0));
        let chosen = best_search_result(vec![unknown, far], Some(222)).unwrap();
        assert_eq!(chosen.plain_lyrics.as_deref(), Some("Unknown"));
    }

    #[test]
    fn the_closest_length_beats_richer_timing_from_a_far_one() {
        let live = with_duration(record(Some("[00:01.00]Live"), None, false), Some(400.0));
        let studio = with_duration(record(None, Some("Studio"), false), Some(221.0));
        let chosen = best_search_result(vec![live, studio], Some(222)).unwrap();
        assert_eq!(chosen.plain_lyrics.as_deref(), Some("Studio"));

        let far = with_duration(record(Some("[00:01.00]Far"), None, false), Some(300.0));
        let nearer = with_duration(record(None, Some("Nearer"), false), Some(260.0));
        let chosen = best_search_result(vec![far, nearer], Some(222)).unwrap();
        assert_eq!(chosen.plain_lyrics.as_deref(), Some("Nearer"));
    }

    #[test]
    fn inside_the_window_richness_still_decides() {
        let plain = with_duration(record(None, Some("Plain"), false), Some(222.0));
        let synced = with_duration(record(Some("[00:01.00]Timed"), None, false), Some(223.5));
        let chosen = best_search_result(vec![plain, synced], Some(222)).unwrap();
        assert_eq!(chosen.synced_lyrics.as_deref(), Some("[00:01.00]Timed"));
    }

    #[test]
    fn an_unknown_length_is_usable_but_loses_to_a_match() {
        let unknown = record(Some("[00:01.00]Unknown"), None, false);
        let matching = with_duration(record(None, Some("Match"), false), Some(222.0));
        let chosen = best_search_result(vec![unknown, matching], Some(222)).unwrap();
        assert_eq!(chosen.plain_lyrics.as_deref(), Some("Match"));
        let unknown = record(Some("[00:01.00]Unknown"), None, false);
        assert!(best_search_result(vec![unknown], Some(222)).is_some());
    }

    #[test]
    fn without_a_query_length_search_keeps_the_old_ranking() {
        let far = with_duration(record(Some("[00:01.00]Far"), None, false), Some(900.0));
        let near = with_duration(record(None, Some("Near"), false), Some(222.0));
        let chosen = best_search_result(vec![near, far], None).unwrap();
        assert_eq!(chosen.synced_lyrics.as_deref(), Some("[00:01.00]Far"));
    }

    #[test]
    fn timing_from_a_different_length_is_not_presented_as_synced() {
        let both = with_duration(
            record(Some("[00:01.00]Timed"), Some("Plain one\nPlain two"), false),
            Some(240.0),
        );
        let document = LyricsClient::document_for(both, Some(222)).unwrap();
        assert!(!document.synced);
        assert_eq!(document.lines.len(), 2);
        assert!(document.lines.iter().all(|line| line.at_ms < 0));
        assert_eq!(document.lines[0].text, "Plain one");

        let only_synced = with_duration(record(Some("[00:01.00]Timed"), None, false), Some(240.0));
        let document = LyricsClient::document_for(only_synced, Some(222)).unwrap();
        assert!(!document.synced);
        assert_eq!(document.lines[0].text, "Timed");
        assert_eq!(document.lines[0].at_ms, -1);
    }

    #[test]
    fn timing_within_the_tolerance_or_without_a_length_stays_synced() {
        let near = with_duration(record(Some("[00:01.00]Timed"), None, false), Some(224.5));
        assert!(LyricsClient::document_for(near, Some(222)).unwrap().synced);
        let unknown = record(Some("[00:01.00]Timed"), None, false);
        assert!(
            LyricsClient::document_for(unknown, Some(222))
                .unwrap()
                .synced
        );
        let far = with_duration(record(Some("[00:01.00]Timed"), None, false), Some(500.0));
        assert!(LyricsClient::document_for(far, None).unwrap().synced);
    }

    #[test]
    fn an_instrumental_is_untouched_by_the_drift_rule() {
        let instrumental = with_duration(record(None, None, true), Some(500.0));
        let document = LyricsClient::document_for(instrumental, Some(222)).unwrap();
        assert!(document.lines[0].text.contains("Instrumental"));
    }

    #[test]
    fn a_lyricsfile_with_an_offset_is_skipped_so_the_lrc_offset_applies() {
        let mut shifted = record(Some("[offset:+500]\n[00:02.00]Legacy"), None, false);
        shifted.lyricsfile = Some(lyricsfile::tests::WORDS.replace(
            "artist: Test artist",
            "artist: Test artist\n  offset_ms: 500",
        ));
        let document = LyricsClient::document_ref(&shifted).unwrap();
        assert_eq!(document.lines[0].text, "Legacy");
        assert_eq!(document.lines[0].at_ms, 1_500);
    }

    #[test]
    fn search_with_no_lyrics_anywhere_finds_nothing() {
        assert!(best_search_result(vec![], None).is_none());
        assert!(best_search_result(vec![record(Some(" "), Some(""), false)], None).is_none());
    }

    #[test]
    fn only_failures_that_say_nothing_about_the_track_fall_back_to_search() {
        assert!(falls_back_to_search(&LyricsError::Network(
            "timed out".into()
        )));
        assert!(falls_back_to_search(&LyricsError::Decode("eof".into())));
        assert!(falls_back_to_search(&LyricsError::Upstream(503)));
        assert!(falls_back_to_search(&LyricsError::Upstream(429)));
        assert!(!falls_back_to_search(&LyricsError::Upstream(400)));
        assert!(!falls_back_to_search(&LyricsError::InvalidRequest(
            "no title".into()
        )));
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
