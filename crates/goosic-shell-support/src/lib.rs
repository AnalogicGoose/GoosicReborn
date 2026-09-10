//! The half of a shell that is the same on every platform.
//!
//! A shell owns its window, its renderer and its secure storage, and none of that belongs here.
//! What does belong here is everything a shell decides that has no machine in it: the conversation
//! with `goosic-service`, and the rules the Swift shell used to keep inside its `Core` directory —
//! where a sign-in window may navigate, whether a renderer's report is believable, what the system
//! media controls may offer, which catalog row is playable, what plays next. Three shells writing
//! those separately would produce three subtly different clients of one authority.
//!
//! This crate does not link `goosic-core`, and it takes no UI, WebView, cookie, audio or
//! secure-storage dependency — a shell that needs one of those is holding a platform concern,
//! which stays in the shell. Labels, glyphs and layout are not here either: how a thing is shown
//! is presentation, and presentation stays native.
//!
//! | Module | Moved from | What it decides |
//! | --- | --- | --- |
//! | [`client`], [`framing`], [`response`] | `ServiceClient.swift` | the transport |
//! | [`login`] | `AccountLoginModel.swift` | where sign-in may go, when it is complete |
//! | [`bridge`] | `OfficialBridge.swift` | whether a web player's report is trustworthy |
//! | [`media`] | `SystemMediaPlayback.swift` | what the system media controls show and allow |
//! | [`catalog`] | `Catalog.swift` | which rows are playable and how a page is shaped |
//! | [`playback`] | `Models.swift` | what plays next, clamps, seek settling, preference saves |
//! | [`lyrics`] | `Models.swift` | which line is current |
//! | [`artwork`] | `ArtworkCache.swift` | which hosts artwork may come from, its cache key |
//! | [`navigation`] | `Models.swift`, `Catalog.swift`, `ThemeHost.swift` | route, filter and mode identity |

use std::time::Duration;

pub mod artwork;
pub mod bridge;
pub mod catalog;
pub mod client;
pub mod framing;
pub mod login;
pub mod lyrics;
pub mod media;
pub mod navigation;
pub mod playback;
pub mod response;
mod text;

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

    /// The code and message a shell shows for this failure.
    ///
    /// A refusal carries the service's own code, so a screen can react to `catalogEmpty`
    /// differently from `catalogUnavailable`. Everything else is reported under `transport`, so
    /// the person reading it can tell the authority said no from the channel breaking.
    pub fn describe(&self) -> (String, String) {
        match self {
            Self::Remote { code, message } => (code.clone(), message.clone()),
            other => ("transport".to_owned(), other.to_string()),
        }
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

    #[test]
    fn a_refusal_is_described_by_the_service_and_anything_else_as_transport() {
        let refusal = TransportError::Remote { code: "catalogEmpty".into(), message: "none".into() };
        assert_eq!(refusal.describe(), ("catalogEmpty".to_owned(), "none".to_owned()));
        let (code, message) = TransportError::TimedOut.describe();
        assert_eq!(code, "transport");
        assert_eq!(message, "the service did not respond before the timeout");
    }
}
