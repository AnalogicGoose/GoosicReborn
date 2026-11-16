//! Presentation-only lyric sweep, rest and row-motion rules shared by native shells.
//!
//! Swift and C# currently keep tested copies until their rule bridges expose these functions.
//! These spans shape drawing only: `lyrics::word_progress` still uses supplied ends exclusively.

use goosic_protocol::LyricsLine;

pub const ROW_DELAY_MS: u64 = 50;
pub const MAX_DELAYED_ROWS: usize = 10;
pub const ROW_SPRING_PERIOD: f64 = 0.4;
pub const ANCHOR: f64 = 0.36;
pub const PAUSED_COVER_SCALE: f64 = 0.74;
pub const REST_MIN_MS: i64 = 1_000;
pub const INTERLUDE_MIN_MS: i64 = 4_000;

/// One word's drawing span. An inferred span never changes its protocol timing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sweep {
    pub start_ms: f64,
    pub end_ms: f64,
    pub supplied: bool,
}

impl Sweep {
    pub fn progress(self, position_ms: f64) -> f64 {
        if !position_ms.is_finite() || self.end_ms <= self.start_ms {
            return 0.0;
        }
        ((position_ms - self.start_ms) / (self.end_ms - self.start_ms)).clamp(0.0, 1.0)
    }
}

/// Start-only words use the next supplied start, capped at 2.5 seconds. The last uses the
/// supplied line end, or a fixed 450 ms. Equal starts settle over 220 ms.
pub fn sweeps(line: &LyricsLine) -> Vec<Sweep> {
    line.words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let start_ms = word.at_ms as f64;
            if let Some(end) = word.end_ms.filter(|end| *end > word.at_ms) {
                return Sweep {
                    start_ms,
                    end_ms: end as f64,
                    supplied: true,
                };
            }
            let end_ms = if let Some(next) = line.words.get(index + 1) {
                if next.at_ms > word.at_ms {
                    (next.at_ms as f64).min(start_ms + 2_500.0)
                } else {
                    start_ms + 220.0
                }
            } else if let Some(end) = line.end_ms.filter(|end| *end > word.at_ms) {
                (end as f64).min(start_ms + 2_500.0)
            } else {
                start_ms + 450.0
            };
            Sweep {
                start_ms,
                end_ms,
                supplied: false,
            }
        })
        .collect()
}

/// A supplied end creates a rest only when at least a second remains before the next line.
pub fn rest_start(lines: &[LyricsLine], index: usize) -> Option<i64> {
    let line = lines.get(index)?;
    let end = line.end_ms.filter(|end| *end > line.at_ms)?;
    if lines
        .get(index + 1)
        .is_some_and(|next| next.at_ms.saturating_sub(end) < REST_MIN_MS)
    {
        return None;
    }
    Some(end)
}

/// Provider-marked silence; its row keeps its height outside the active stretch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interlude {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl Interlude {
    pub fn progress(self, position_ms: f64) -> f64 {
        if !position_ms.is_finite() || self.end_ms <= self.start_ms {
            return 0.0;
        }
        ((position_ms - self.start_ms as f64) / (self.end_ms as f64 - self.start_ms as f64))
            .clamp(0.0, 1.0)
    }

    pub fn dot_opacity(self, dot: usize, position_ms: f64) -> f64 {
        0.25 + 0.75 * (self.progress(position_ms) * 3.0 - dot as f64).clamp(0.0, 1.0)
    }

    pub fn scale(self, position_ms: f64, animated: bool) -> f64 {
        if !animated || !position_ms.is_finite() {
            return 1.0;
        }
        let breath = 1.0 + 0.08 * (position_ms / 1_800.0 * std::f64::consts::TAU).sin();
        let remaining = self.end_ms as f64 - position_ms;
        if remaining >= 600.0 {
            return breath;
        }
        let x = (1.0 - remaining / 600.0).clamp(0.0, 1.0);
        breath
            * if x < 0.35 {
                1.0 + 0.25 * x / 0.35
            } else {
                1.25 * (1.0 - (x - 0.35) / 0.65)
            }
    }
}

pub fn introduction(lines: &[LyricsLine], synced: bool) -> Option<Interlude> {
    let first = lines.first()?;
    (synced && first.at_ms >= INTERLUDE_MIN_MS).then_some(Interlude {
        start_ms: 0,
        end_ms: first.at_ms,
    })
}

pub fn empty_line(lines: &[LyricsLine], synced: bool, index: usize) -> Option<Interlude> {
    let line = lines.get(index)?;
    let next = lines.get(index + 1)?;
    (synced
        && line.text.trim().is_empty()
        && next.at_ms.saturating_sub(line.at_ms) >= INTERLUDE_MIN_MS)
        .then_some(Interlude {
            start_ms: line.at_ms,
            end_ms: next.at_ms,
        })
}

pub fn interlude_after(lines: &[LyricsLine], synced: bool, index: usize) -> Option<Interlude> {
    let line = lines.get(index)?;
    let next = lines.get(index + 1)?;
    let end = line.end_ms.filter(|end| *end > line.at_ms)?;
    (synced && !line.text.trim().is_empty() && next.at_ms.saturating_sub(end) >= INTERLUDE_MIN_MS)
        .then_some(Interlude {
            start_ms: end,
            end_ms: next.at_ms,
        })
}

pub fn row_delay_ms(order: usize, leader: usize) -> u64 {
    ROW_DELAY_MS * order.saturating_sub(leader).min(MAX_DELAYED_ROWS) as u64
}

pub fn covered(seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    let omega = std::f64::consts::TAU / ROW_SPRING_PERIOD;
    1.0 - (1.0 + omega * seconds) * (-omega * seconds).exp()
}

pub fn padding(viewport: f64, immersive: bool) -> (f64, f64) {
    let least: f64 = if immersive { 160.0 } else { 120.0 };
    (
        least.max(viewport * ANCHOR),
        least.max(viewport * (1.0 - ANCHOR)),
    )
}

pub fn target(row_top: f64, row_height: f64, viewport: f64, scrollable: f64) -> f64 {
    (row_top + row_height * ANCHOR - viewport * ANCHOR).clamp(0.0, scrollable.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::LyricsWord;

    fn line(at_ms: i64, end_ms: Option<i64>, text: &str) -> LyricsLine {
        LyricsLine {
            at_ms,
            end_ms,
            text: text.into(),
            words: vec![],
        }
    }

    #[test]
    fn sweeps_preserve_supplied_ends_and_bound_start_only_words() {
        let mut line = line(0, Some(8_000), "abcd");
        line.words = [
            (0, Some(500), "a"),
            (500, None, "b"),
            (500, None, "c"),
            (5_000, None, "d"),
        ]
        .into_iter()
        .map(|(at_ms, end_ms, text)| LyricsWord {
            at_ms,
            end_ms,
            text: text.into(),
        })
        .collect();
        let spans = sweeps(&line);
        assert_eq!(
            spans.iter().map(|span| span.end_ms).collect::<Vec<_>>(),
            [500.0, 720.0, 3_000.0, 7_500.0]
        );
        assert!(spans[0].supplied);
        assert!(!spans[1].supplied);
        assert_eq!(spans[0].progress(250.0), 0.5);
        assert_eq!(spans[0].progress(f64::NAN), 0.0);
        line.end_ms = None;
        assert_eq!(sweeps(&line)[3].end_ms, 5_450.0);
    }

    #[test]
    fn rests_and_dots_require_provider_marked_silence() {
        let mut lines = vec![line(4_000, Some(6_000), "sung"), line(10_000, None, "next")];
        assert_eq!(rest_start(&lines, 0), Some(6_000));
        let gap = interlude_after(&lines, true, 0).unwrap();
        assert_eq!(gap.dot_opacity(0, 6_000.0), 0.25);
        assert_eq!(gap.dot_opacity(2, 10_000.0), 1.0);
        assert_eq!(gap.scale(10_000.0, false), 1.0);
        assert!(gap.scale(10_000.0, true).abs() < 1e-12);
        assert!(introduction(&lines, true).is_some());
        assert!(introduction(&lines, false).is_none());
        lines[1].at_ms = 6_999;
        assert_eq!(rest_start(&lines, 0), None);
        lines[1].at_ms = 7_000;
        assert_eq!(rest_start(&lines, 0), Some(6_000));
        assert_eq!(interlude_after(&lines, true, 0), None);
        lines[0].end_ms = None;
        assert_eq!(rest_start(&lines, 0), None);
        lines[0].text.clear();
        lines[1].at_ms = 8_000;
        assert!(empty_line(&lines, true, 0).is_some());
    }

    #[test]
    fn rows_match_native_spring_and_anchor_cases() {
        assert_eq!(row_delay_ms(2, 3), 0);
        assert_eq!(row_delay_ms(4, 3), 50);
        assert_eq!(row_delay_ms(30, 3), 500);
        assert!((covered(0.1) - 0.466).abs() < 0.01);
        assert!((covered(0.34) - 0.97).abs() < 0.01);
        assert_eq!(padding(500.0, true), (180.0, 320.0));
        assert_eq!(target(400.0, 100.0, 500.0, 1_000.0), 256.0);
        assert_eq!(target(0.0, 100.0, 500.0, 0.0), 0.0);
    }
}
