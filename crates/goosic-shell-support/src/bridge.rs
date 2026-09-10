//! The half of the official playback bridge that is not tied to a WebKit.
//!
//! macOS drives a `WKWebView`, Linux a WebKitGTK `WebKitWebView`, and Windows will drive WebView2,
//! but what counts as a trustworthy event must not depend on which one is running. The wire shape,
//! the page observer, and the rules that reject an event live here so every host decides
//! identically; a second copy of [`rejection_reason`] would be a second answer to "may this play".
//!
//! The scripts are data the rules travel with, not code a shell runs on its own terms. They are
//! the versions on `development`; the macOS branch has since taught the observer to follow the
//! media element that is actually playing, and this copy is updated when that branch lands.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The only origin this bridge speaks to.
pub const ALLOWED_HOST: &str = "music.youtube.com";

/// The script message handler's name, on every platform.
pub const HANDLER_NAME: &str = "goosicBridge";

/// A bridge message larger than this is refused before it is parsed.
pub const MAX_BODY_BYTES: usize = 16 * 1024;

/// YouTube Music refuses to run its player under a bare engine user agent and shows "not optimized
/// for your browser" instead. Naming a Safari version makes the agent a complete Safari string,
/// which is what these engines actually are.
pub const SAFARI_USER_AGENT_SUFFIX: &str = "Version/18.5 Safari/605.1.15";

/// The stable identity of guest web storage. Account profiles supply their own; cookies are never
/// exported from any of them.
pub const GUEST_PROFILE_ID: Uuid = Uuid::from_u128(0x8E4CA2CD_373A_46E3_A5B0_9A2A7B3B5084);

/// The event version this build understands.
pub const EVENT_VERSION: i64 = 2;

/// The wire shape a page observer posts across the bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeEvent {
    pub version: i64,
    pub token: String,
    pub generation: u64,
    /// The observer writes `videoId`; the mapping is part of the contract.
    pub video_id: String,
    pub sequence: u64,
    pub state: String,
    pub current_time: f64,
    pub duration: f64,
    pub is_advertisement: bool,
    pub volume: f64,
    pub muted: bool,
}

/// Decodes a message body, refusing an oversized one before it is parsed.
pub fn parse_event(body: &[u8]) -> Option<BridgeEvent> {
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    serde_json::from_slice(body).ok()
}

/// Why an event is not trustworthy, or `None` when it is.
///
/// Each check answers a different way a report can be wrong: an observer from an older build, a
/// document the host has already replaced, a lease that has moved on, a page that navigated to a
/// track nobody asked for, a replayed or reordered message, and numbers no media element can
/// produce.
pub fn rejection_reason(
    event: &BridgeEvent,
    expected_token: Option<&str>,
    expected_generation: Option<u64>,
    expected_video_id: Option<&str>,
    last_sequence: u64,
) -> Option<String> {
    if event.version != EVENT_VERSION {
        return Some(format!("unsupported bridge version {}", event.version));
    }
    if Some(event.token.as_str()) != expected_token {
        return Some("it came from a superseded document".to_owned());
    }
    if Some(event.generation) != expected_generation {
        return Some(format!("generation {} is not the active lease", event.generation));
    }
    if Some(event.video_id.as_str()) != expected_video_id {
        let described = if event.video_id.is_empty() { "no video" } else { &event.video_id };
        return Some(format!("it describes {described}, not the requested video"));
    }
    if event.sequence <= last_sequence {
        return Some(format!(
            "sequence {} did not advance past {last_sequence}",
            event.sequence
        ));
    }
    if !event.current_time.is_finite()
        || event.current_time < 0.0
        || !event.duration.is_finite()
        || event.duration < 0.0
    {
        return Some("it reported an impossible position or duration".to_owned());
    }
    if !event.volume.is_finite() || !(0.0..=1.0).contains(&event.volume) {
        return Some("it reported an impossible volume".to_owned());
    }
    None
}

/// Whether `video_id` is shaped like a YouTube video id: eleven characters of URL-safe base64.
///
/// Stricter than the Swift version on purpose. That one used Unicode `isLetter` and `isNumber`, so
/// it would accept eleven accented letters; a video id is ASCII by construction, and a guard that
/// sits in front of a URL the host builds should not be more generous than the thing it guards.
pub fn is_valid_video_id(video_id: &str) -> bool {
    video_id.len() == 11
        && video_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Encodes a string as a JavaScript string literal, so an injected value can never terminate the
/// literal or inject code.
pub fn js_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The per-load page observer.
///
/// Identity is injected rather than read from the URL: the official app rewrites its own location
/// and drops query items it does not recognise. The video id is still read live, so that a
/// client-side navigation to a different track is reported honestly and rejected.
pub fn observer_script(token: &str, generation: u64, video_id: &str) -> String {
    format!(
        "(() => {{\n  const token = {};\n  const generation = {generation};\n  const requestedVideoId = {};\n{OBSERVER_BODY}",
        js_string_literal(token),
        js_string_literal(video_id),
    )
}

const OBSERVER_BODY: &str = r#"  let sequence = 0;
  let media;
  const currentVideoId = () =>
    new URLSearchParams(window.location.search).get('v') || requestedVideoId;
  const isAd = () => Boolean(document.querySelector(
    '.ad-showing, .ytp-ad-player-overlay, .ytp-ad-text, [class*=ad-showing]'
  ));
  const send = () => {
    media = document.querySelector('audio,video');
    if (!media || !window.webkit?.messageHandlers?.goosicBridge) return;
    const actualVideoId = currentVideoId();
    const advertisement = isAd();
    if (actualVideoId !== requestedVideoId && !advertisement && !media.paused) {
      // Stop the app's own "up next" immediately rather than letting an unrequested
      // content track play while the native side is still being told about it. A
      // pre-roll advertisement is allowed to finish before queue takeover.
      try { media.pause(); } catch (error) { /* the native side is told regardless */ }
    }
    const currentTime =
      Number.isFinite(media.currentTime) && media.currentTime >= 0 ? media.currentTime : 0;
    const duration =
      Number.isFinite(media.duration) && media.duration >= 0 ? media.duration : 0;
    const state = media.ended ? 'ended' : media.paused ? 'paused' : 'playing';
    const volume = Number.isFinite(media.volume) ? Math.min(Math.max(media.volume, 0), 1) : 1;
    window.webkit.messageHandlers.goosicBridge.postMessage({
      version: 2, token, generation,
      // Ads belong to the active official load even if the page has already changed its
      // content route. Defer exposing a mismatched id until non-ad content appears.
      videoId: advertisement ? requestedVideoId : actualVideoId,
      sequence: ++sequence, state, currentTime, duration,
      isAdvertisement: advertisement, volume, muted: Boolean(media.muted)
    });
  };
  const install = () => {
    const next = document.querySelector('audio,video');
    if (next === media) return;
    media = next;
    if (!media) return;
    ['play','pause','ended','timeupdate','durationchange','loadedmetadata','volumechange','seeked']
      .forEach(name => media.addEventListener(name, send, { passive: true }));
    send();
  };
  install();
  new MutationObserver(install).observe(document.documentElement, {
    childList: true, subtree: true
  });
  window.setInterval(send, 500);
})();
"#;

/// Clears the page's Media Session metadata and action handlers so the operating system has one
/// owner of its media controls: the shell's adapter. The script carries no Goosic state, URLs, or
/// credentials.
pub const MEDIA_SESSION_GUARD_SCRIPT: &str = r#"(() => {
  const descriptor = (object, name) => {
    for (let current = object; current; current = Object.getPrototypeOf(current)) {
      const found = Object.getOwnPropertyDescriptor(current, name);
      if (found) return found;
    }
    return null;
  };
  const clear = () => {
    try {
      const session = navigator.mediaSession;
      if (!session) return;
      // Prefer a write-time guard. Some WebKit builds expose these members only on the
      // prototype, so every operation remains best-effort and the periodic clear below is
      // retained as a fallback.
      if (!session.__goosicMediaSessionGuard) {
        const metadata = descriptor(session, 'metadata');
        if (metadata?.set) {
          try { Object.defineProperty(session, 'metadata', {
            configurable: false, enumerable: metadata.enumerable,
            get: () => null, set: () => { try { metadata.set.call(session, null); } catch (_) {} }
          }); } catch (_) {}
        }
        const playbackState = descriptor(session, 'playbackState');
        if (playbackState?.set) {
          try { Object.defineProperty(session, 'playbackState', {
            configurable: false, enumerable: playbackState.enumerable,
            get: () => 'none', set: () => { try { playbackState.set.call(session, 'none'); } catch (_) {} }
          }); } catch (_) {}
        }
        const originalActionHandler = session.setActionHandler;
        if (typeof originalActionHandler === 'function') {
          try { Object.defineProperty(session, 'setActionHandler', {
            configurable: false, writable: false,
            value: (action, handler) => originalActionHandler.call(session, action, null)
          }); } catch (_) {}
        }
        try { Object.defineProperty(session, '__goosicMediaSessionGuard', { value: true }); } catch (_) {}
      }
      session.metadata = null;
      try { session.playbackState = 'none'; } catch (_) {}
      ['play', 'pause', 'seekbackward', 'seekforward', 'previoustrack', 'nexttrack', 'stop']
        .forEach(action => { try { session.setActionHandler(action, null); } catch (_) {} });
    } catch (_) {}
  };
  clear();
  window.setInterval(clear, 750);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Exactly the shape `observer_script` posts.
    fn payload(token: &str, generation: u64, sequence: u64) -> String {
        format!(
            r#"{{"version":2,"token":"{token}","generation":{generation},"videoId":"dQw4w9WgXcQ",
               "sequence":{sequence},"state":"playing","currentTime":12.5,"duration":214,
               "isAdvertisement":false,"volume":0.8,"muted":false}}"#
        )
    }

    fn decoded(body: &str) -> BridgeEvent {
        parse_event(body.as_bytes()).expect("the observer payload must decode")
    }

    fn reason(event: &BridgeEvent, last_sequence: u64) -> Option<String> {
        rejection_reason(event, Some("token"), Some(7), Some("dQw4w9WgXcQ"), last_sequence)
    }

    #[test]
    fn the_observer_payload_decodes_into_the_event_the_hosts_expect() {
        let event = decoded(&payload("token", 7, 1));
        assert_eq!(event.version, 2);
        // The JavaScript writes `videoId`; the mapping is the contract.
        assert_eq!(event.video_id, "dQw4w9WgXcQ");
        assert_eq!(event.generation, 7);
        assert_eq!(event.current_time, 12.5);
        assert!(!event.muted);
    }

    #[test]
    fn an_oversized_body_is_refused_before_it_is_parsed() {
        let padded = format!("{}{}", payload("token", 7, 1), " ".repeat(MAX_BODY_BYTES));
        assert_eq!(parse_event(padded.as_bytes()), None);
    }

    #[test]
    fn an_event_matching_the_active_load_is_accepted() {
        assert_eq!(reason(&decoded(&payload("token", 7, 1)), 0), None);
    }

    #[test]
    fn an_event_from_a_superseded_document_is_refused() {
        assert_eq!(
            reason(&decoded(&payload("stale", 7, 1)), 0).as_deref(),
            Some("it came from a superseded document")
        );
    }

    #[test]
    fn an_event_carrying_another_lease_is_refused() {
        assert_eq!(
            reason(&decoded(&payload("token", 6, 1)), 0).as_deref(),
            Some("generation 6 is not the active lease")
        );
    }

    #[test]
    fn a_sequence_that_did_not_advance_is_refused() {
        assert_eq!(
            reason(&decoded(&payload("token", 7, 4)), 4).as_deref(),
            Some("sequence 4 did not advance past 4")
        );
    }

    #[test]
    fn impossible_numbers_and_foreign_versions_are_refused() {
        let good = decoded(&payload("token", 7, 1));
        let refused = |event: BridgeEvent| reason(&event, 0);
        assert!(refused(BridgeEvent { version: 1, ..good.clone() }).unwrap().contains("version 1"));
        assert!(refused(BridgeEvent { volume: 1.5, ..good.clone() }).unwrap().contains("volume"));
        assert!(refused(BridgeEvent { current_time: -1.0, ..good.clone() }).unwrap().contains("position"));
        assert!(refused(BridgeEvent { video_id: String::new(), ..good.clone() })
            .unwrap()
            .contains("no video"));
        assert!(
            rejection_reason(&good, None, Some(7), Some("dQw4w9WgXcQ"), 0).is_some(),
            "no active load means nothing is expected"
        );
    }

    #[test]
    fn video_ids_are_eleven_url_safe_characters() {
        assert!(is_valid_video_id("dQw4w9WgXcQ"));
        assert!(is_valid_video_id("qXI87eMP-bs"));
        assert!(is_valid_video_id("abc_def-GHI"));
        assert!(!is_valid_video_id("dQw4w9WgXc"), "too short");
        assert!(!is_valid_video_id("dQw4w9WgXcQQ"), "too long");
        assert!(!is_valid_video_id("dQw4w9WgX/Q"), "a path separator");
        assert!(!is_valid_video_id("dQw4w9WgX&Q"), "a query separator");
        assert!(!is_valid_video_id("ééééééééééé"), "not ASCII");
    }

    /// The observer only ever posts to a handler it can see, and on Linux that handler lives in a
    /// script world the page cannot reach. Both sides have to agree on the name for the isolation
    /// to mean anything.
    #[test]
    fn the_observer_posts_to_the_handler_name_every_host_registers() {
        let script = observer_script("t", 1, "v");
        assert!(script.contains(&format!("window.webkit.messageHandlers.{HANDLER_NAME}.postMessage")));
    }

    #[test]
    fn the_observer_carries_the_identity_of_its_own_load() {
        let script = observer_script("abc", 42, "xyz");
        assert!(script.contains("const generation = 42;"));
        assert!(script.contains("\"abc\""));
        assert!(script.contains("\"xyz\""));
    }

    #[test]
    fn an_injected_value_cannot_escape_its_literal() {
        let script = observer_script("\"; alert(1); \"", 1, "v");
        assert!(script.contains(r#"const token = "\"; alert(1); \"";"#));
    }

    #[test]
    fn the_media_session_guard_clears_metadata_and_handlers() {
        let script = MEDIA_SESSION_GUARD_SCRIPT;
        assert!(script.contains("session.metadata = null"));
        assert!(script.contains("session.playbackState = 'none'"));
        assert!(script.contains("Object.defineProperty(session, 'setActionHandler'"));
        assert!(script.contains("setActionHandler(action, null)"));
    }

    #[test]
    fn the_guest_profile_identity_is_the_one_the_hosts_already_use() {
        assert_eq!(
            GUEST_PROFILE_ID.hyphenated().to_string().to_uppercase(),
            "8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084"
        );
    }
}
