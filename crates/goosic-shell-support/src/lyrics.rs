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
        LyricsLine { at_ms, text: text.into() }
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
        assert_eq!(active_line_index(&[line(-1, "Just words")], false, 9_999), None);
    }

    #[test]
    fn an_empty_document_never_highlights() {
        assert_eq!(active_line_index(&[], true, 1_000), None);
    }
}
