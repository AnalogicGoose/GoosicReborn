//! Which lyric line is current, and the one parse the lookup depends on.
//!
//! The highlight is decided by the shell rather than by the service because the shell is the side
//! that knows the playback position moment to moment. Deciding it here keeps that true while still
//! giving every shell the same answer.

use goosic_protocol::LyricsLine;

use crate::text::trim_spaces;

/// Which line is current at `position_ms`.
///
/// Unsynced lyrics never highlight, because guessing a position would be worse than showing the
/// words plainly, and nothing is highlighted before the first line begins.
pub fn active_line_index(lines: &[LyricsLine], synced: bool, position_ms: i64) -> Option<usize> {
    let first = lines.first()?;
    if !synced || first.at_ms > position_ms {
        return None;
    }
    lines.iter().rposition(|line| line.at_ms <= position_ms)
}

/// Validate imported word timing without guessing missing timestamps.
pub fn valid_timing(document: &goosic_protocol::LyricsDocument) -> bool {
    if !document.synced || document.lines.is_empty() || document.lines.len() > 900 {
        return false;
    }
    let mut previous = -1;
    for line in &document.lines {
        if line.at_ms < 0 || line.at_ms <= previous || line.text.chars().count() > 512 {
            return false;
        }
        previous = line.at_ms;
        if line.words.len() > 128 {
            return false;
        }
        if !line.words.is_empty() {
            if line
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<String>()
                != line.text
            {
                return false;
            }
            let mut end = line.at_ms;
            for word in &line.words {
                if word.at_ms < end
                    || word.end_ms.unwrap_or(word.at_ms) < word.at_ms
                    || word.text.is_empty()
                {
                    return false;
                }
                end = word.end_ms.unwrap_or(word.at_ms);
            }
        }
    }
    document.lines.windows(2).all(|pair| {
        pair[0]
            .words
            .last()
            .and_then(|word| word.end_ms)
            .map_or(true, |end| end <= pair[1].at_ms)
    })
}

/// Render only supplied start/end timing; missing ends activate a whole word at its start.
pub fn word_progress(word: &goosic_protocol::LyricsWord, position_ms: f64) -> f64 {
    if !position_ms.is_finite() {
        return 0.0;
    }
    match word.end_ms.filter(|end| *end > word.at_ms) {
        Some(end) => {
            ((position_ms - word.at_ms as f64) / (end as f64 - word.at_ms as f64)).clamp(0.0, 1.0)
        }
        None => {
            if position_ms >= word.at_ms as f64 {
                1.0
            } else {
                0.0
            }
        }
    }
}

/// Parses a display duration such as `3:42` or `1:02:03` into whole seconds.
///
/// Returns `None` for anything else, so a malformed value is refused rather than narrowing the
/// lyrics lookup to the wrong recording.
pub fn duration_seconds(text: &str) -> Option<u32> {
    let parts: Vec<&str> = text.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    parts.into_iter().try_fold(0_u32, |total, part| {
        let value: u32 = trim_spaces(part).parse().ok()?;
        total.checked_mul(60)?.checked_add(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(at_ms: i64, text: &str) -> LyricsLine {
        LyricsLine {
            at_ms,
            text: text.into(),
            words: Vec::new(),
        }
    }

    fn document() -> goosic_protocol::LyricsDocument {
        goosic_protocol::LyricsDocument {
            source: "test".into(),
            synced: true,
            lines: vec![
                LyricsLine {
                    at_ms: 1000,
                    text: "Hi all".into(),
                    words: vec![
                        goosic_protocol::LyricsWord {
                            at_ms: 1000,
                            end_ms: Some(2000),
                            text: "Hi ".into(),
                        },
                        goosic_protocol::LyricsWord {
                            at_ms: 2000,
                            end_ms: Some(4000),
                            text: "all".into(),
                        },
                    ],
                },
                line(5000, "Next"),
            ],
            truncated: false,
        }
    }

    #[test]
    fn imported_timing_must_match_text_and_not_overlap() {
        let mut doc = document();
        assert!(valid_timing(&doc));
        doc.lines[0].words[1].at_ms = 1500;
        assert!(!valid_timing(&doc));
        doc = document();
        doc.lines[0].words[1].text = "wrong".into();
        assert!(!valid_timing(&doc));
        doc = document();
        doc.lines[0].words[1].end_ms = Some(6000);
        assert!(!valid_timing(&doc));
    }

    #[test]
    fn word_progress_never_guesses_a_missing_end() {
        let mut word = goosic_protocol::LyricsWord {
            at_ms: 1000,
            end_ms: Some(2000),
            text: "Hi".into(),
        };
        assert_eq!(word_progress(&word, 500.0), 0.0);
        assert_eq!(word_progress(&word, 1500.0), 0.5);
        assert_eq!(word_progress(&word, 2500.0), 1.0);
        assert_eq!(word_progress(&word, f64::NAN), 0.0);
        word.end_ms = None;
        assert_eq!(word_progress(&word, 999.0), 0.0);
        assert_eq!(word_progress(&word, 1000.0), 1.0);
        let mut doc = document();
        doc.lines[0].words[0].end_ms = None;
        assert!(valid_timing(&doc));
    }

    fn timed() -> Vec<LyricsLine> {
        vec![line(0, "Zero"), line(10_000, "Ten"), line(20_000, "Twenty")]
    }

    #[test]
    fn display_durations_become_seconds() {
        assert_eq!(duration_seconds("3:42"), Some(222));
        assert_eq!(duration_seconds("0:09"), Some(9));
        assert_eq!(duration_seconds("1:02:03"), Some(3_723));
    }

    #[test]
    fn a_malformed_duration_is_refused_rather_than_guessed() {
        assert_eq!(duration_seconds(""), None);
        assert_eq!(duration_seconds("222"), None);
        assert_eq!(duration_seconds("a:b"), None);
        assert_eq!(duration_seconds("1:2:3:4"), None);
        // Foundation's `split` drops the empty part and reads this as 1:02. An empty field is
        // not a duration, and this port refuses it.
        assert_eq!(duration_seconds("1::2"), None);
    }

    #[test]
    fn an_overflowing_duration_is_refused() {
        assert_eq!(duration_seconds("4294967295:59"), None);
    }

    #[test]
    fn the_highlight_follows_the_position() {
        let lines = timed();
        assert_eq!(active_line_index(&lines, true, 0), Some(0));
        assert_eq!(active_line_index(&lines, true, 9_999), Some(0));
        assert_eq!(active_line_index(&lines, true, 10_000), Some(1));
        assert_eq!(active_line_index(&lines, true, 25_000), Some(2));
    }

    #[test]
    fn nothing_is_highlighted_before_the_first_line() {
        let late = [line(5_000, "Starts late")];
        assert_eq!(active_line_index(&late, true, 0), None);
        assert_eq!(active_line_index(&late, true, 4_999), None);
        assert_eq!(active_line_index(&late, true, 5_000), Some(0));
    }

    #[test]
    fn unsynced_lyrics_never_highlight() {
        assert_eq!(
            active_line_index(&[line(-1, "Just words")], false, 9_999),
            None
        );
    }

    #[test]
    fn an_empty_document_never_highlights() {
        assert_eq!(active_line_index(&[], true, 1_000), None);
    }
}
