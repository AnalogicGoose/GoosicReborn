#if os(macOS)
import AppKit
import AppKitBackend
import Foundation
import SwiftCrossUI
import WebKit

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

/// Stable mount point for the renderer. Rebinding replaces its one child in place, so SwiftUI
/// layout churn cannot create a second media owner while an account profile changes.
@MainActor
final class OfficialPlaybackContainer: NSView {
    override func layout() {
        super.layout()
        subviews.first?.frame = bounds
    }
}

/// Owns the single WKWebView and the security boundary around its bridge.
@MainActor
final class OfficialPlaybackHost: NSObject {
    private static let maxBridgeBodyBytes = 16 * 1024
    nonisolated private static let allowedHost = "music.youtube.com"
    private static let bridgeName = "goosicBridge"
    private static let safariUserAgentSuffix = "Version/18.5 Safari/605.1.15"

    private weak var webView: WKWebView?
    private weak var container: OfficialPlaybackContainer?
    private var messageProxy: ScriptMessageProxy?
    private var expectedToken: String?
    private var expectedGeneration: UInt64?
    private var expectedVideoID: String?
    private var lastSequence: UInt64 = 0
    private var advertisementActive = false
    private var activeProfile = OfficialPlaybackProfile.guest
    private(set) var loadedVideoID: String?
    private(set) var isLoading = false
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    /// A plain description of what the official page currently is, for when playback does not
    /// start and the reason is the page rather than the bridge.
    var onDiagnostics: ((String) -> Void)?
    /// The official app followed its own "up next" to a video Goosic did not request. Goosic
    /// owns the queue, so the shell decides what actually plays instead.
    var onPageAdvanced: ((String) -> Void)?

    func makeWebView(profile: OfficialPlaybackProfile = .guest) -> WKWebView {
        if let webView {
            if activeProfile.identifier == profile.identifier {
            // SwiftCrossUI may recreate the representable wrapper during layout. Reparent the
            // existing renderer instead of creating a second media owner or crashing.
            webView.removeFromSuperview()
            container?.addSubview(webView)
            return webView
            }
            destroyRenderer()
        }
        activeProfile = profile

        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = WKWebsiteDataStore(forIdentifier: profile.identifier)
        configuration.preferences.isFraudulentWebsiteWarningEnabled = true
        // YouTube Music refuses to run its player under WKWebView's bare user agent and shows
        // "not optimized for your browser" instead. Naming a Safari version makes the default
        // agent a complete Safari string, which is what this engine actually is.
        configuration.applicationNameForUserAgent = Self.safariUserAgentSuffix
        // The user pressed play in Goosic; that gesture does not cross into the web view, so
        // without this the host's own play request is blocked and no media element ever starts.
        configuration.mediaTypesRequiringUserActionForPlayback = []

        let userContentController = WKUserContentController()
        let proxy = ScriptMessageProxy(host: self)
        userContentController.add(proxy, name: Self.bridgeName)
        // The observer is installed per load, in `load(videoID:generation:)`, because it carries
        // that load's identity.
        configuration.userContentController = userContentController
        // The embedded page must not publish a competing Now Playing session. This runs before
        // the page's own scripts; the short watchdog also clears handlers the app installs later.
        userContentController.addUserScript(WKUserScript(
            source: Self.mediaSessionGuardScript,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))

        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = self
        view.uiDelegate = self
        view.allowsMagnification = false
        webView = view
        messageProxy = proxy
        container?.addSubview(view)
        return view
    }

    func makeContainer(profile: OfficialPlaybackProfile? = nil) -> OfficialPlaybackContainer {
        let profile = profile ?? activeProfile
        if let container {
            self.container = container
            _ = makeWebView(profile: profile)
            return container
        }
        let container = OfficialPlaybackContainer(frame: .zero)
        self.container = container
        _ = makeWebView(profile: profile)
        return container
    }

    /// Switches the WebKit data store only after the Rust lease has been released. The old
    /// renderer is fully detached before the new one is created.
    func bind(profile: OfficialPlaybackProfile) {
        guard profile.identifier != activeProfile.identifier || webView == nil else { return }
        destroyRenderer()
        activeProfile = profile
        if container != nil { _ = makeWebView(profile: profile) }
    }

    func load(videoID: String, generation: UInt64) {
        guard let webView else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        guard Self.isValidVideoID(videoID) else {
            onStatus?("Enter a valid YouTube Music video ID (11 characters).")
            return
        }

        // The nonce is injected into this load's observer script. It is not an auth credential:
        // it prevents an old document's bridge messages from being accepted after a same-video
        // reload. It is deliberately not a URL parameter, because the official app rewrites its
        // own location and drops unknown query items.
        let token = UUID().uuidString
        expectedToken = token
        expectedGeneration = generation
        expectedVideoID = videoID
        loadedVideoID = videoID
        lastSequence = 0
        advertisementActive = false
        isLoading = true

        var components = URLComponents()
        components.scheme = "https"
        components.host = Self.allowedHost
        components.path = "/watch"
        components.queryItems = [URLQueryItem(name: "v", value: videoID)]
        guard let url = components.url else {
            onStatus?("Could not create the official YouTube Music route.")
            return
        }

        // Each load gets its own observer carrying this load's identity, so a document from a
        // previous load can never satisfy the checks in `handleMessage`.
        let controller = webView.configuration.userContentController
        controller.removeAllUserScripts()
        // `removeAllUserScripts` also removes the configuration-time guard, so restore it for
        // every document before adding this load's identity-bound observer.
        controller.addUserScript(WKUserScript(
            source: Self.mediaSessionGuardScript,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))
        controller.addUserScript(WKUserScript(
            source: Self.observerScript(token: token, generation: generation, videoID: videoID),
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true
        ))

        webView.load(URLRequest(url: url, cachePolicy: .useProtocolCachePolicy))
        onStatus?("Official host loading \(videoID)…")
    }

    func play() {
        evaluateMediaScript("media => { const result = media.play(); return result ? 'play-requested' : 'play-requested'; }")
    }

    func pause() {
        evaluateMediaScript("media => { media.pause(); return 'pause-requested'; }")
    }

    /// Requests a position change. Like play and pause, this is a request: the position is not
    /// treated as moved until the player reports it back through the bridge.
    func seek(to seconds: Double) {
        guard seconds.isFinite, seconds >= 0 else { return }
        guard !advertisementActive else {
            onStatus?("Seeking is unavailable while the official player is showing an advertisement.")
            return
        }
        guard let webView else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        let target = Self.javaScriptNumber(seconds)
        let script = """
        (() => {
          const candidates = [
            document.querySelector('#movie_player'),
            document.querySelector('ytmusic-player'),
            document.querySelector('ytmusic-player-bar')
          ];
          const player = candidates.find(candidate => candidate && typeof candidate.seekTo === 'function');
          if (!player) return 'seek-unsupported';
          player.seekTo(\(target), true);
          return 'seek-requested';
        })();
        """
        webView.evaluateJavaScript(script) { [weak self] value, error in
            Task { @MainActor [weak self] in
                guard let self else { return }
                if let error {
                    self.onStatus?("Official player seek was rejected: \(error.localizedDescription)")
                } else if value as? String == "seek-unsupported" {
                    self.onStatus?("This official player page does not expose its supported seek API.")
                }
            }
        }
    }

    func setVolume(_ volume: Double) {
        guard volume.isFinite else { return }
        guard !advertisementActive else {
            onStatus?("Volume is unchanged while the official player is showing an advertisement.")
            return
        }
        let target = Self.javaScriptNumber(min(max(volume, 0), 1))
        evaluateMediaScript("media => { media.muted = false; media.volume = \(target); return 'volume-requested'; }")
    }

    func setMuted(_ muted: Bool) {
        guard !advertisementActive else {
            onStatus?("Mute is unavailable while the official player is showing an advertisement.")
            return
        }
        evaluateMediaScript("media => { media.muted = \(muted ? "true" : "false"); return 'mute-requested'; }")
    }

    /// Formats a validated, finite `Double` as a JavaScript numeric literal.
    ///
    /// The value is already range-checked by the callers above; this only guarantees the
    /// literal is plain digits, with no locale separators and no exponent notation.
    private static func javaScriptNumber(_ value: Double) -> String {
        String(format: "%.3f", value)
    }

    func quiesce(completion: @escaping @MainActor () -> Void) {
        guard let webView else {
            completion()
            return
        }
        var completed = false
        let finish: @MainActor () -> Void = {
            guard !completed else { return }
            completed = true
            completion()
        }
        let script = "Array.from(document.querySelectorAll('audio,video')).forEach(media => media.pause()); 'quiesced';"
        webView.evaluateJavaScript(script) { _, _ in
            Task { @MainActor in
                finish()
            }
        }
        // A crashed or hung WebContent process must not hold Rust's lease forever. Invalidating
        // the bridge immediately after this bounded wait still prevents late samples from being
        // accepted, while release remains available to the user.
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            finish()
        }
    }

    /// Clears the lease-bound event identity. Call after media is quiesced and before releasing
    /// Rust's lease so a late event from the old document cannot be forwarded.
    func invalidateExpectations() {
        expectedToken = nil
        expectedGeneration = nil
        expectedVideoID = nil
        lastSequence = 0
        advertisementActive = false
        loadedVideoID = nil
        isLoading = false
    }

    func detach(completion: (@MainActor () -> Void)? = nil) {
        invalidateExpectations()
        quiesce { [weak self] in
            guard let self else { return }
            self.destroyRenderer()
            completion?()
        }
    }

    func detach() { detach(completion: nil) }

    private func destroyRenderer() {
        invalidateExpectations()
        webView?.stopLoading()
        webView?.navigationDelegate = nil
        webView?.uiDelegate = nil
        webView?.configuration.userContentController.removeScriptMessageHandler(forName: Self.bridgeName)
        webView?.removeFromSuperview()
        webView = nil
        messageProxy = nil
        loadedVideoID = nil
        isLoading = false
    }

    private func evaluateMediaScript(_ functionBody: String) {
        guard let webView else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        let script = """
        (() => {
          const media = document.querySelector('audio,video');
          if (!media) return 'no-media';
          return (\(functionBody))(media);
        })();
        """
        webView.evaluateJavaScript(script) { [weak self] _, error in
            guard let self, let error else { return }
            DispatchQueue.main.async {
                self.onStatus?("Official host command was rejected: \(error.localizedDescription)")
            }
        }
    }

    /// Reports what the loaded page contains.
    ///
    /// When no bridge event ever arrives, the cause is almost always the page — a consent wall,
    /// a sign-in redirect, or a player that never created a media element — and that is
    /// invisible in a host rendered at one pixel.
    func probePage() {
        guard let webView else { return }
        let script = """
        (() => {
          const media = document.querySelectorAll('audio,video');
          const text = (document.body?.innerText || '').slice(0, 200).replace(/\\s+/g, ' ');
          return JSON.stringify({
            url: location.href,
            title: document.title || '',
            media: media.length,
            readyState: media[0] ? media[0].readyState : -1,
            paused: media[0] ? media[0].paused : null,
            text
          });
        })();
        """
        webView.evaluateJavaScript(script) { [weak self] value, error in
            Task { @MainActor [weak self] in
                guard let self else { return }
                if let error {
                    self.onDiagnostics?("Page probe failed: \(error.localizedDescription)")
                } else {
                    self.onDiagnostics?(value as? String ?? "Page probe returned nothing.")
                }
            }
        }
    }

    private func handleMessage(_ message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else { return }
        let origin = message.frameInfo.securityOrigin
        guard origin.protocol == "https", origin.host == Self.allowedHost, origin.port == 0 || origin.port == 443 else {
            onStatus?("Rejected a bridge message from an untrusted origin.")
            return
        }
        guard JSONSerialization.isValidJSONObject(message.body),
              let body = try? JSONSerialization.data(withJSONObject: message.body, options: []),
              body.count <= Self.maxBridgeBodyBytes,
              let event = try? JSONDecoder().decode(BridgeEvent.self, from: body) else {
            onStatus?("Rejected an invalid official-player bridge message.")
            return
        }

        // The official app has its own autoplay queue. A well-formed event for a different video
        // means it moved on by itself; that is not a rejection to shrug at, it is a signal that
        // Goosic's own queue must take over.
        if event.version == 2,
           event.token == expectedToken,
           event.generation == expectedGeneration,
           let expected = expectedVideoID,
           event.videoID != expected,
           !event.isAdvertisement,
           !event.videoID.isEmpty {
            onStatus?("The official app moved to its own next video; Goosic's queue decides instead.")
            onPageAdvanced?(expected)
            return
        }

        if let reason = Self.rejectionReason(
            for: event,
            expectedToken: expectedToken,
            expectedGeneration: expectedGeneration,
            expectedVideoID: expectedVideoID,
            lastSequence: lastSequence
        ) {
            // An opaque rejection is unactionable, and every one of these has a different fix.
            onStatus?("Rejected an official-player bridge event: \(reason).")
            return
        }
        lastSequence = event.sequence
        advertisementActive = event.isAdvertisement
        isLoading = false
        onEvent?(OfficialPlaybackEvent(
            generation: event.generation,
            videoID: event.videoID,
            sequence: event.sequence,
            state: event.state,
            currentTime: event.currentTime,
            duration: event.duration,
            isAdvertisement: event.isAdvertisement,
            volume: event.volume,
            isMuted: event.muted
        ))
    }

    /// Why an event is not trustworthy, or `nil` when it is.
    nonisolated private static func rejectionReason(
        for event: BridgeEvent,
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

    private static func isValidVideoID(_ videoID: String) -> Bool {
        guard videoID.count == 11 else { return false }
        return videoID.allSatisfy { $0.isNumber || $0.isLetter || $0 == "-" || $0 == "_" }
    }

    /// The per-load page observer.
    ///
    /// Identity is injected rather than read from the URL: the official app rewrites its own
    /// location and drops query items it does not recognize. The video id is still read live so
    /// that a client-side navigation to a different track is reported honestly and rejected.
    private static func observerScript(token: String, generation: UInt64, videoID: String) -> String {
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
    nonisolated static let mediaSessionGuardScript = """
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
    nonisolated private static func jsonStringLiteral(_ value: String) -> String {
        guard let data = try? JSONEncoder().encode(value), let literal = String(data: data, encoding: .utf8) else {
            return "\"\""
        }
        return literal
    }

    private struct BridgeEvent: Codable {
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

    private final class ScriptMessageProxy: NSObject, WKScriptMessageHandler {
        weak var host: OfficialPlaybackHost?

        init(host: OfficialPlaybackHost) {
            self.host = host
        }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            Task { @MainActor [weak host] in
                host?.handleMessage(message)
            }
        }
    }
}

extension OfficialPlaybackHost: WKNavigationDelegate, WKUIDelegate {
    func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        let allowed = Self.isAllowedNavigation(navigationAction.request.url)
        decisionHandler(allowed ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor [weak self] in
            self?.isLoading = false
            self?.onStatus?("Official host is ready; waiting for a validated player event.")
            self?.probePage()
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        Task { @MainActor [weak self] in
            self?.isLoading = false
            self?.onStatus?("Official host navigation failed: \(error.localizedDescription)")
        }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        // Official playback is single-renderer. Popups must not create a second media owner.
        nil
    }

    nonisolated private static func isAllowedNavigation(_ url: URL?) -> Bool {
        guard let url else { return false }
        if url.absoluteString == "about:blank" { return true }
        return url.scheme == "https" && url.host == allowedHost
    }
}

struct OfficialPlaybackSurface: NSViewRepresentable {
    let model: GoosicAppModel

    func makeNSView(context: Context) -> OfficialPlaybackContainer {
        model.officialPlaybackHost.makeContainer()
    }

    func updateNSView(_ nsView: OfficialPlaybackContainer, context: Context) {
        nsView.wantsLayer = true
        nsView.layer?.opacity = 0.01
    }

    static func dismantleNSView(_ nsView: OfficialPlaybackContainer, coordinator: Void) {
        // The model owns the host and performs asynchronous media quiescing before detachment.
        // This is intentionally a no-op here; SwiftCrossUI may dismantle/recreate wrappers during
        // layout, and a second WKWebView must never be created for the same model.
    }
}
#else
import Foundation
import SwiftCrossUI

struct OfficialPlaybackProfile: Equatable {
    let identifier: UUID
    static let guest = OfficialPlaybackProfile(identifier: UUID(uuidString: "8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084")!)
}

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

/// Explicit non-macOS stub: the first supported host is AppKit/WebKit on macOS.
@MainActor
final class OfficialPlaybackHost {
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    var onDiagnostics: ((String) -> Void)?
    private(set) var loadedVideoID: String?

    func load(videoID: String, generation: UInt64) {
        loadedVideoID = nil
        onStatus?("Official playback host is only available on macOS.")
    }

    func play() { onStatus?("Official playback host is only available on macOS.") }
    func pause() { onStatus?("Official playback host is only available on macOS.") }
    func probePage() {}
    var onPageAdvanced: ((String) -> Void)?
    func seek(to seconds: Double) { onStatus?("Official playback host is only available on macOS.") }
    func setVolume(_ volume: Double) { onStatus?("Official playback host is only available on macOS.") }
    func setMuted(_ muted: Bool) { onStatus?("Official playback host is only available on macOS.") }
    func quiesce(completion: @escaping @MainActor () -> Void) { completion() }
    func bind(profile: OfficialPlaybackProfile) {}
    func detach(completion: (@MainActor () -> Void)? = nil) { completion?() }
    func detach() { detach(completion: nil) }
}

struct OfficialPlaybackSurface: View {
    let model: GoosicAppModel

    var body: some View {
        EmptyView()
    }
}
#endif
