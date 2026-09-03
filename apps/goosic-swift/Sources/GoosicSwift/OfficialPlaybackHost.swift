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
}

struct OfficialPlaybackProfile {
    /// Guest storage is intentionally stable. Account profiles will supply their own stable UUID
    /// once account isolation is connected; cookies are never exported from this store.
    let identifier: UUID

    static let guest = OfficialPlaybackProfile(
        identifier: UUID(uuidString: "8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084")!
    )
}

/// Owns the single WKWebView and the security boundary around its bridge.
@MainActor
final class OfficialPlaybackHost: NSObject {
    private static let maxBridgeBodyBytes = 16 * 1024
    nonisolated private static let allowedHost = "music.youtube.com"
    private static let bridgeName = "goosicBridge"

    private weak var webView: WKWebView?
    private var messageProxy: ScriptMessageProxy?
    private var expectedToken: String?
    private var expectedGeneration: UInt64?
    private var expectedVideoID: String?
    private var lastSequence: UInt64 = 0
    private(set) var loadedVideoID: String?
    private(set) var isLoading = false
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?

    func makeWebView(profile: OfficialPlaybackProfile = .guest) -> WKWebView {
        precondition(webView == nil, "Goosic owns exactly one official playback web view")

        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = WKWebsiteDataStore(forIdentifier: profile.identifier)
        configuration.preferences.isFraudulentWebsiteWarningEnabled = true

        let userContentController = WKUserContentController()
        let proxy = ScriptMessageProxy(host: self)
        userContentController.add(proxy, name: Self.bridgeName)
        userContentController.addUserScript(WKUserScript(
            source: Self.observerScript,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true
        ))
        configuration.userContentController = userContentController

        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = self
        view.uiDelegate = self
        view.allowsMagnification = false
        webView = view
        messageProxy = proxy
        return view
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

        // The nonce is passed only as an opaque route parameter. It is not an auth credential.
        // It prevents an old document's bridge messages from being accepted after a same-video
        // reload while preserving the official watch route and its cookies.
        let token = UUID().uuidString
        expectedToken = token
        expectedGeneration = generation
        expectedVideoID = videoID
        loadedVideoID = videoID
        lastSequence = 0
        isLoading = true

        var components = URLComponents()
        components.scheme = "https"
        components.host = Self.allowedHost
        components.path = "/watch"
        components.queryItems = [
            URLQueryItem(name: "v", value: videoID),
            URLQueryItem(name: "goosicGeneration", value: String(generation)),
            URLQueryItem(name: "goosicSession", value: token),
        ]
        guard let url = components.url else {
            onStatus?("Could not create the official YouTube Music route.")
            return
        }
        webView.load(URLRequest(url: url, cachePolicy: .useProtocolCachePolicy))
        onStatus?("Official host loading \(videoID)…")
    }

    func play() {
        evaluateMediaScript("media => { const result = media.play(); return result ? 'play-requested' : 'play-requested'; }")
    }

    func pause() {
        evaluateMediaScript("media => { media.pause(); return 'pause-requested'; }")
    }

    func quiesce(completion: @escaping @MainActor () -> Void) {
        guard let webView else {
            completion()
            return
        }
        let script = "Array.from(document.querySelectorAll('audio,video')).forEach(media => media.pause()); 'quiesced';"
        webView.evaluateJavaScript(script) { _, _ in
            DispatchQueue.main.async {
                completion()
            }
        }
    }

    func detach() {
        quiesce { [weak self] in
            guard let self else { return }
            self.webView?.stopLoading()
            self.webView?.navigationDelegate = nil
            self.webView?.uiDelegate = nil
            self.webView?.configuration.userContentController.removeScriptMessageHandler(forName: Self.bridgeName)
            self.webView = nil
            self.messageProxy = nil
            self.expectedToken = nil
            self.expectedGeneration = nil
            self.expectedVideoID = nil
            self.loadedVideoID = nil
            self.isLoading = false
        }
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

        guard event.version == 2,
              event.token == expectedToken,
              event.generation == expectedGeneration,
              event.videoID == expectedVideoID,
              event.sequence > lastSequence,
              event.currentTime.isFinite,
              event.duration.isFinite,
              event.currentTime >= 0,
              event.duration >= 0 else {
            onStatus?("Rejected a stale or invalid official-player bridge event.")
            return
        }
        lastSequence = event.sequence
        isLoading = false
        onEvent?(OfficialPlaybackEvent(
            generation: event.generation,
            videoID: event.videoID,
            sequence: event.sequence,
            state: event.state,
            currentTime: event.currentTime,
            duration: event.duration,
            isAdvertisement: event.isAdvertisement
        ))
    }

    private static func isValidVideoID(_ videoID: String) -> Bool {
        guard videoID.count == 11 else { return false }
        return videoID.allSatisfy { $0.isNumber || $0.isLetter || $0 == "-" || $0 == "_" }
    }

    private static let observerScript = """
    (() => {
      const params = new URLSearchParams(window.location.search);
      const token = params.get('goosicSession') || crypto.randomUUID();
      const generation = Number(params.get('goosicGeneration') || 0);
      const videoId = params.get('v') || '';
      let sequence = 0;
      let media;
      let timer;
      const isAd = () => Boolean(document.querySelector(
        '.ad-showing, .ytp-ad-player-overlay, .ytp-ad-text, [id*=ad], [class*=ad-showing]'
      ));
      const send = () => {
        media = document.querySelector('audio,video');
        if (!media || !window.webkit?.messageHandlers?.goosicBridge) return;
        const currentTime = Number.isFinite(media.currentTime) && media.currentTime >= 0 ? media.currentTime : 0;
        const duration = Number.isFinite(media.duration) && media.duration >= 0 ? media.duration : 0;
        const state = media.ended ? 'ended' : media.paused ? 'paused' : 'playing';
        window.webkit.messageHandlers.goosicBridge.postMessage({
          version: 2, token, generation, videoId, sequence: ++sequence, state,
          currentTime, duration, isAdvertisement: isAd()
        });
      };
      const install = () => {
        const next = document.querySelector('audio,video');
        if (next === media) return;
        media = next;
        if (!media) return;
        ['play','pause','ended','timeupdate','durationchange','loadedmetadata'].forEach(name => {
          media.addEventListener(name, send, { passive: true });
        });
        send();
      };
      install();
      new MutationObserver(install).observe(document.documentElement, { childList: true, subtree: true });
      timer = window.setInterval(send, 500);
    })();
    """

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

        enum CodingKeys: String, CodingKey {
            case version, token, generation, sequence, state, currentTime, duration, isAdvertisement
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

    func makeNSView(context: Context) -> WKWebView {
        model.officialPlaybackHost.makeWebView()
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {
        nsView.wantsLayer = true
        nsView.layer?.opacity = 0.01
    }

    static func dismantleNSView(_ nsView: WKWebView, coordinator: Void) {
        // The model owns the host and performs asynchronous media quiescing before detachment.
        // This is intentionally a no-op here; SwiftCrossUI may dismantle/recreate wrappers during
        // layout, and a second WKWebView must never be created for the same model.
    }
}
#else
import SwiftCrossUI

struct OfficialPlaybackEvent {
    let generation: UInt64
    let videoID: String
    let sequence: UInt64
    let state: String
    let currentTime: Double
    let duration: Double
    let isAdvertisement: Bool
}

/// Explicit non-macOS stub: the first supported host is AppKit/WebKit on macOS.
@MainActor
final class OfficialPlaybackHost {
    var onEvent: ((OfficialPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?
    private(set) var loadedVideoID: String?

    func load(videoID: String, generation: UInt64) {
        loadedVideoID = nil
        onStatus?("Official playback host is only available on macOS.")
    }

    func play() { onStatus?("Official playback host is only available on macOS.") }
    func pause() { onStatus?("Official playback host is only available on macOS.") }
    func quiesce(completion: @escaping @MainActor () -> Void) { completion() }
    func detach() {}
}

struct OfficialPlaybackSurface: View {
    let model: GoosicAppModel

    var body: some View {
        EmptyView()
    }
}
#endif
