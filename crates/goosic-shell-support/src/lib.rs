//! The half of a shell that is the same on every platform.
//!
//! A shell owns its window, its renderer and its secure storage, and none of that belongs here.
//! What does belong here is the conversation with `goosic-service`: how a frame is found in a
//! byte stream, how long a command is allowed to take, which answers are the service disagreeing
//! with you and which mean the channel can no longer be trusted. That is identical for macOS,
//! Linux and Windows, and writing it three times would produce three subtly different clients of
//! one authority.
//!
//! This crate talks to the service. It does not link `goosic-core`, and it takes no UI, WebView,
//! cookie, audio or secure-storage dependency — a shell that needs one of those is holding a
//! platform concern, which stays in the shell.

use std::time::Duration;

pub mod framing;
pub mod response;

pub use framing::FrameReader;
pub use response::accept_response;

/// The largest frame a shell will assemble before giving up on the stream.
///
/// Catalog pages are the only large responses and the service clamps a page well below this, so
/// anything past it is a stream that has lost its framing rather than a big answer.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;

/// How long a command may take before the shell stops waiting.
///
/// Three tiers, and each is a fact about the work rather than a preference. A catalog command
/// reaches a third-party service, so its wait has to cover that service's own upstream timeout
/// instead of tearing down the child mid-request. Preparing a download may decode a whole
/// WebM/Opus file into the local WAV cache on first play, which is off the UI thread but slow on
/// a long track; later plays reuse the cache and return at once. Everything else is local work.
pub fn timeout_for(command: &str) -> Duration {
    if command == "downloads.prepare" {
        Duration::from_secs(120)
    } else if command.starts_with("catalog.") {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(5)
    }
}

/// What can go wrong between asking the service something and believing its answer.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransportError {
    #[error("the service returned an invalid protocol response")]
    InvalidResponse,
    #[error("protocol version mismatch (expected {expected}, got {actual})")]
    ProtocolVersionMismatch { expected: String, actual: String },
    #[error("response request id mismatch (expected {expected}, got {actual})")]
    RequestIdMismatch { expected: String, actual: String },
    #[error("{code}: {message}")]
    Remote { code: String, message: String },
    #[error("the service did not respond before the timeout")]
    TimedOut,
    #[error("goosic-service closed its output")]
    EndOfFile,
    #[error("the service response exceeded the frame limit")]
    ResponseTooLarge,
}

impl TransportError {
    /// Whether this failure means the channel itself can no longer be trusted.
    ///
    /// The distinction is the whole reason this is a method and not a guess at each call site. A
    /// remote error is the authority answering — refusing a stale generation is a *correct*
    /// response and the connection is fine. Everything else means the stream is out of step:
    /// after a frame that could not be parsed, or one carrying somebody else's request id, there
    /// is no way to tell where the next frame begins, and a client that keeps reading will pair
    /// the following answer with the wrong question.
    pub fn invalidates_connection(&self) -> bool {
        !matches!(self, Self::Remote { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_get_the_wait_their_work_needs() {
        assert_eq!(timeout_for("playback.claim"), Duration::from_secs(5));
        assert_eq!(timeout_for("catalog.home"), Duration::from_secs(20));
        assert_eq!(timeout_for("catalog.search"), Duration::from_secs(20));
        assert_eq!(timeout_for("downloads.prepare"), Duration::from_secs(120));
        // Neighbouring commands must not inherit the long decode wait by prefix.
        assert_eq!(timeout_for("downloads.list"), Duration::from_secs(5));
    }

    #[test]
    fn only_a_remote_error_leaves_the_channel_usable() {
        let remote = TransportError::Remote {
            code: "generationMismatch".into(),
            message: "stale lease".into(),
        };
        assert!(!remote.invalidates_connection());
        for error in [
            TransportError::InvalidResponse,
            TransportError::TimedOut,
            TransportError::EndOfFile,
            TransportError::ResponseTooLarge,
            TransportError::RequestIdMismatch { expected: "a".into(), actual: "b".into() },
            TransportError::ProtocolVersionMismatch {
                expected: "0.3.0".into(),
                actual: "0.4.0".into(),
            },
        ] {
            assert!(error.invalidates_connection(), "{error} should end the channel");
        }
    }
}
