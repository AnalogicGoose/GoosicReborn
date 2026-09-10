//! The half of a shell that is the same on every platform.
//!
//! A shell owns its window, its renderer and its secure storage, and none of that belongs here.
//! What does belong here is the conversation with `goosic-service`: how a frame is found in a
//! byte stream, how an answer finds the question it belongs to, how long a command is allowed to
//! take, and which failures are the service disagreeing with you rather than the channel breaking.
//! That is identical for macOS, Linux and Windows, and writing it three times would produce three
//! subtly different clients of one authority.
//!
//! This crate talks to the service. It does not link `goosic-core`, and it takes no UI, WebView,
//! cookie, audio or secure-storage dependency — a shell that needs one of those is holding a
//! platform concern, which stays in the shell.

use std::time::Duration;

pub mod client;
pub mod framing;
pub mod response;

pub use client::{ServiceClient, ServiceClientBuilder};
pub use framing::FrameReader;
pub use response::{decode_response, into_result};

/// The largest frame a shell will assemble before giving up on the stream.
///
/// Catalog pages are the only large responses and the service clamps a page well below this, so
/// anything past it is a stream that has lost its framing rather than a big answer.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;

/// How long a command may take before the shell stops waiting for it.
///
/// Three tiers, and each is a fact about the work rather than a preference. Catalog and lyrics
/// reads reach a third-party service, so their wait has to cover that service's own upstream
/// timeout instead of failing a request that is still coming. Preparing a download may decode a
/// whole WebM/Opus file into the local WAV cache on first play, which is off the UI thread but
/// slow on a long track; later plays reuse the cache and return at once. Everything else is local
/// work.
pub fn timeout_for(command: &str) -> Duration {
    if command == "downloads.prepare" {
        Duration::from_secs(120)
    } else if command.starts_with("catalog.") || command.starts_with("lyrics.") {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(5)
    }
}

/// What can go wrong between asking the service something and believing its answer.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum TransportError {
    #[error("goosic-service is unavailable: {0}")]
    Unavailable(String),
    #[error("the service returned an invalid protocol response")]
    InvalidResponse,
    #[error("protocol version mismatch (expected {expected}, got {actual})")]
    ProtocolVersionMismatch { expected: String, actual: String },
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
    /// The distinction is the whole reason this is a method and not a guess at each call site.
    /// Two failures leave the connection usable. A remote error is the authority answering —
    /// refusing a stale generation is a *correct* response. A timeout is one request giving up:
    /// the service answers requests out of order, so a slow catalog read says nothing about the
    /// play command queued behind it, and tearing the service down for it is how one unanswered
    /// read used to stop playback.
    ///
    /// Everything else means the stream is out of step or gone. After a frame that could not be
    /// parsed, or one speaking another protocol version, there is no way to know that the next
    /// bytes are a frame at all.
    pub fn invalidates_connection(&self) -> bool {
        !matches!(self, Self::Remote { .. } | Self::TimedOut)
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
        assert_eq!(timeout_for("lyrics.get"), Duration::from_secs(20));
        assert_eq!(timeout_for("downloads.prepare"), Duration::from_secs(120));
        // Neighbouring commands must not inherit the long decode wait by prefix.
        assert_eq!(timeout_for("downloads.list"), Duration::from_secs(5));
    }

    #[test]
    fn only_a_refusal_or_a_timeout_leaves_the_channel_usable() {
        let remote = TransportError::Remote {
            code: "generationMismatch".into(),
            message: "stale lease".into(),
        };
        assert!(!remote.invalidates_connection());
        assert!(!TransportError::TimedOut.invalidates_connection());
        for error in [
            TransportError::Unavailable("gone".into()),
            TransportError::InvalidResponse,
            TransportError::EndOfFile,
            TransportError::ResponseTooLarge,
            TransportError::ProtocolVersionMismatch {
                expected: "0.3.0".into(),
                actual: "0.4.0".into(),
            },
        ] {
            assert!(error.invalidates_connection(), "{error} should end the channel");
        }
    }
}
