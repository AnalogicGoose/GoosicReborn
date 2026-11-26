//! The shared queue engine and the stateless shell rules, over JSON.
//!
//! The bridge functions above take and return plain values because each is one question with a
//! small answer. The queue is a state machine with a few dozen commands, and the rules beside it
//! keep growing, so a C function per case would be a second, hand-maintained copy of the API in
//! every shell. Instead each crosses as JSON: one command in, one [`Outcome`] out, with the
//! serde shapes in `goosic-shell-support` as the single definition. A call is microseconds, which
//! is nothing beside a click, a track end or a frame.
//!
//! Every answer is an object with either `ok` (the result) or `error` (a sentence). A malformed
//! command is answered rather than ignored, so a shell that sends the wrong shape finds out in its
//! tests instead of losing a click.
//!
//! [`Outcome`]: goosic_shell_support::queue_engine::Outcome

use std::ffi::c_char;
use std::sync::Mutex;

use goosic_shell_support::navigation::RepeatMode;
use goosic_shell_support::player_text;
use goosic_shell_support::queue_engine::{self, Command, LaunchSurface, QueueEngine};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{borrow, hand_over};

/// An engine owned by the caller between [`goosic_queue_new`] and [`goosic_queue_free`].
///
/// The mutex makes a call from the wrong thread wait rather than corrupt the queue. Both shells
/// drive it from their UI thread, so it is never contended.
pub struct GoosicQueue(Mutex<QueueEngine>);

fn answer(result: Result<Value, String>) -> *mut c_char {
    let value = match result {
        Ok(ok) => json!({ "ok": ok }),
        Err(error) => json!({ "error": error }),
    };
    hand_over(value.to_string())
}

/// Creates a queue. Hand it back to [`goosic_queue_free`] exactly once.
#[no_mangle]
pub extern "C" fn goosic_queue_new() -> *mut GoosicQueue {
    Box::into_raw(Box::new(GoosicQueue(Mutex::new(QueueEngine::new()))))
}

/// Frees a queue. Null is accepted and ignored.
///
/// # Safety
///
/// `queue` must have come from [`goosic_queue_new`] and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn goosic_queue_free(queue: *mut GoosicQueue) {
    if !queue.is_null() {
        drop(unsafe { Box::from_raw(queue) });
    }
}

/// Applies one JSON command and answers `{"ok": Outcome}` or `{"error": "..."}`. The answer is
/// owned by the caller until [`crate::goosic_string_free`].
///
/// # Safety
///
/// `queue` must be a live queue from [`goosic_queue_new`]; `command` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn goosic_queue_handle(
    queue: *mut GoosicQueue,
    command: *const c_char,
) -> *mut c_char {
    if queue.is_null() {
        return answer(Err("no queue".to_owned()));
    }
    let Some(command) = (unsafe { borrow(command) }) else {
        return answer(Err("the command is not UTF-8 text".to_owned()));
    };
    let command: Command = match serde_json::from_str(command) {
        Ok(command) => command,
        Err(error) => return answer(Err(format!("unreadable queue command: {error}"))),
    };
    let queue = unsafe { &*queue };
    // A panic inside the engine would poison the lock; the queue it guarded is still the best
    // state there is, so it is recovered rather than lost.
    let mut engine = queue
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let outcome = engine.handle(command);
    answer(serde_json::to_value(outcome).map_err(|error| error.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Surface {
    surface: LaunchSurface,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SongCount {
    count: usize,
    #[serde(default)]
    has_more: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SleepTimer {
    #[serde(default)]
    remaining_seconds: Option<f64>,
    #[serde(default)]
    end_of_song: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpNext {
    #[serde(default)]
    source: Option<String>,
    remaining: usize,
    #[serde(default)]
    finding_more: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    subject: String,
}

#[derive(Deserialize)]
struct Metadata {
    metadata: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repeat {
    repeat: RepeatMode,
}

fn parse<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, String> {
    serde_json::from_str(input).map_err(|error| format!("unreadable arguments: {error}"))
}

/// Answers one stateless rule by name.
pub fn call_rule(name: &str, input: &str) -> Result<Value, String> {
    match name {
        "launchKind" => {
            let Surface { surface } = parse(input)?;
            Ok(json!(queue_engine::launch_kind_for(surface)))
        }
        "songCount" => {
            let SongCount { count, has_more } = parse(input)?;
            Ok(json!(player_text::song_count(count, has_more)))
        }
        "sleepTimer" => {
            let SleepTimer {
                remaining_seconds,
                end_of_song,
            } = parse(input)?;
            Ok(json!(player_text::sleep_timer(
                remaining_seconds,
                end_of_song
            )))
        }
        "upNext" => {
            let UpNext {
                source,
                remaining,
                finding_more,
            } = parse(input)?;
            Ok(json!(player_text::up_next(
                source.as_deref(),
                remaining,
                finding_more
            )))
        }
        "nextRepeat" => {
            let Repeat { repeat } = parse(input)?;
            Ok(json!(repeat.next()))
        }
        "failureText" => {
            let Failure {
                code,
                message,
                subject,
            } = parse(input)?;
            let text = goosic_shell_support::catalog::failure_text(&code, &message, &subject);
            Ok(json!({ "title": text.title, "detail": text.detail }))
        }
        "loginSummary" => {
            let Metadata { metadata } = parse(input)?;
            Ok(json!(goosic_shell_support::login::sanitize_metadata(
                metadata.as_bytes()
            )))
        }
        other => Err(format!("no rule named {other}")),
    }
}

/// Answers a stateless rule: `name` with JSON arguments, as `{"ok": value}` or
/// `{"error": "..."}`. The answer is owned by the caller until [`crate::goosic_string_free`].
///
/// # Safety
///
/// Both pointers must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn goosic_rules_call(
    name: *const c_char,
    input: *const c_char,
) -> *mut c_char {
    let (Some(name), Some(input)) = (unsafe { borrow(name) }, unsafe { borrow(input) }) else {
        return answer(Err("the arguments are not UTF-8 text".to_owned()));
    };
    answer(call_rule(name, input))
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;
    use crate::goosic_string_free;

    fn take(raw: *mut c_char) -> Value {
        assert!(!raw.is_null());
        let text = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_owned();
        unsafe { goosic_string_free(raw) };
        serde_json::from_str(&text).unwrap()
    }

    fn send(queue: *mut GoosicQueue, command: &str) -> Value {
        let command = CString::new(command).unwrap();
        take(unsafe { goosic_queue_handle(queue, command.as_ptr()) })
    }

    #[test]
    fn a_queue_round_trips_through_json() {
        let queue = goosic_queue_new();
        let answer = send(
            queue,
            r#"{"type":"launch","tracks":[{"videoId":"a","title":"A"},{"videoId":"b","title":"B"}],"start":0,"kind":"station"}"#,
        );
        let outcome = &answer["ok"];
        assert_eq!(outcome["snapshot"]["entries"].as_array().unwrap().len(), 1);
        assert_eq!(outcome["snapshot"]["sourceTitle"], "A radio");
        let kinds: Vec<&str> = outcome["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|effect| effect["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["play", "fetchRadio"]);

        let bad = send(queue, r#"{"type":"teleport"}"#);
        assert!(bad["error"]
            .as_str()
            .unwrap()
            .contains("unreadable queue command"));
        unsafe { goosic_queue_free(queue) };
        unsafe { goosic_queue_free(std::ptr::null_mut()) };
    }

    #[test]
    fn rules_answer_by_name() {
        let call = |name: &str, input: &str| {
            let name = CString::new(name).unwrap();
            let input = CString::new(input).unwrap();
            take(unsafe { goosic_rules_call(name.as_ptr(), input.as_ptr()) })
        };
        assert_eq!(
            call("launchKind", r#"{"surface":"search"}"#)["ok"],
            "station"
        );
        assert_eq!(
            call("launchKind", r#"{"surface":"album"}"#)["ok"],
            "ordered"
        );
        assert_eq!(
            call("songCount", r#"{"count":100,"hasMore":true}"#)["ok"],
            "100+ songs"
        );
        assert_eq!(
            call("sleepTimer", r#"{"remainingSeconds":61}"#)["ok"],
            "Sleep timer: 2 min left"
        );
        assert_eq!(
            call(
                "upNext",
                r#"{"source":"Songs","remaining":0,"findingMore":true}"#
            )["ok"],
            "From Songs · Finding songs like this…"
        );
        assert_eq!(call("nextRepeat", r#"{"repeat":"one"}"#)["ok"], "off");
        assert_eq!(
            call(
                "failureText",
                r#"{"code":"catalogEmpty","message":"","subject":"Search"}"#
            )["ok"]["title"],
            "No results"
        );
        assert_eq!(
            call(
                "loginSummary",
                r#"{"metadata":"{\"displayName\":\" Ada \",\"email\":\"ada@example.com\"}"}"#
            )["ok"]["displayName"],
            "Ada"
        );
        assert!(call("nope", "{}")["error"].as_str().is_some());
    }
}
