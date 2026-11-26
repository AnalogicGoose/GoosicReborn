//! The rest of the shared rules a shell asks: page scripts, artwork, the playing cover's shape, an
//! artist hero, advertisement recovery and the system media controls.
//!
//! Each was once restated in Swift or C# beside the Rust original and held to the same cases by
//! hand. They cross as plain values and `#[repr(C)]` structs, for the reason [`crate::playback`]
//! gives: positions and lengths may be NaN, which JSON cannot carry.

use std::ffi::c_char;

use goosic_protocol::Owner;
use goosic_shell_support::catalog::Track;
use goosic_shell_support::media::{
    CommandAvailability, MediaPlaybackState, MediaSnapshot, NowPlaying,
};
use goosic_shell_support::navigation::PlaybackTransition;
use goosic_shell_support::playback::AdPlaybackRecovery;
use goosic_shell_support::{artwork, bridge, hero, login};

use crate::{borrow, hand_over};

// ---- Page scripts ----------------------------------------------------------------------------

/// The expression that picks the page's active media element.
#[no_mangle]
pub extern "C" fn goosic_bridge_active_media_element_script() -> *mut c_char {
    hand_over(bridge::ACTIVE_MEDIA_ELEMENT_SCRIPT.to_owned())
}

/// The script that turns the page's Automix off, answering `absent`, `off`, `turned off` or
/// `still on`.
#[no_mangle]
pub extern "C" fn goosic_bridge_automix_off_script() -> *mut c_char {
    hand_over(bridge::AUTOMIX_OFF_SCRIPT.to_owned())
}

/// How many times, half a second apart, a host runs the Automix script while it answers `absent`.
#[no_mangle]
pub extern "C" fn goosic_bridge_automix_attempts() -> u32 {
    bridge::AUTOMIX_ATTEMPTS
}

/// What a host appends to its engine's user agent so YouTube Music runs its player.
#[no_mangle]
pub extern "C" fn goosic_bridge_safari_user_agent_suffix() -> *mut c_char {
    hand_over(bridge::SAFARI_USER_AGENT_SUFFIX.to_owned())
}

/// The body of the async function a login surface runs to recognise a finished sign-in.
#[no_mangle]
pub extern "C" fn goosic_login_completion_script() -> *mut c_char {
    hand_over(login::COMPLETION_SCRIPT.to_owned())
}

/// How long, in seconds, a staged sign-in may sit on one page before it is abandoned.
#[no_mangle]
pub extern "C" fn goosic_login_completion_timeout_seconds() -> f64 {
    login::COMPLETION_TIMEOUT.as_secs_f64()
}

/// Whether `host` is one of Google's sign-in services, on any of its country domains.
///
/// # Safety
///
/// `host` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_login_is_google_sign_in_host(host: *const c_char) -> bool {
    unsafe { borrow(host) }.is_some_and(login::is_google_sign_in_host)
}

// ---- Artwork ---------------------------------------------------------------------------------

/// Whether artwork may be fetched from `url`.
///
/// # Safety
///
/// `url` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_artwork_is_allowed(url: *const c_char) -> bool {
    unsafe { borrow(url) }.is_some_and(artwork::is_allowed)
}

/// The file name a cover from `remote` is cached under.
///
/// # Safety
///
/// `remote` must be NUL-terminated or null; null answers null.
#[no_mangle]
pub unsafe extern "C" fn goosic_artwork_cache_key(remote: *const c_char) -> *mut c_char {
    match unsafe { borrow(remote) } {
        Some(remote) => hand_over(artwork::cache_key(remote)),
        None => std::ptr::null_mut(),
    }
}

/// The largest artwork download accepted, in bytes.
#[no_mangle]
pub extern "C" fn goosic_artwork_max_bytes() -> u64 {
    artwork::MAX_BYTES as u64
}

/// How many artwork downloads may run at once.
#[no_mangle]
pub extern "C" fn goosic_artwork_max_concurrent_fetches() -> u32 {
    artwork::MAX_CONCURRENT_FETCHES as u32
}

/// The width-to-height ratio the playing cover is drawn at, held between 9:16 and 16:9.
#[no_mangle]
pub extern "C" fn goosic_cover_aspect(pixel_width: f64, pixel_height: f64) -> f64 {
    artwork::cover_aspect(pixel_width, pixel_height)
}

/// The size a cover of `aspect` takes inside a slot, written through `width` and `height`.
///
/// # Safety
///
/// `width` and `height` must each be null or point to a writable `double`.
#[no_mangle]
pub unsafe extern "C" fn goosic_cover_fit(
    aspect: f64,
    max_width: f64,
    max_height: f64,
    width: *mut f64,
    height: *mut f64,
) {
    let (fitted_width, fitted_height) = artwork::fit_cover(aspect, max_width, max_height);
    if !width.is_null() {
        unsafe { *width = fitted_width };
    }
    if !height.is_null() {
        unsafe { *height = fitted_height };
    }
}

// ---- Artist hero -----------------------------------------------------------------------------

/// An artist hero's height for a page `width` wide.
#[no_mangle]
pub extern "C" fn goosic_hero_height(width: f64) -> f64 {
    hero::height(width)
}

/// The colour an artist page takes from its banner, written as three sRGB bytes to `rgb`.
/// Answers false, writing nothing, for a bitmap too small to hold what it claims.
///
/// # Safety
///
/// `rgba` must point to `length` readable bytes, and `rgb` to three writable ones.
#[no_mangle]
pub unsafe extern "C" fn goosic_hero_tint(
    rgba: *const u8,
    length: usize,
    width: usize,
    height: usize,
    rgb: *mut u8,
) -> bool {
    if rgba.is_null() || rgb.is_null() {
        return false;
    }
    let pixels = unsafe { std::slice::from_raw_parts(rgba, length) };
    match hero::tint(pixels, width, height) {
        Some(colour) => {
            unsafe { std::ptr::copy_nonoverlapping(colour.as_ptr(), rgb, 3) };
            true
        }
        None => false,
    }
}

// ---- Advertisement recovery ------------------------------------------------------------------

/// What [`goosic_ad_recovery_observe`] remembers between samples. A shell holds one per load,
/// zeroed when the load starts.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct GoosicAdRecovery {
    /// An advertisement played and content has not been heard since; no end counts meanwhile.
    pub awaiting_content: bool,
    pub resume_requested: bool,
}

/// Feeds one sample to the advertisement recovery and answers whether to ask the page to play:
/// content left paused by the handoff from an advertisement is resumed once, and a listener's
/// own pause always wins.
///
/// # Safety
///
/// `recovery` must point to a live [`GoosicAdRecovery`]; `state` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_ad_recovery_observe(
    recovery: *mut GoosicAdRecovery,
    advertisement: bool,
    state: *const c_char,
    listener_paused: bool,
) -> bool {
    if recovery.is_null() {
        return false;
    }
    let held = unsafe { &mut *recovery };
    let mut rule = AdPlaybackRecovery {
        awaiting_content: held.awaiting_content,
        resume_requested: held.resume_requested,
    };
    let state = unsafe { borrow(state) }.unwrap_or("");
    let resume = rule.observe(advertisement, state, listener_paused);
    held.awaiting_content = rule.awaiting_content;
    held.resume_requested = rule.resume_requested;
    resume
}

// ---- System media controls -------------------------------------------------------------------

/// The confirmed state the system media controls are computed from.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct GoosicMediaInput {
    pub has_track: bool,
    /// True only after a validated event for this exact owner, generation and video.
    pub is_ready: bool,
    /// A renderer holds the playback lease.
    pub has_owner: bool,
    pub current_time: f64,
    pub duration: f64,
    pub is_paused: bool,
    pub is_advertisement: bool,
    pub has_queue: bool,
    /// No claim, release or local preparation is in flight.
    pub is_idle: bool,
}

/// What the system shows, and which of its buttons may work. `playback_state` is 0 stopped,
/// 1 paused, 2 playing. The track's own text is shown only while `is_active`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct GoosicMediaProjection {
    pub is_active: bool,
    pub playback_state: u8,
    pub elapsed_time: f64,
    pub duration: f64,
    pub playback_rate: f64,
    pub play: bool,
    pub pause: bool,
    pub toggle_play_pause: bool,
    pub next: bool,
    pub previous: bool,
    pub change_position: bool,
    pub stop: bool,
    pub change_volume: bool,
}

/// Projects confirmed state onto the system media controls.
///
/// # Safety
///
/// `input` must point to a readable [`GoosicMediaInput`] and `output` to a writable
/// [`GoosicMediaProjection`].
#[no_mangle]
pub unsafe extern "C" fn goosic_media_project(
    input: *const GoosicMediaInput,
    output: *mut GoosicMediaProjection,
) {
    if input.is_null() || output.is_null() {
        return;
    }
    let input = unsafe { *input };
    // Only whether a track is known matters to these rules; its text is the shell's to show.
    let track = input.has_track.then(|| Track {
        id: String::new(),
        title: String::new(),
        subtitle: String::new(),
        artist: String::new(),
        artist_id: None,
        album: String::new(),
        album_id: None,
        duration: String::new(),
        video_id: String::new(),
        explicit: false,
        thumbnail: None,
    });
    let snapshot = MediaSnapshot {
        track,
        current_time: input.current_time,
        duration: input.duration,
        is_paused: input.is_paused,
        owner: if input.has_owner {
            Owner::OfficialWebView
        } else {
            Owner::None
        },
        is_advertisement: input.is_advertisement,
        has_queue: input.has_queue,
        transition: if input.is_idle {
            PlaybackTransition::Idle
        } else {
            PlaybackTransition::Claiming
        },
        volume: 1.0,
        is_muted: false,
        is_ready: input.is_ready,
    };
    let shown = NowPlaying::from_snapshot(&snapshot);
    let allowed = CommandAvailability::from_snapshot(&snapshot);
    unsafe {
        *output = GoosicMediaProjection {
            is_active: shown.is_active,
            playback_state: match shown.playback_state {
                MediaPlaybackState::Stopped => 0,
                MediaPlaybackState::Paused => 1,
                MediaPlaybackState::Playing => 2,
            },
            elapsed_time: shown.elapsed_time,
            duration: shown.duration,
            playback_rate: shown.playback_rate,
            play: allowed.play,
            pause: allowed.pause,
            toggle_play_pause: allowed.toggle_play_pause,
            next: allowed.next,
            previous: allowed.previous,
            change_position: allowed.change_position,
            stop: allowed.stop,
            change_volume: allowed.change_volume,
        };
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;
    use crate::goosic_string_free;

    fn text(raw: *mut c_char) -> String {
        let value = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_owned();
        unsafe { goosic_string_free(raw) };
        value
    }

    #[test]
    fn scripts_and_limits_cross_unchanged() {
        assert_eq!(
            text(goosic_bridge_automix_off_script()),
            bridge::AUTOMIX_OFF_SCRIPT
        );
        assert_eq!(
            text(goosic_bridge_active_media_element_script()),
            bridge::ACTIVE_MEDIA_ELEMENT_SCRIPT
        );
        assert_eq!(
            text(goosic_login_completion_script()),
            login::COMPLETION_SCRIPT
        );
        assert!(text(goosic_bridge_safari_user_agent_suffix()).contains("Safari"));
        assert_eq!(goosic_bridge_automix_attempts(), 40);
        assert_eq!(goosic_login_completion_timeout_seconds(), 180.0);
        assert_eq!(goosic_artwork_max_concurrent_fetches(), 6);
        let host = CString::new("accounts.google.com.ni").unwrap();
        assert!(unsafe { goosic_login_is_google_sign_in_host(host.as_ptr()) });
    }

    #[test]
    fn artwork_and_cover_rules_answer() {
        let allowed = CString::new("https://lh3.googleusercontent.com/a.jpg").unwrap();
        let elsewhere = CString::new("https://example.com/a.jpg").unwrap();
        assert!(unsafe { goosic_artwork_is_allowed(allowed.as_ptr()) });
        assert!(!unsafe { goosic_artwork_is_allowed(elsewhere.as_ptr()) });
        let key = text(unsafe { goosic_artwork_cache_key(allowed.as_ptr()) });
        assert_eq!(
            key,
            artwork::cache_key("https://lh3.googleusercontent.com/a.jpg")
        );
        assert!(unsafe { goosic_artwork_cache_key(std::ptr::null()) }.is_null());

        assert_eq!(goosic_cover_aspect(1280.0, 720.0), 16.0 / 9.0);
        let (mut width, mut height) = (0.0, 0.0);
        unsafe { goosic_cover_fit(16.0 / 9.0, 320.0, 320.0, &mut width, &mut height) };
        assert_eq!((width, height), (320.0, 180.0));
    }

    #[test]
    fn the_hero_takes_its_height_and_tint_from_rust() {
        assert_eq!(goosic_hero_height(100.0), hero::MIN_HEIGHT);
        let white = [255u8; 4 * 4 * 4];
        let mut rgb = [0u8; 3];
        assert!(unsafe { goosic_hero_tint(white.as_ptr(), white.len(), 4, 4, rgb.as_mut_ptr()) });
        assert_eq!(Some(rgb), hero::tint(&white, 4, 4));
        assert!(!unsafe { goosic_hero_tint(white.as_ptr(), 3, 4, 4, rgb.as_mut_ptr()) });
    }

    #[test]
    fn content_left_paused_after_an_advertisement_is_resumed_once() {
        let mut held = GoosicAdRecovery::default();
        let paused = CString::new("paused").unwrap();
        let playing = CString::new("playing").unwrap();
        assert!(!unsafe { goosic_ad_recovery_observe(&mut held, true, playing.as_ptr(), false) });
        assert!(held.awaiting_content);
        assert!(unsafe { goosic_ad_recovery_observe(&mut held, false, paused.as_ptr(), false) });
        assert!(!unsafe { goosic_ad_recovery_observe(&mut held, false, paused.as_ptr(), false) });
        assert!(!unsafe { goosic_ad_recovery_observe(&mut held, false, playing.as_ptr(), false) });
        assert!(!held.awaiting_content);
    }

    #[test]
    fn media_controls_follow_confirmed_state() {
        let input = GoosicMediaInput {
            has_track: true,
            is_ready: true,
            has_owner: true,
            current_time: 500.0,
            duration: f64::NAN,
            is_paused: false,
            is_advertisement: true,
            has_queue: true,
            is_idle: true,
        };
        let mut out = GoosicMediaProjection::default();
        unsafe { goosic_media_project(&input, &mut out) };
        assert!(out.is_active);
        assert_eq!(out.playback_state, 2);
        assert_eq!(out.duration, 0.0);
        assert_eq!(out.elapsed_time, 500.0);
        assert!(out.pause && !out.play);
        assert!(
            !out.next,
            "an advertisement cannot be skipped from the system controls"
        );

        let idle = GoosicMediaInput {
            is_ready: false,
            ..input
        };
        unsafe { goosic_media_project(&idle, &mut out) };
        assert!(!out.is_active && out.playback_state == 0 && !out.toggle_play_pause);
    }
}
