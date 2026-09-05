import Foundation

/// A validated event emitted by the one official YouTube Music renderer.
struct OfficialPlaybackEvent {
    let generation: UInt64
    let videoID: String
    let sequence: UInt64
    let state: String
    let currentTime: Double
    let duration: Double
    let isAdvertisement: Bool
    let volume: Double
    let isMuted: Bool
}

struct OfficialPlaybackProfile: Equatable {
    /// Guest storage is intentionally stable. Account profiles will supply their own stable UUID
    /// once account isolation is connected; cookies are never exported from this store.
    let identifier: UUID

    static let guest = OfficialPlaybackProfile(
        identifier: UUID(uuidString: "8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084")!
    )
}

/// The half of the official playback bridge that is not tied to a WebKit implementation.
///
/// macOS drives a `WKWebView` and Linux a WebKitGTK `WebKitWebView`, but what counts as a
/// trustworthy event must not depend on which one is running. The wire shape, the page observer,
/// and the rules that reject an event live here so both hosts decide identically; a second copy
/// of `rejectionReason` would be a second answer to "may this play".
enum OfficialBridge {
    /// The only origin this bridge speaks to.
    static let allowedHost = "music.youtube.com"
    /// The script message handler's name, on both platforms.
    static let name = "goosicBridge"
    /// A bridge message larger than this is refused before it is parsed.
    static let maxBodyBytes = 16 * 1024
    /// YouTube Music refuses to run its player under a bare engine user agent and shows
    /// "not optimized for your browser" instead. Naming a Safari version makes the agent a
    /// complete Safari string, which is what these engines actually are.
    static let safariUserAgentSuffix = "Version/18.5 Safari/605.1.15"

    /// The wire shape a page observer posts across the bridge.
    struct Event: Codable {
        let version: Int
        let token: String
        let generation: UInt64
        let videoID: String
        let sequence: UInt64
        let state: String
        let currentTime: Double
        let duration: Double
        let isAdvertisement: Bool
        let volume: Double
        let muted: Bool

        enum CodingKeys: String, CodingKey {
            case version, token, generation, sequence, state, currentTime, duration, isAdvertisement
            case volume, muted
            case videoID = "videoId"
        }
    }

    /// Why an event is not trustworthy, or `nil` when it is.
    static func rejectionReason(
        for event: Event,
        expectedToken: String?,
        expectedGeneration: UInt64?,
        expectedVideoID: String?,
        lastSequence: UInt64
    ) -> String? {
        guard event.version == 2 else {
            return "unsupported bridge version \(event.version)"
        }
        guard event.token == expectedToken else {
            return "it came from a superseded document"
        }
        guard event.generation == expectedGeneration else {
            return "generation \(event.generation) is not the active lease"
        }
        guard event.videoID == expectedVideoID else {
            return "it describes \(event.videoID.isEmpty ? "no video" : event.videoID), not the requested video"
        }
        guard event.sequence > lastSequence else {
            return "sequence \(event.sequence) did not advance past \(lastSequence)"
        }
        guard event.currentTime.isFinite, event.currentTime >= 0,
              event.duration.isFinite, event.duration >= 0 else {
            return "it reported an impossible position or duration"
        }
        guard event.volume.isFinite, (0...1).contains(event.volume) else {
            return "it reported an impossible volume"
        }
        return nil
    }

    static func isValidVideoID(_ videoID: String) -> Bool {
        guard videoID.count == 11 else { return false }
        return videoID.allSatisfy { $0.isNumber || $0.isLetter || $0 == "-" || $0 == "_" }
    }

    /// The per-load page observer.
    ///
    /// Identity is injected rather than read from the URL: the official app rewrites its own
    /// location and drops query items it does not recognize. The video id is still read live so
    /// that a client-side navigation to a different track is reported honestly and rejected.
    static func observerScript(token: String, generation: UInt64, videoID: String) -> String {
        let encodedToken = jsonStringLiteral(token)
        let encodedVideoID = jsonStringLiteral(videoID)
        return """
        (() => {
          const token = \(encodedToken);
          const generation = \(generation);
          const requestedVideoId = \(encodedVideoID);
          let sequence = 0;
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
        """
    }

    /// Clears the page's Media Session metadata and action handlers so macOS has one owner:
    /// `SystemMediaControls`. The script contains no Goosic state, URLs, or credentials.
    static let mediaSessionGuardScript = """
    (() => {
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
    """

    /// Encodes a Swift string as a JavaScript string literal, so an injected value can never
    /// terminate the literal or inject code.
    private static func jsonStringLiteral(_ value: String) -> String {
        guard let data = try? JSONEncoder().encode(value), let literal = String(data: data, encoding: .utf8) else {
            return "\"\""
        }
        return literal
    }
}
