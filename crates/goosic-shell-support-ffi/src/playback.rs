//! Playback rules a shell asks about on every sample, slider drag and frame.
//!
//! These take and return plain numbers and flags rather than JSON. They run several times a
//! second, and JSON has no way to carry the NaN a player reports for an unknown length: both
//! shells' encoders refuse it, so a JSON form would have to be guarded at every call. A NaN
//! crosses a C `double` as itself, and each rule here already treats it as "not known".

use std::ffi::c_char;

use goosic_shell_support::{page_swap, playback};

use crate::{borrow, hand_over};

/// The gain, 0 to 1, a volume slider at `position` (0 to 1) plays at, on an audio taper.
#[no_mangle]
pub extern "C" fn goosic_volume_gain(position: f64) -> f64 {
    playback::volume_gain(position)
}

/// The slider position, 0 to 1, that plays at `gain`; the inverse of [`goosic_volume_gain`].
#[no_mangle]
pub extern "C" fn goosic_volume_position(gain: f64) -> f64 {
    playback::volume_position(gain)
}

/// `seconds` as `m:ss`, or `h:mm:ss` past an hour. Free the answer with
/// [`crate::goosic_string_free`].
#[no_mangle]
pub extern "C" fn goosic_time_text(seconds: f64) -> *mut c_char {
    hand_over(playback::time_text(seconds))
}

/// Whether a sample says the track has played through, including the page's pause a moment
/// short of the end when Automix is off.
///
/// # Safety
///
/// `state` must be NUL-terminated or null; null reads as no state at all.
#[no_mangle]
pub unsafe extern "C" fn goosic_track_has_finished(
    state: *const c_char,
    position: f64,
    duration: f64,
    listener_paused: bool,
) -> bool {
    let state = unsafe { borrow(state) }.unwrap_or("");
    playback::has_finished(state, position, duration, listener_paused)
}

/// Whether a sample shows the requested track itself playing.
///
/// # Safety
///
/// `state` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_track_heard_playing(
    state: *const c_char,
    duration: f64,
    is_advertisement: bool,
) -> bool {
    let state = unsafe { borrow(state) }.unwrap_or("");
    page_swap::heard_playing(state, duration, is_advertisement)
}

/// Whether the page's title names the song that was asked for, so another version of it may
/// stand in.
///
/// # Safety
///
/// Both pointers must be NUL-terminated or null; null never matches.
#[no_mangle]
pub unsafe extern "C" fn goosic_is_same_song(
    requested: *const c_char,
    page: *const c_char,
) -> bool {
    match (unsafe { borrow(requested) }, unsafe { borrow(page) }) {
        (Some(requested), Some(page)) => page_swap::is_same_song(requested, page),
        _ => false,
    }
}

/// [`goosic_early_switch_counted`] for a shell that still says only whether the load may be
/// replaced: `may_adopt` false is a choice already replaced as often as it may be.
///
/// Kept so a shell built against the earlier signature keeps compiling while it moves to the
/// counted one; it can go once both shells call that.
///
/// # Safety
///
/// As for [`goosic_early_switch_counted`].
#[no_mangle]
pub unsafe extern "C" fn goosic_early_switch(
    requested_video_id: *const c_char,
    page_video_id: *const c_char,
    page_title: *const c_char,
    may_adopt: bool,
    loaded_video_id: *const c_char,
    chosen_title: *const c_char,
) -> i32 {
    let adoptions = if may_adopt {
        0
    } else {
        page_swap::MAX_ADOPTIONS
    };
    unsafe {
        goosic_early_switch_counted(
            requested_video_id,
            page_video_id,
            page_title,
            adoptions,
            loaded_video_id,
            chosen_title,
        )
    }
}

/// What to do when the page switched to `page_video_id` before `requested_video_id` was heard:
/// `0` ignore it (the load is no longer current), `1` treat the track as refused, `2` adopt the
/// page's video, which is another version of the chosen song, in the requested one's place.
/// `adoptions` is how many versions were already loaded in place of the listener's choice; a
/// choice is replaced at most `MAX_ADOPTIONS` times.
///
/// # Safety
///
/// Every pointer must be NUL-terminated or null; `loaded_video_id` null means nothing is loaded.
#[no_mangle]
pub unsafe extern "C" fn goosic_early_switch_counted(
    requested_video_id: *const c_char,
    page_video_id: *const c_char,
    page_title: *const c_char,
    adoptions: u32,
    loaded_video_id: *const c_char,
    chosen_title: *const c_char,
) -> i32 {
    let (Some(requested), Some(page)) = (unsafe { borrow(requested_video_id) }, unsafe {
        borrow(page_video_id)
    }) else {
        return 0;
    };
    let switch = page_swap::EarlySwitch {
        requested_video_id: requested,
        page_video_id: page,
        page_title: unsafe { borrow(page_title) }.unwrap_or(""),
        adoptions,
    };
    let loaded = unsafe { borrow(loaded_video_id) };
    let chosen = unsafe { borrow(chosen_title) }.unwrap_or("");
    match page_swap::decide_early_switch(&switch, loaded, chosen) {
        page_swap::EarlySwitchDecision::Ignore => 0,
        page_swap::EarlySwitchDecision::Refused => 1,
        page_swap::EarlySwitchDecision::Adopt { .. } => 2,
    }
}

/// Where the position line sits for a seek to `target` asked for `elapsed_seconds` ago, when
/// the player reports `confirmed`. Writes the position to draw through `shown` and answers
/// whether the seek is settled and can be forgotten.
///
/// # Safety
///
/// `shown` must be null or point to a writable `double`.
#[no_mangle]
pub unsafe extern "C" fn goosic_seek_settle(
    confirmed: f64,
    target: f64,
    elapsed_seconds: f64,
    shown: *mut f64,
) -> bool {
    let answer = playback::settle_seek(confirmed, target, elapsed_seconds);
    if !shown.is_null() {
        unsafe { *shown = answer.shown };
    }
    answer.settled
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;
    use crate::goosic_string_free;

    #[test]
    fn the_scalar_rules_answer_through_the_c_abi() {
        assert!((goosic_volume_gain(0.5) - 0.125).abs() < 1e-12);
        assert!((goosic_volume_position(0.125) - 0.5).abs() < 1e-12);
        assert_eq!(goosic_volume_gain(f64::NAN), 0.0);

        let text = goosic_time_text(3_725.0);
        assert_eq!(unsafe { CStr::from_ptr(text) }.to_str().unwrap(), "1:02:05");
        unsafe { goosic_string_free(text) };

        let paused = CString::new("paused").unwrap();
        assert!(unsafe { goosic_track_has_finished(paused.as_ptr(), 228.8, 229.0, false) });
        assert!(!unsafe { goosic_track_has_finished(paused.as_ptr(), 228.8, 229.0, true) });
        assert!(!unsafe { goosic_track_has_finished(std::ptr::null(), 229.0, 229.0, false) });
        let playing = CString::new("playing").unwrap();
        assert!(unsafe { goosic_track_heard_playing(playing.as_ptr(), 230.0, false) });
        assert!(!unsafe { goosic_track_heard_playing(playing.as_ptr(), f64::NAN, false) });

        let chosen = CString::new("Hold On, We're Going Home").unwrap();
        let video = CString::new("Hold On, We're Going Home (Official Video)").unwrap();
        assert!(unsafe { goosic_is_same_song(chosen.as_ptr(), video.as_ptr()) });
        assert!(!unsafe { goosic_is_same_song(chosen.as_ptr(), std::ptr::null()) });

        let requested = CString::new("song0000001").unwrap();
        let version = CString::new("video000001").unwrap();
        let other = CString::new("Moment 4 Life").unwrap();
        let switch = |title: &CString, may_adopt: bool, loaded: *const c_char| unsafe {
            goosic_early_switch(
                requested.as_ptr(),
                version.as_ptr(),
                title.as_ptr(),
                may_adopt,
                loaded,
                chosen.as_ptr(),
            )
        };
        assert_eq!(switch(&video, true, requested.as_ptr()), 2);
        assert_eq!(switch(&video, false, requested.as_ptr()), 1);
        assert_eq!(switch(&other, true, requested.as_ptr()), 1);
        assert_eq!(switch(&video, true, version.as_ptr()), 0);
        assert_eq!(switch(&video, true, std::ptr::null()), 0);

        // A version swapped back once more is still adopted; a page going round is refused.
        let counted = |adoptions: u32| unsafe {
            goosic_early_switch_counted(
                requested.as_ptr(),
                version.as_ptr(),
                video.as_ptr(),
                adoptions,
                requested.as_ptr(),
                chosen.as_ptr(),
            )
        };
        assert_eq!(counted(0), 2);
        assert_eq!(counted(1), 2);
        assert_eq!(counted(2), 1);

        let mut shown = 0.0;
        assert!(!unsafe { goosic_seek_settle(30.0, 120.0, 0.3, &mut shown) });
        assert_eq!(shown, 120.0);
        assert!(unsafe { goosic_seek_settle(119.5, 120.0, 0.3, &mut shown) });
        assert_eq!(shown, 119.5);
        assert!(unsafe { goosic_seek_settle(1.0, 2.0, 0.0, std::ptr::null_mut()) });
    }
}
