//! Words the player shows about the queue, a list's length and the sleep timer.
//!
//! These were written twice, in `ListRules.swift` and the C# `ListRules.cs`, and held to the same
//! test cases by hand. They are product copy rather than layout, so every shell shows the same
//! sentence for the same state, and they live here once.

/// "From Liked Music · 99 songs": where the queue came from, then what is left.
pub fn up_next(source: Option<&str>, remaining: usize, finding_more: bool) -> String {
    let prefix = match source {
        Some(source) if !source.is_empty() => format!("From {source} · "),
        _ => String::new(),
    };
    let count = match remaining {
        0 if finding_more => "Finding songs like this…".to_owned(),
        0 => "Nothing after this".to_owned(),
        1 => "1 song".to_owned(),
        n => format!("{n} songs"),
    };
    prefix + &count
}

/// "100 songs", or "100+ songs" while a continuation is still pending: a partial list must not
/// present itself as complete.
pub fn song_count(count: usize, has_more: bool) -> String {
    if count == 1 && !has_more {
        return "1 song".to_owned();
    }
    let plus = if has_more { "+" } else { "" };
    format!("{count}{plus} songs")
}

/// The sleep timer's menu label: off, at the end of the song, or the minutes left.
pub fn sleep_timer(remaining_seconds: Option<f64>, end_of_song: bool) -> String {
    if end_of_song {
        return "Sleep timer: end of song".to_owned();
    }
    let Some(remaining) = remaining_seconds.filter(|value| value.is_finite()) else {
        return "Sleep timer".to_owned();
    };
    // Rounded up, so the last half-minute reads "1 min left" rather than "0".
    let minutes = ((remaining / 60.0).ceil() as i64).max(1);
    format!("Sleep timer: {minutes} min left")
}

/// The title a song radio's queue is shown under.
pub fn radio_source(title: &str) -> String {
    format!("{title} radio")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_next_names_the_source_then_what_is_left() {
        assert_eq!(
            up_next(Some("Liked Music"), 99, false),
            "From Liked Music · 99 songs"
        );
        assert_eq!(up_next(Some("Songs"), 1, false), "From Songs · 1 song");
        assert_eq!(up_next(None, 0, false), "Nothing after this");
        assert_eq!(up_next(Some(""), 0, true), "Finding songs like this…");
        assert_eq!(
            up_next(Some("Mix"), 0, true),
            "From Mix · Finding songs like this…"
        );
    }

    #[test]
    fn a_partial_count_says_so() {
        assert_eq!(song_count(1, false), "1 song");
        assert_eq!(song_count(1, true), "1+ songs");
        assert_eq!(song_count(100, true), "100+ songs");
        assert_eq!(song_count(0, false), "0 songs");
    }

    #[test]
    fn the_sleep_timer_rounds_up_and_never_reads_zero() {
        assert_eq!(sleep_timer(None, false), "Sleep timer");
        assert_eq!(sleep_timer(Some(10.0), true), "Sleep timer: end of song");
        assert_eq!(sleep_timer(Some(20.0), false), "Sleep timer: 1 min left");
        assert_eq!(sleep_timer(Some(61.0), false), "Sleep timer: 2 min left");
        assert_eq!(sleep_timer(Some(0.0), false), "Sleep timer: 1 min left");
        assert_eq!(sleep_timer(Some(f64::NAN), false), "Sleep timer");
    }
}
