//! LRC parsing.
//!
//! Pure and fully unit-tested: the network side of this crate has nothing to decide, so
//! everything worth getting right lives here.

use goosic_protocol::LyricsLine;

/// The longest line this parser will keep. Lyrics lines are short; anything past this is not a
/// lyric and would only bloat a protocol frame.
const MAX_LINE_CHARS: usize = 512;
/// A guard on absurd inputs. Real lyrics run to a few hundred lines.
const MAX_LINES: usize = 2_000;

/// Parses a timestamp tag body such as `01:23.45`, `01:23.456`, or `01:23`.
///
/// Returns milliseconds, or `None` when the tag is metadata (`ar:`, `ti:`, …) rather than a time.
fn parse_timestamp(body: &str) -> Option<i64> {
    // `h:mm:ss.xx` carries hours. A fraction after a second colon is how `mm:ss:xx` files write
    // hundredths, so the hour form is recognised only when a dot follows two colons.
    if let Some((clock, fraction)) = body.split_once('.') {
        if clock.matches(':').count() == 2 {
            let (hours, rest) = clock.split_once(':')?;
            let hours: i64 = hours.trim().parse().ok()?;
            if hours < 0 {
                return None;
            }
            let minutes: i64 = rest.split_once(':')?.0.trim().parse().ok()?;
            if !(0..60).contains(&minutes) {
                return None;
            }
            let within = parse_timestamp(&format!("{rest}.{fraction}"))?;
            return Some(hours * 3_600_000 + within);
        }
    }
    let (minutes, rest) = body.split_once(':')?;
    let minutes: i64 = minutes.trim().parse().ok()?;
    if minutes < 0 {
        return None;
    }

    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (rest, None),
    };
    let seconds: i64 = seconds.trim().parse().ok()?;
    if !(0..60).contains(&seconds) {
        return None;
    }

    let milliseconds = match fraction {
        None => 0,
        Some(fraction) => {
            let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                return None;
            }
            // LRC writes hundredths far more often than milliseconds, so the field is scaled by
            // its own width rather than assumed.
            let value: i64 = digits.parse().ok()?;
            match digits.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value / 10_i64.pow(digits.len() as u32 - 3),
            }
        }
    };

    Some(minutes * 60_000 + seconds * 1_000 + milliseconds)
}

/// Reads an `[offset:+250]` tag body as milliseconds, or `None` when the tag is something else.
fn parse_offset(body: &str) -> Option<i64> {
    let (name, value) = body.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case("offset") {
        return None;
    }
    value.trim().trim_start_matches('+').parse().ok()
}

/// Splits the leading `[...]` tags off a line, returning them with the remaining text.
fn split_tags(line: &str) -> (Vec<&str>, &str) {
    let mut tags = Vec::new();
    let mut rest = line;
    while let Some(stripped) = rest.strip_prefix('[') {
        let Some(end) = stripped.find(']') else { break };
        tags.push(&stripped[..end]);
        rest = &stripped[end + 1..];
    }
    (tags, rest)
}

/// Parses synced LRC into timed lines, sorted by time.
///
/// One line may carry several timestamps when a phrase repeats; each becomes its own entry.
/// Metadata tags and untimed lines are dropped, because a line with no time cannot be
/// highlighted and would only be noise next to timed ones.
///
/// The first `[offset:N]` tag shifts every line. By the LRC convention a positive offset makes
/// lyrics appear sooner, so it is subtracted from each time, never below zero.
pub fn parse_lrc(source: &str) -> Vec<LyricsLine> {
    let mut lines: Vec<LyricsLine> = Vec::new();
    let mut offset: Option<i64> = None;
    for raw in source.lines() {
        let (tags, text) = split_tags(raw.trim());
        if tags.is_empty() {
            continue;
        }
        let text = text.trim();
        if text.chars().count() > MAX_LINE_CHARS {
            continue;
        }
        for tag in tags {
            if let Some(value) = parse_offset(tag) {
                offset.get_or_insert(value);
                continue;
            }
            let Some(at_ms) = parse_timestamp(tag) else {
                continue;
            };
            if lines.len() >= MAX_LINES {
                break;
            }
            lines.push(LyricsLine {
                end_ms: None,
                words: Vec::new(),
                at_ms,
                text: text.to_owned(),
            });
        }
    }
    if let Some(offset) = offset.filter(|offset| *offset != 0) {
        for line in &mut lines {
            line.at_ms = line.at_ms.saturating_sub(offset).max(0);
        }
    }
    // A stable sort keeps repeated timestamps in the order the file wrote them.
    lines.sort_by_key(|line| line.at_ms);
    lines
}

/// Turns unsynced lyrics into lines with no timing.
pub fn parse_plain(source: &str) -> Vec<LyricsLine> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().count() <= MAX_LINE_CHARS)
        .take(MAX_LINES)
        .map(|line| LyricsLine {
            end_ms: None,
            words: Vec::new(),
            at_ms: -1,
            text: line.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundredths_and_milliseconds_are_both_understood() {
        assert_eq!(parse_timestamp("00:01.5"), Some(1_500));
        assert_eq!(parse_timestamp("00:01.50"), Some(1_500));
        assert_eq!(parse_timestamp("00:01.500"), Some(1_500));
        assert_eq!(parse_timestamp("01:23.45"), Some(83_450));
        assert_eq!(parse_timestamp("01:23"), Some(83_000));
    }

    #[test]
    fn a_colon_separated_fraction_is_accepted_because_some_files_use_it() {
        assert_eq!(parse_timestamp("01:23:45"), Some(83_450));
    }

    #[test]
    fn metadata_tags_are_not_timestamps() {
        assert_eq!(parse_timestamp("ar:Signal Fires"), None);
        assert_eq!(parse_timestamp("ti:Afterglow"), None);
        assert_eq!(parse_timestamp("length"), None);
    }

    #[test]
    fn impossible_times_are_refused() {
        assert_eq!(parse_timestamp("00:60.00"), None);
        assert_eq!(parse_timestamp("-1:00.00"), None);
        assert_eq!(parse_timestamp("00:01."), None);
    }

    #[test]
    fn hour_timestamps_are_read_as_hours() {
        assert_eq!(parse_timestamp("1:02:03.00"), Some(3_723_000));
        assert_eq!(parse_timestamp("0:00:01.50"), Some(1_500));
        assert_eq!(parse_timestamp("1:60:00.00"), None);
        let lines = parse_lrc(
            "[1:02:03.00]Late
",
        );
        assert_eq!(lines[0].at_ms, 3_723_000);
    }

    #[test]
    fn a_positive_offset_makes_lyrics_sooner_and_clamps_at_zero() {
        let lines = parse_lrc(
            "[offset:+500]
[00:01.00]A
[00:00.20]B
[00:03.00]C
",
        );
        let times: Vec<i64> = lines.iter().map(|line| line.at_ms).collect();
        assert_eq!(times, vec![0, 500, 2_500]);
    }

    #[test]
    fn a_negative_offset_makes_lyrics_later_and_only_the_first_tag_counts() {
        let lines = parse_lrc(
            "[offset:-250]
[offset:900]
[00:01.00]A
",
        );
        assert_eq!(lines[0].at_ms, 1_250);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn a_malformed_offset_is_ignored() {
        let lines = parse_lrc(
            "[offset:soon]
[00:01.00]A
",
        );
        assert_eq!(lines[0].at_ms, 1_000);
    }

    #[test]
    fn a_synced_file_becomes_timed_lines_in_order() {
        let lines = parse_lrc(
            "[ar:Signal Fires]\n[ti:Afterglow]\n[00:12.00]First line\n[00:05.50]Earlier line\n",
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at_ms, 5_500);
        assert_eq!(lines[0].text, "Earlier line");
        assert_eq!(lines[1].at_ms, 12_000);
    }

    #[test]
    fn one_line_with_several_timestamps_repeats_at_each() {
        let lines = parse_lrc("[00:10.00][01:10.00]Chorus\n");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.text == "Chorus"));
        assert_eq!(lines[0].at_ms, 10_000);
        assert_eq!(lines[1].at_ms, 70_000);
    }

    #[test]
    fn untimed_and_metadata_only_lines_are_dropped_from_synced_output() {
        let lines = parse_lrc("[ar:Someone]\nA bare line\n[00:01.00]Kept\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Kept");
    }

    #[test]
    fn an_empty_timed_line_is_kept_because_it_marks_a_pause() {
        let lines = parse_lrc("[00:01.00]\n[00:02.00]Words\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "");
    }

    #[test]
    fn plain_lyrics_have_no_timing() {
        let lines = parse_plain("First\nSecond\n");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.at_ms < 0));
        assert_eq!(lines[1].text, "Second");
    }

    #[test]
    fn an_absurdly_long_line_is_dropped_rather_than_shipped() {
        let long = "x".repeat(MAX_LINE_CHARS + 1);
        assert!(parse_lrc(&format!("[00:01.00]{long}\n")).is_empty());
        assert!(parse_plain(&long).is_empty());
    }

    #[test]
    fn line_count_is_bounded() {
        let source: String = (0..MAX_LINES + 500)
            .map(|index| format!("[00:{:02}.00]line\n", index % 60))
            .collect();
        assert!(parse_lrc(&source).len() <= MAX_LINES);
    }
}
