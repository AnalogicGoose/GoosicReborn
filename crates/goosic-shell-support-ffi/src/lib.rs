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

/// The verdict on one bridge message, as the shell receives it.
#[derive(serde::Serialize)]
struct Verdict<'a> {
    accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<&'a bridge::BridgeEvent>,
}

/// Decides whether one message from the page may be believed.
///
/// The whole decision crosses in a single call, returning JSON, because the alternative is the
/// shell marshalling a struct field by field and then applying the checks itself -- which is the
/// third copy of the rules this boundary exists to avoid. An empty `expected_token` or
/// `expected_video_id` means "nothing is loaded", which every check treats as a mismatch.
///
/// # Safety
///
/// Every pointer must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_bridge_validate_event(
    body: *const c_char,
    expected_token: *const c_char,
    expected_generation: c_ulonglong,
    expected_video_id: *const c_char,
    last_sequence: c_ulonglong,
) -> *mut c_char {
    let Some(body) = (unsafe { borrow(body) }) else {
        return hand_over(
            serde_json::to_string(&Verdict {
                accepted: false,
                reason: Some("the message body was not valid UTF-8".to_owned()),
                event: None,
            })
            .unwrap_or_default(),
        );
    };

    let Some(event) = bridge::parse_event(body.as_bytes()) else {
        return hand_over(
            serde_json::to_string(&Verdict {
                accepted: false,
                reason: Some("the message was not a bridge event".to_owned()),
                event: None,
            })
            .unwrap_or_default(),
        );
    };

    let token = unsafe { borrow(expected_token) }.filter(|value| !value.is_empty());
    let video_id = unsafe { borrow(expected_video_id) }.filter(|value| !value.is_empty());
    let reason = bridge::rejection_reason(
        &event,
        token,
        Some(expected_generation),
        video_id,
        last_sequence,
    );

    let verdict = match &reason {
        Some(reason) => Verdict {
            accepted: false,
            reason: Some(reason.clone()),
            event: None,
        },
        None => Verdict {
            accepted: true,
            reason: None,
            event: Some(&event),
        },
    };
    hand_over(serde_json::to_string(&verdict).unwrap_or_default())
}

/// Whether a sign-in window may navigate its main frame to `url`.
///
/// # Safety
///
/// `url` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_login_is_allowed_url(url: *const c_char) -> bool {
    unsafe { borrow(url) }.is_some_and(goosic_shell_support::login::is_allowed_login_url)
}

/// Whether a page is on the one origin a completed sign-in lands on.
///
/// # Safety
///
/// `url` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_login_is_completion_origin(url: *const c_char) -> bool {
    unsafe { borrow(url) }.is_some_and(goosic_shell_support::login::is_exact_completion_origin)
}

/// The cleaned account summary of a finished sign-in, as JSON, or null when the report is not
/// enough to call the sign-in complete.
///
/// Every condition is Rust's: distinct canonical identities, the exact completion origin, bounded
/// metadata, and a second account-specific field. The shell only forwards what the page said.
///
/// # Safety
///
/// Every pointer must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn goosic_login_make_result(
    account_id: *const c_char,
    profile_id: *const c_char,
    metadata: *const c_char,
    page_url: *const c_char,
) -> *mut c_char {
    use goosic_shell_support::login;
    let (Some(account_id), Some(profile_id), Some(metadata), Some(page_url)) = (
        unsafe { borrow(account_id) },
        unsafe { borrow(profile_id) },
        unsafe { borrow(metadata) },
        unsafe { borrow(page_url) },
    ) else {
        return std::ptr::null_mut();
    };
    if !login::are_distinct_profiles(account_id, profile_id) {
        return std::ptr::null_mut();
    }
    let (Ok(account), Ok(profile)) = (
        uuid::Uuid::parse_str(account_id),
        uuid::Uuid::parse_str(profile_id),
    ) else {
        return std::ptr::null_mut();
    };
    match login::make_result(account, profile, metadata.as_bytes(), page_url) {
        Some(result) => hand_over(serde_json::to_string(&result.summary).unwrap_or_default()),
        None => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod login_tests {
    use super::*;

    fn owned(value: &str) -> CString {
        CString::new(value).unwrap()
    }

    #[test]
    fn login_navigation_follows_the_shared_rule() {
        let google = owned("https://accounts.google.com/ServiceLogin");
        let elsewhere = owned("https://example.com/");
        unsafe {
            assert!(goosic_login_is_allowed_url(google.as_ptr()));
            assert!(!goosic_login_is_allowed_url(elsewhere.as_ptr()));
            assert!(!goosic_login_is_allowed_url(std::ptr::null()));
        }
    }

    #[test]
    fn a_finished_sign_in_needs_distinct_identities_and_a_second_field() {
        let account = owned("6f1c1a52-6b7e-4f0e-9d1b-0a4f4bb1e001");
        let profile = owned("6f1c1a52-6b7e-4f0e-9d1b-0a4f4bb1e002");
        let page = owned("https://music.youtube.com/");
        let good = owned(
            r#"{"displayName":"Ada","email":"ada@example.com","channel":null,"avatarUrl":null}"#,
        );
        let weak = owned(r#"{"displayName":"Ada","email":null,"channel":null,"avatarUrl":null}"#);
        unsafe {
            let result = goosic_login_make_result(
                account.as_ptr(),
                profile.as_ptr(),
                good.as_ptr(),
                page.as_ptr(),
            );
            assert!(!result.is_null());
            let text = CStr::from_ptr(result).to_str().unwrap().to_owned();
            goosic_string_free(result);
            assert!(text.contains("ada@example.com"));
            assert!(goosic_login_make_result(
                account.as_ptr(),
                profile.as_ptr(),
                weak.as_ptr(),
                page.as_ptr()
            )
            .is_null());
            assert!(goosic_login_make_result(
                account.as_ptr(),
                account.as_ptr(),
                good.as_ptr(),
                page.as_ptr()
            )
            .is_null());
        }
    }
}
