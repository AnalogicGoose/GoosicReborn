//! The lyrics panel's state, decided without GTK.
//!
//! Lyrics are looked up only while the panel is open, once per track, and an answer that arrives
//! after the track changed is dropped rather than shown against the wrong song. Which line to
//! highlight, and the duration parse the lookup needs, are shared rules from
//! `goosic-shell-support`.

use goosic_protocol::{LyricsDocument, LyricsQuery, ResponseEnvelope};
use goosic_shell_support::catalog::Track;
use goosic_shell_support::lyrics::{active_line_index, duration_seconds};
use goosic_shell_support::TransportError;

const NOTHING_PLAYING: &str = "Nothing playing.";
const NOT_FOUND: &str = "No lyrics were found for this track.";
/// Local timed-lyrics import cap, matching `NativeMacLyricsImport.swift` (2 MB).
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;

/// Decodes a session-only local timed-lyrics JSON file (`LyricsDocument` wire format).
pub fn decode_imported_lyrics(bytes: &[u8]) -> Result<LyricsDocument, String> {
    if bytes.is_empty() {
        return Err("The selected lyrics file is empty.".to_owned());
    }
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err("Lyrics files larger than 2 MB are refused.".to_owned());
    }
    let document: LyricsDocument = serde_json::from_slice(bytes)
        .map_err(|_| "That file is not a valid Goosic timed-lyrics JSON document.".to_owned())?;
    if document.lines.is_empty() {
        return Err("The selected lyrics file contains no lines.".to_owned());
    }
    Ok(document)
}

#[derive(Debug)]
pub struct Lyrics {
    pub visible: bool,
    document: Option<LyricsDocument>,
    /// The track the lyrics were asked for, so one track is not asked about twice.
    video_id: Option<String>,
    in_flight: bool,
    status: String,
    /// Bumped whenever what the panel draws changes, so the panel redraws only then.
    revision: u64,
}

impl Default for Lyrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Lyrics {
    pub fn new() -> Lyrics {
        Lyrics {
            visible: false,
            document: None,
            video_id: None,
            in_flight: false,
            status: NOTHING_PLAYING.to_owned(),
            revision: 0,
        }
    }

    pub fn document(&self) -> Option<&LyricsDocument> {
        self.document.as_ref()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Forgets what was loaded, because another track is starting.
    pub fn track_changed(&mut self) {
        self.video_id = None;
        self.set(None, NOTHING_PLAYING);
    }

    /// Installs session-only imported lyrics for `video_id`, without altering playback authority.
    pub fn install_imported(&mut self, video_id: &str, mut document: LyricsDocument, label: &str) {
        self.in_flight = false;
        self.video_id = Some(video_id.to_owned());
        if document.source.trim().is_empty() {
            document.source = label.to_owned();
        }
        let status = if document.synced {
            format!("Imported timed lyrics from {label} (this session only).")
        } else {
            format!("Imported lyrics from {label} (this session only).")
        };
        self.set(Some(document), &status);
    }

    /// Retries a failed lookup using the real duration reported by an adopted track version.
    pub fn retry_with_duration(
        &mut self,
        track: &Track,
        real_duration_seconds: f64,
    ) -> Option<(String, LyricsQuery)> {
        if !self.visible
            || self.in_flight
            || self.document.is_some()
            || !real_duration_seconds.is_finite()
            || real_duration_seconds <= 0.0
        {
            return None;
        }
        let rounded = real_duration_seconds.round() as u32;
        if duration_seconds(&track.duration) == Some(rounded) {
            return None;
        }
        self.in_flight = true;
        self.video_id = Some(track.video_id.clone());
        let query = LyricsQuery {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_seconds: Some(rounded),
        };
        Some((track.video_id.clone(), query))
    }

    /// The lookup `current` needs, marked as in flight — or `None` when the panel is closed, the
    /// lyrics are already loaded for it, or an earlier lookup has not answered yet.
    pub fn begin(&mut self, current: Option<&Track>) -> Option<(String, LyricsQuery)> {
        if !self.visible {
            return None;
        }
        let Some(track) = current else {
            self.video_id = None;
            self.set(None, NOTHING_PLAYING);
            return None;
        };
        if self.in_flight || self.video_id.as_deref() == Some(track.video_id.as_str()) {
            return None;
        }
        self.in_flight = true;
        self.video_id = Some(track.video_id.clone());
        self.set(None, &format!("Looking up lyrics for {}…", track.title));
        let query = LyricsQuery {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_seconds: duration_seconds(&track.duration),
        };
        Some((track.video_id.clone(), query))
    }

    /// Takes the answer to a lookup made for `requested_for`, unless `current` is another track
    /// by now.
    pub fn finish(
        &mut self,
        requested_for: &str,
        current: Option<&str>,
        answer: Result<ResponseEnvelope, TransportError>,
    ) {
        self.in_flight = false;
        if current != Some(requested_for) {
            return;
        }
        match answer {
            Ok(response) => {
                let document = response
                    .payload
                    .and_then(|payload| payload.lyrics)
                    .filter(|document| !document.lines.is_empty());
                match document {
                    Some(document) => {
                        let status = if document.synced {
                            format!("Synced lyrics from {}.", document.source)
                        } else {
                            format!(
                                "Lyrics from {}; this version is not synced.",
                                document.source
                            )
                        };
                        self.set(Some(document), &status);
                    }
                    None => self.set(None, NOT_FOUND),
                }
            }
            Err(error) => {
                let (code, message) = error.describe();
                if code == "lyricsNotFound" {
                    self.set(None, NOT_FOUND);
                } else {
                    self.set(None, &format!("Could not load lyrics: {message}"));
                }
            }
        }
    }

    /// The line to highlight at `position_seconds`, if any.
    pub fn active_line(&self, position_seconds: f64) -> Option<usize> {
        let document = self.document.as_ref()?;
        active_line_index(
            &document.lines,
            document.synced,
            (position_seconds * 1000.0) as i64,
        )
    }

    fn set(&mut self, document: Option<LyricsDocument>, status: &str) {
        self.document = document;
        self.status = status.to_owned();
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::{LyricsLine, ResponsePayload};

    fn track(video_id: &str) -> Track {
        Track {
            id: video_id.into(),
            title: format!("Song {video_id}"),
            subtitle: String::new(),
            artist: "Artist".into(),
            artist_id: None,
            album: "Album".into(),
            album_id: None,
            duration: "3:42".into(),
            video_id: video_id.into(),
            explicit: false,
            thumbnail: None,
        }
    }

    fn found(synced: bool, lines: usize) -> Result<ResponseEnvelope, TransportError> {
        let lines = (0..lines)
            .map(|index| LyricsLine {
                words: Vec::new(),
                end_ms: None,
                at_ms: if synced { index as i64 * 5_000 } else { -1 },
                text: format!("line {index}"),
            })
            .collect();
        Ok(ResponseEnvelope::success(
            "r",
            ResponsePayload {
                lyrics: Some(LyricsDocument {
                    source: "LRCLIB".into(),
                    synced,
                    lines,
                    truncated: false,
                }),
                ..Default::default()
            },
        ))
    }

    fn open() -> Lyrics {
        Lyrics {
            visible: true,
            ..Lyrics::new()
        }
    }

    #[test]
    fn a_closed_panel_asks_for_nothing() {
        let mut lyrics = Lyrics::new();
        assert!(lyrics.begin(Some(&track("a"))).is_none());
    }

    #[test]
    fn each_track_is_asked_about_once_with_its_length_in_seconds() {
        let mut lyrics = open();
        let (requested, query) = lyrics.begin(Some(&track("a"))).expect("a first lookup");
        assert_eq!(requested, "a");
        assert_eq!(query.duration_seconds, Some(222));
        assert_eq!(query.artist, "Artist");
        assert!(
            lyrics.begin(Some(&track("a"))).is_none(),
            "already in flight"
        );
        lyrics.finish("a", Some("a"), found(true, 3));
        assert!(lyrics.begin(Some(&track("a"))).is_none(), "already loaded");
        assert_eq!(lyrics.status(), "Synced lyrics from LRCLIB.");
    }

    #[test]
    fn an_answer_for_a_track_that_is_no_longer_playing_is_dropped() {
        let mut lyrics = open();
        lyrics.begin(Some(&track("a")));
        lyrics.track_changed();
        lyrics.finish("a", Some("b"), found(true, 3));
        assert!(lyrics.document().is_none());
        assert!(
            lyrics.begin(Some(&track("b"))).is_some(),
            "the new track is looked up once the old answer is out of the way"
        );
    }

    #[test]
    fn missing_lyrics_are_said_plainly() {
        let mut lyrics = open();
        lyrics.begin(Some(&track("a")));
        lyrics.finish("a", Some("a"), found(true, 0));
        assert_eq!(lyrics.status(), NOT_FOUND);

        let mut lyrics = open();
        lyrics.begin(Some(&track("a")));
        lyrics.finish(
            "a",
            Some("a"),
            Err(TransportError::Remote {
                code: "lyricsNotFound".into(),
                message: "none".into(),
            }),
        );
        assert_eq!(lyrics.status(), NOT_FOUND);
    }

    #[test]
    fn only_synced_lyrics_highlight_and_they_follow_the_position() {
        let mut lyrics = open();
        lyrics.begin(Some(&track("a")));
        lyrics.finish("a", Some("a"), found(true, 3));
        assert_eq!(lyrics.active_line(6.0), Some(1));
        assert_eq!(lyrics.active_line(11.0), Some(2));

        let mut plain = open();
        plain.begin(Some(&track("a")));
        plain.finish("a", Some("a"), found(false, 3));
        assert_eq!(plain.active_line(6.0), None);
        assert_eq!(
            plain.status(),
            "Lyrics from LRCLIB; this version is not synced."
        );
    }

    #[test]
    fn stopping_clears_the_panel() {
        let mut lyrics = open();
        lyrics.begin(Some(&track("a")));
        lyrics.finish("a", Some("a"), found(true, 3));
        let before = lyrics.revision();
        assert!(lyrics.begin(None).is_none());
        assert!(lyrics.document().is_none());
        assert_eq!(lyrics.status(), NOTHING_PLAYING);
        assert!(lyrics.revision() > before, "the panel is told to redraw");
    }

    #[test]
    fn imported_lyrics_validate_bounds_and_clear_on_track_change() {
        assert!(decode_imported_lyrics(b"").is_err());
        assert!(decode_imported_lyrics(&vec![b' '; MAX_IMPORT_BYTES + 1]).is_err());
        assert!(decode_imported_lyrics(b"{not json").is_err());
        let empty = br#"{"source":"local","synced":true,"lines":[],"truncated":false}"#;
        assert!(decode_imported_lyrics(empty).is_err());

        let valid = br#"{
            "source":"local",
            "synced":true,
            "lines":[
                {"atMs":1000,"endMs":3000,"text":"hello world","words":[
                    {"atMs":1000,"endMs":1800,"text":"hello "},
                    {"atMs":1800,"endMs":3000,"text":"world"}
                ]}
            ],
            "truncated":false
        }"#;
        let doc = decode_imported_lyrics(valid).expect("valid lyrics");
        let mut lyrics = open();
        lyrics.install_imported("a", doc, "song.json");
        assert!(lyrics.document().is_some());
        assert_eq!(lyrics.active_line(1.5), Some(0));
        assert!(lyrics.status().contains("song.json"));
        lyrics.track_changed();
        assert!(lyrics.document().is_none());
    }
}
