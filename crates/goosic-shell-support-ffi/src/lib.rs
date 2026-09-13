//! A C ABI over the platform-neutral rules, for shells that cannot link Rust directly.
//!
//! [`NATIVE_SHELL_MIGRATION.md`] says to expose an FFI contract only when a real use proves it
//! valuable, and the Windows shell is that use: it is written in C#, and the alternative is a
//! third copy of rules that [`SHELL_CONTRACT.md`] already explains cannot be kept in step —
//! "a rule with three copies has three answers as soon as one of them is edited".
//!
//! What crosses the boundary is deliberately narrow. Only the rules a shell cannot reasonably
//! restate are here: the injected page scripts, which are security-sensitive generated
//! JavaScript, and the validators that decide whether a bridge event may be believed. Screens,
//! focus and layout stay native, because moving them here would build the cross-language UI
//! framework the plan rules out.
//!
//! # Memory
//!
//! Every function returning a string returns ownership of a NUL-terminated UTF-8 buffer that the
//! caller must hand back to [`goosic_string_free`]. Returning a borrowed pointer would be
//! smaller, but the caller cannot know how long it stays valid, and a shell that guesses wrong
//! reads freed memory.
//!
//! [`NATIVE_SHELL_MIGRATION.md`]: ../../../docs/NATIVE_SHELL_MIGRATION.md
//! [`SHELL_CONTRACT.md`]: ../../../docs/SHELL_CONTRACT.md

use std::ffi::{c_char, c_ulonglong, CStr, CString};

use goosic_shell_support::bridge;

/// Reads a caller's string, refusing anything that is not valid UTF-8.
///
/// # Safety
///
/// `value` must be NUL-terminated or null.
unsafe fn borrow<'a>(value: *const c_char) -> Option<&'a str> {
    if value.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(value) }.to_str().ok()
}

/// Hands a string to the caller, who owns it until [`goosic_string_free`].
fn hand_over(value: String) -> *mut c_char {
    // A NUL inside the string would truncate it at the boundary, so it is refused rather than
    // silently shortened. None of these values can contain one; this is the guard, not a case.
    match CString::new(value) {
        Ok(owned) => owned.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Frees a string produced by this library. Null is accepted and ignored.
///
/// # Safety
///
/// `value` must have come from this library and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn goosic_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

/// The only host the official playback surface may load.
#[no_mangle]
pub extern "C" fn goosic_bridge_allowed_host() -> *mut c_char {
    hand_over(bridge::ALLOWED_HOST.to_owned())
}

/// The name the page posts bridge messages to.
#[no_mangle]
pub extern "C" fn goosic_bridge_handler_name() -> *mut c_char {
    hand_over(bridge::HANDLER_NAME.to_owned())
}

/// The bridge event version this build speaks.
#[no_mangle]
pub extern "C" fn goosic_bridge_event_version() -> i64 {
    bridge::EVENT_VERSION
}

/// The largest bridge message body that will be read.
#[no_mangle]
pub extern "C" fn goosic_bridge_max_body_bytes() -> usize {
    bridge::MAX_BODY_BYTES
}

/// Whether `video_id` is shaped like a YouTube video id.
///
/// # Safety
///
/// `video_id` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_bridge_is_valid_video_id(video_id: *const c_char) -> bool {
    unsafe { borrow(video_id) }.is_some_and(bridge::is_valid_video_id)
}

/// The per-load page observer, for one lease generation and one requested track.
///
/// # Safety
///
/// `token` and `video_id` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_bridge_observer_script(
    token: *const c_char,
    generation: c_ulonglong,
    video_id: *const c_char,
) -> *mut c_char {
    let (Some(token), Some(video_id)) = (unsafe { borrow(token) }, unsafe { borrow(video_id) })
    else {
        return std::ptr::null_mut();
    };
    hand_over(bridge::observer_script(token, generation, video_id))
}

/// The script that stops the page installing its own media-session handlers.
#[no_mangle]
pub extern "C" fn goosic_bridge_media_session_guard_script() -> *mut c_char {
    hand_over(bridge::MEDIA_SESSION_GUARD_SCRIPT.to_owned())
}

/// Encodes a value as a JavaScript string literal.
///
/// # Safety
///
/// `value` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_bridge_js_string_literal(value: *const c_char) -> *mut c_char {
    match unsafe { borrow(value) } {
        Some(value) => hand_over(bridge::js_string_literal(value)),
        None => std::ptr::null_mut(),
    }
}
