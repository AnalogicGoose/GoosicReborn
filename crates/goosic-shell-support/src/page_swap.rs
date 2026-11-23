//! What a shell makes of the official page replacing the requested track with another video.
//!
//! YouTube Music's page sometimes reports a different video before any of the requested track
//! has played. Taking that for the track's end skipped the song and started the next row of the
//! list without a word. Before the requested track has been heard, a switch is one of three
//! things: the page passing through another video while it loads, another version of the same
//! song (the audio track for the music video, or back), or the page refusing the track.
//!
//! The Windows shell's copy is `PlaybackOrder.IsSameSong` and the early-switch handling in its
//! `OfficialPlaybackHost`; the macOS copy is `PageSwap` in `TrackEnd.swift`. Until those shells
//! consume this crate, the three are held to the same cases, and a change to one is a change to
//! all of them.

/// Reports in a row that describe another video before a shell acts on them.
///
/// One alone can be the page still showing what its player held before it reached the requested
/// video; two in a row is the page having moved. The page's observer reports every half second,
/// so a real switch is acted on within about a second.
pub const STRAY_REPORTS_TO_ACT: u32 = 2;

/// Whether a sample shows the requested track itself playing.
///
/// Until one does, a switch to another video under the same load cannot be the track's end.
pub fn heard_playing(state: &str, duration: f64, is_advertisement: bool) -> bool {
    state == "playing" && !is_advertisement && duration.is_finite() && duration > 0.0
}

/// Whether the page's title names the song that was asked for.
///
/// The titles of two versions differ only by decoration such as "(Official Video)" or
/// "(feat. …)", so both are reduced to their words, and one must contain the other as whole
/// words, so "Down" is not "Downtown". An empty title on either side never matches.
pub fn is_same_song(requested: &str, page: &str) -> bool {
    let requested = song_words(requested);
    let page = song_words(page);
    if requested.is_empty() || page.is_empty() {
        return false;
    }
    format!(" {page} ").contains(&format!(" {requested} "))
        || format!(" {requested} ").contains(&format!(" {page} "))
}

fn song_words(title: &str) -> String {
    let mut text = String::with_capacity(title.len());
    let mut depth = 0usize;
    // Drop bracketed decoration: (Official Video), [Lyrics], (feat. …) and the like.
    for character in title.chars() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => {
                depth -= 1;
                text.push(' ');
            }
            _ if depth == 0 => text.push(character),
            _ => {}
        }
    }
    let mut words = String::with_capacity(text.len());
    for character in text.to_lowercase().chars() {
        // An apostrophe joins a word rather than splitting it: "don't" and "dont" are one word.
        if character == '\'' || character == '\u{2019}' {
            continue;
        }
        words.push(if character.is_alphanumeric() {
            character
        } else {
            ' '
        });
    }
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A switch the page made before the requested track was heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlySwitch<'a> {
    /// The video the shell had loaded when the page switched.
    pub requested_video_id: &'a str,
    /// The video the page switched to.
    pub page_video_id: &'a str,
    /// The page's own title for that video; empty when it could not be read.
    pub page_title: &'a str,
    /// Whether the load was the listener's own choice rather than a version already loaded in its
    /// place. A version may be swapped in once; swapping again is the page wandering.
    pub may_adopt: bool,
}

/// What a shell does with an [`EarlySwitch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EarlySwitchDecision {
    /// Load the page's version in its place, still presented as the song the listener chose.
    Adopt { video_id: String },
    /// The page would not play the requested track: say so and move past it.
    Refused,
    /// The report is about a load that is no longer the current one.
    Ignore,
}

/// Decides an early switch. `loaded_video_id` is what the shell has loaded now, which may be a
/// version adopted earlier; `chosen_title` is the title of the song the listener chose.
pub fn decide_early_switch(
    switch: &EarlySwitch<'_>,
    loaded_video_id: Option<&str>,
    chosen_title: &str,
) -> EarlySwitchDecision {
    if loaded_video_id != Some(switch.requested_video_id) {
        return EarlySwitchDecision::Ignore;
    }
    if switch.may_adopt
        && switch.page_video_id != switch.requested_video_id
        && is_same_song(chosen_title, switch.page_title)
    {
        return EarlySwitchDecision::Adopt {
            video_id: switch.page_video_id.to_owned(),
        };
    }
    EarlySwitchDecision::Refused
}

#[cfg(test)]
mod tests {
    use super::*;

    // The same cases as the Windows shell's `SameSongTests` and the macOS `PageSwapTests`.
    #[test]
    fn another_version_of_the_song_matches() {
        for (requested, page) in [
            ("Hideaway", "Hideaway"),
            ("Hideaway", "Bella Poarch - Hideaway (Official Video)"),
            ("Swim", "SWIM [Official Audio]"),
            ("I Adore You (feat. Daecolm)", "I Adore You"),
            ("Don't Stop Me Now", "Dont Stop Me Now (Remastered 2011)"),
            (
                "Hold On, We\u{2019}re Going Home",
                "Hold On, We're Going Home (feat. Majid Jordan)",
            ),
        ] {
            assert!(is_same_song(requested, page), "{requested} / {page}");
        }
    }

    #[test]
    fn a_different_song_does_not() {
        for (requested, page) in [
            ("Down", "Downtown"),
            ("Hideaway", "Ribcage"),
            ("Hideaway", ""),
            ("", "Hideaway"),
            ("(Intro)", "Anything"),
            ("Hold On, We're Going Home", "Moment 4 Life (feat. Drake)"),
        ] {
            assert!(!is_same_song(requested, page), "{requested} / {page}");
        }
    }

    #[test]
    fn only_the_requested_track_playing_counts_as_heard() {
        assert!(heard_playing("playing", 230.0, false));
        assert!(!heard_playing("playing", 0.0, false));
        assert!(!heard_playing("playing", f64::NAN, false));
        assert!(!heard_playing("paused", 230.0, false));
        assert!(!heard_playing("playing", 15.0, true));
    }

    fn switch<'a>(page: &'a str, title: &'a str, may_adopt: bool) -> EarlySwitch<'a> {
        EarlySwitch {
            requested_video_id: "requested01",
            page_video_id: page,
            page_title: title,
            may_adopt,
        }
    }

    #[test]
    fn the_same_song_under_another_id_is_adopted() {
        assert_eq!(
            decide_early_switch(
                &switch(
                    "musicvideo1",
                    "Hold On, We're Going Home (feat. Majid Jordan)",
                    true
                ),
                Some("requested01"),
                "Hold On, We\u{2019}re Going Home",
            ),
            EarlySwitchDecision::Adopt {
                video_id: "musicvideo1".to_owned()
            }
        );
    }

    #[test]
    fn another_song_or_an_unread_title_is_a_refusal() {
        let chosen = "Hold On, We're Going Home";
        let other = switch("othersong01", "Moment 4 Life (feat. Drake)", true);
        assert_eq!(
            decide_early_switch(&other, Some("requested01"), chosen),
            EarlySwitchDecision::Refused
        );
        assert_eq!(
            decide_early_switch(
                &switch("othersong01", "", true),
                Some("requested01"),
                chosen
            ),
            EarlySwitchDecision::Refused
        );
    }

    #[test]
    fn an_adopted_version_is_not_swapped_again() {
        assert_eq!(
            decide_early_switch(
                &switch("thirdone001", "Hold On, We're Going Home", false),
                Some("requested01"),
                "Hold On, We're Going Home",
            ),
            EarlySwitchDecision::Refused
        );
    }

    #[test]
    fn a_report_for_an_earlier_load_is_ignored() {
        assert_eq!(
            decide_early_switch(
                &switch("othersong01", "Anything", true),
                Some("nextsong001"),
                "x"
            ),
            EarlySwitchDecision::Ignore
        );
        assert_eq!(
            decide_early_switch(&switch("othersong01", "Anything", true), None, "x"),
            EarlySwitchDecision::Ignore
        );
    }
}
