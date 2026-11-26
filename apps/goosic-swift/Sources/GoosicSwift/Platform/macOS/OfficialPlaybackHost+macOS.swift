#if os(macOS) && !GOOSIC_PREVIEW_NO_WEBKIT
import AppKit
import Foundation
import WebKit

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
    private weak var webView: WKWebView?
    private weak var container: OfficialPlaybackContainer?
    private var messageProxy: ScriptMessageProxy?
    private var expectedToken: String?
    private var expectedGeneration: UInt64?
    private var expectedVideoID: String?
    /// Sequence from the current page document, used only to reject duplicate bridge events.
    private var lastBridgeSequence: UInt64 = 0
    /// Lease-wide sequence sent to Rust. Unlike the page sequence, it survives track reloads.
    private var sampleSequence = OfficialSampleSequence()
    private var advertisementActive = false
    private var transport = MacMediaTransportRequest()
    private var transportInFlight = false
    /// Whether the pending transport request is the listener's own press, which may answer the
    /// page's "still listening?" prompt. A load's own play and the host's requests never do.
    private var transportFromListener = false
    /// Whether this load has reported playing at all, an advertisement included.
    private var heardPlaying = false
    /// Whether the listener pressed Play or Pause since this load, or it was loaded paused.
    private var listenerTookOver = false
    /// The token whose start is being watched; see `StartWatch`.
    private var startWatchToken: String?
    private var startWatchTask: Task<Void, Never>?
    /// The last state a validated sample reported, to notice the page pausing by itself.
    private var lastSampleState = ""
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
    /// The page replaced the requested track before any of it played. The shell decides whether
    /// the replacement is the same song under another id or the page refusing the track.
    var onPageSwappedBeforePlaying: ((OfficialPageSwap) -> Void)?
    /// The requested track never started; the shell treats it as a track the page refused.
    var onNeverStarted: ((String) -> Void)?
    /// Whether the requested track has been heard playing under this load.
    private var heardRequested = false
    /// Whether this load's switch to another video has been acted on already.
    private var pageSwapHandled = false
    /// Reports in a row that describe a video other than the requested one. One alone can be the
    /// page still showing what its player held before it reached the requested video; two in a
    /// row is the page having moved, which is the Windows shell's rule too.
    private var strayReports = 0
    /// Whether this load is the listener's own choice, which the page may swap for another
    /// version of the same song once. A version loaded in its place may not be swapped again:
    /// that is the page wandering, not another version.
    private var allowsSubstitute = true

    func makeWebView(profile: OfficialPlaybackProfile = .guest) -> WKWebView {
        if let webView {
            if activeProfile.identifier == profile.identifier {
            // SwiftUI may recreate the representable wrapper during layout. Reparent the
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
        // This page exists only to play audio and nobody ever sees it. Lockdown turns off the
        // JIT and other engine features it does not need, which measured about half the memory
        // at the same CPU. It is scoped to this renderer; the login and catalog pages are unchanged.
        configuration.defaultWebpagePreferences.isLockdownModeEnabled = true
        // YouTube Music refuses to run its player under WKWebView's bare user agent and shows
        // "not optimized for your browser" instead. Naming a Safari version makes the default
        // agent a complete Safari string, which is what this engine actually is.
        configuration.applicationNameForUserAgent = OfficialBridge.safariUserAgentSuffix
        // The user pressed play in Goosic; that gesture does not cross into the web view, so
        // without this the host's own play request is blocked and no media element ever starts.
        configuration.mediaTypesRequiringUserActionForPlayback = []

        let userContentController = WKUserContentController()
        let proxy = ScriptMessageProxy(host: self)
        userContentController.add(proxy, name: OfficialBridge.name)
        // The observer is installed per load, in `load(videoID:generation:)`, because it carries
        // that load's identity.
        configuration.userContentController = userContentController
        // The embedded page must not publish a competing Now Playing session. This runs before
        // the page's own scripts; the short watchdog also clears handlers the app installs later.
        userContentController.addUserScript(WKUserScript(
            source: OfficialBridge.mediaSessionGuardScript,
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

    func load(
        videoID: String,
        generation: UInt64,
        volume: Double = 1,
        muted: Bool = false,
        allowsSubstitute: Bool = true,
        startPaused: Bool = false
    ) {
        guard let webView else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        guard OfficialBridge.isValidVideoID(videoID) else {
            onStatus?("Enter a valid YouTube Music video ID (11 characters).")
            return
        }

        // The nonce is injected into this load's observer script. It is not an auth credential:
        // it prevents an old document's bridge messages from being accepted after a same-video
        // reload. It is deliberately not a URL parameter, because the official app rewrites its
        // own location and drops unknown query items.
        transport.reset()
        transportInFlight = false
        transportFromListener = false
        stopStartWatch()
        heardPlaying = false
        listenerTookOver = startPaused
        lastSampleState = ""
        let token = UUID().uuidString
        expectedToken = token
        expectedGeneration = generation
        expectedVideoID = videoID
        loadedVideoID = videoID
        lastBridgeSequence = 0
        sampleSequence.begin(generation: generation)
        advertisementActive = false
        heardRequested = false
        pageSwapHandled = false
        strayReports = 0
        self.allowsSubstitute = allowsSubstitute
        isLoading = true

        var components = URLComponents()
        components.scheme = "https"
        components.host = OfficialBridge.allowedHost
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
        controller.addUserScript(WKUserScript(
            source: OfficialVolumeBootstrap.script(volume: volume, muted: muted),
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))
        // `removeAllUserScripts` also removes the configuration-time guard, so restore it for
        // every document before adding this load's identity-bound observer.
        controller.addUserScript(WKUserScript(
            source: OfficialBridge.mediaSessionGuardScript,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        ))
        controller.addUserScript(WKUserScript(
            source: OfficialBridge.observerScript(token: token, generation: generation, videoID: videoID),
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true
        ))

        webView.load(URLRequest(url: url, cachePolicy: .useProtocolCachePolicy))
        // A native card tap is the playback gesture. Queue the intent before navigation so the
        // first observer event can start the media as soon as WebKit creates its element. A
        // version loaded in place of a swapped song keeps a pause the listener asked for while
        // its title was being read, instead of starting anyway.
        transport.request(paused: startPaused)
        onStatus?("Official host loading \(videoID)…")
    }

    /// `fromListener` is false for a play the shell makes on its own, such as resuming content an
    /// advertisement left paused; only a listener's press answers the page's prompt.
    func play(fromListener: Bool = true) {
        requestTransport(paused: false, fromListener: fromListener)
    }

    func pause() {
        requestTransport(paused: true, fromListener: true)
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
        let target = Self.javaScriptNumber(min(max(volume, 0), 1))
        evaluateMediaScript("media => { window.goosicSetVolumePreference?.(\(target), undefined); return 'volume-requested'; }")
    }

    func setMuted(_ muted: Bool) {
        evaluateMediaScript("media => { window.goosicSetVolumePreference?.(undefined, \(muted ? "true" : "false")); return 'mute-requested'; }")
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
        transport.reset()
        transportInFlight = false
        transportFromListener = false
        stopStartWatch()
        heardPlaying = false
        listenerTookOver = false
        lastSampleState = ""
        expectedToken = nil
        expectedGeneration = nil
        expectedVideoID = nil
        lastBridgeSequence = 0
        sampleSequence.reset()
        advertisementActive = false
        heardRequested = false
        pageSwapHandled = false
        strayReports = 0
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
        webView?.configuration.userContentController.removeScriptMessageHandler(forName: OfficialBridge.name)
        webView?.removeFromSuperview()
        webView = nil
        messageProxy = nil
        loadedVideoID = nil
        isLoading = false
    }

    private func requestTransport(paused: Bool, fromListener: Bool) {
        guard expectedToken != nil, webView != nil else {
            onStatus?("Choose a track before controlling playback.")
            return
        }
        if fromListener { listenerTookOver = true }
        transportFromListener = fromListener
        transport.request(paused: paused)
        drainTransport()
    }

    /// Keep an early click until a validated sample proves that the media element exists.
    /// Results from a replaced document or superseded request cannot clear the latest intent.
    private func drainTransport() {
        guard !transportInFlight, let paused = transport.paused,
              let token = expectedToken, let webView else { return }
        let revision = transport.revision
        transportInFlight = true
        // The shared scripts pick the active element, retry a play the page interrupted, and
        // name the error when it still refuses. A bare `play()` here dropped the request on its
        // first rejection, without the reason, and the track sat silent.
        let script = paused
            ? OfficialBridge.pauseScript
            : (transportFromListener ? OfficialBridge.listenerPlayScript : OfficialBridge.hostPlayScript)
        webView.callAsyncJavaScript(
            "return await \(script);", arguments: [:], in: nil, in: .page
        ) { [weak self] result in
            guard let self, self.expectedToken == token else { return }
            self.transportInFlight = false
            guard self.transport.revision == revision else {
                self.drainTransport()
                return
            }
            switch result {
            case .success(let value):
                switch PlayOutcome(value as? String ?? "") {
                case .noMedia:
                    // Kept: the next validated sample proves the element exists and drains again.
                    self.onStatus?("Waiting for the official player to become ready…")
                case .failed(let name) where !paused:
                    self.transport.complete(revision: revision)
                    Diagnostics.note(.officialPlayback, "play-failed", ["error": name])
                    self.onStatus?("The official player refused to start (\(name)). Try Play again.")
                default:
                    self.transport.complete(revision: revision)
                }
            case .failure(let error):
                self.transport.complete(revision: revision)
                let failure = error as NSError
                Diagnostics.note(.officialPlayback, "transport-script-failed", [
                    "paused": "\(paused)", "domain": failure.domain, "code": "\(failure.code)",
                ])
                self.onStatus?("The official player could not start or pause playback. Try Play again.")
            }
        }
    }

    /// Watches this load until its track is heard, asks a silent page to play once, and gives up
    /// on a track that never starts; `StartWatch` holds the rule, which Windows shares. Started
    /// once per load, when its page is ready.
    private func beginStartWatch() {
        guard let token = expectedToken, startWatchToken != token else { return }
        startWatchTask?.cancel()
        startWatchToken = token
        let began = Date()
        startWatchTask = Task { @MainActor [weak self] in
            var nudged = false
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 500_000_000)
                guard let self, !Task.isCancelled, self.expectedToken == token else { return }
                switch StartWatch.decide(
                    sinceLoaded: Date().timeIntervalSince(began),
                    heardPlaying: self.heardPlaying,
                    listenerTookOver: self.listenerTookOver,
                    nudged: nudged
                ) {
                case .wait:
                    continue
                case .done:
                    return
                case .nudge:
                    nudged = true
                    self.nudgePlay(token: token)
                case .giveUp:
                    Diagnostics.note(.officialPlayback, "never-started")
                    self.probePage()
                    let requested = self.expectedVideoID ?? ""
                    self.onNeverStarted?(requested)
                    return
                }
            }
        }
    }

    private func stopStartWatch() {
        startWatchTask?.cancel()
        startWatchTask = nil
        startWatchToken = nil
    }

    /// Asks a page that loaded without starting to play, once; the outcome is only logged.
    private func nudgePlay(token: String) {
        guard let webView else { return }
        webView.callAsyncJavaScript(
            "return await \(OfficialBridge.hostPlayScript);", arguments: [:], in: nil, in: .page
        ) { [weak self] result in
            guard let self, self.expectedToken == token else { return }
            let answer: String
            switch result {
            case .success(let value): answer = value as? String ?? "unknown"
            case .failure(let error): answer = "script-failed:\((error as NSError).code)"
            }
            Diagnostics.note(.officialPlayback, "start-nudge", ["result": answer])
        }
    }

    /// Explains a pause nobody in Goosic asked for when it is YouTube Music's "still listening?"
    /// prompt. The prompt is answered only by the listener pressing Play.
    private func noticeIdlePrompt(token: String) {
        guard let webView else { return }
        webView.evaluateJavaScript(OfficialBridge.idlePromptScript) { [weak self] value, _ in
            Task { @MainActor [weak self] in
                guard let self, self.expectedToken == token, value as? Bool == true else { return }
                Diagnostics.note(.officialPlayback, "idle-prompt")
                self.onStatus?("YouTube Music paused to check you\u{2019}re still listening. Press Play to continue.")
            }
        }
    }

    private func evaluateMediaScript(_ functionBody: String) {
        guard let webView else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        let script = """
        (() => {
          const media = \(OfficialBridge.activeMediaElementScript);
          if (!media) return 'no-media';
          return (\(functionBody))(media);
        })();
        """
        webView.evaluateJavaScript(script) { [weak self] value, error in
            guard let self else { return }
            DispatchQueue.main.async {
                if error != nil {
                    self.onStatus?("The official player rejected this control. Try again once playback is ready.")
                } else if value as? String == "no-media" {
                    self.onStatus?("The official player is still loading. Try this control again shortly.")
                }
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

    /// Runs `OfficialBridge.automixOffScript` until the page answers, for this load only, so a
    /// track plays to its end instead of fading into one YouTube Music chose.
    func turnOffAutomix(token: String?, attempt: Int = 0) {
        guard let webView, let token, token == expectedToken,
              attempt < OfficialBridge.automixAttempts else { return }
        webView.evaluateJavaScript(OfficialBridge.automixOffScript) { [weak self] value, _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                let answer = value as? String ?? "absent"
                guard answer == "absent" else {
                    Diagnostics.note(.officialPlayback, "automix", ["result": answer])
                    return
                }
                try? await Task.sleep(nanoseconds: 500_000_000)
                self.turnOffAutomix(token: token, attempt: attempt + 1)
            }
        }
    }

    private func handleMessage(_ message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else { return }
        let origin = message.frameInfo.securityOrigin
        guard origin.protocol == "https", origin.host == OfficialBridge.allowedHost, origin.port == 0 || origin.port == 443 else {
            onStatus?("Rejected a bridge message from an untrusted origin.")
            return
        }
        guard JSONSerialization.isValidJSONObject(message.body),
              let body = try? JSONSerialization.data(withJSONObject: message.body, options: []),
              body.count <= OfficialBridge.maxBodyBytes,
              let event = try? JSONDecoder().decode(OfficialBridge.Event.self, from: body) else {
            onStatus?("Rejected an invalid official-player bridge message.")
            return
        }

        // The official app has its own autoplay queue. A well-formed event for a different video
        // means it moved on by itself; that is not a rejection to shrug at, it is a signal that
        // Goosic's own queue must take over -- but only once the requested track was heard. A
        // switch before that cannot be its end: it is the same song under another id, or the
        // page refusing the track, and taking it for the end skipped the song and started the
        // next row of the list.
        if event.version == 2,
           let token = expectedToken,
           event.token == token,
           event.generation == expectedGeneration,
           let expected = expectedVideoID,
           event.videoID != expected,
           !event.isAdvertisement,
           OfficialBridge.isValidVideoID(event.videoID) {
            guard !pageSwapHandled else { return }
            strayReports += 1
            guard strayReports >= 2 else { return }
            pageSwapHandled = true
            if heardRequested {
                onStatus?("The official app moved to its own next video; Goosic's queue decides instead.")
                onPageAdvanced?(expected)
            } else {
                resolveEarlySwitch(requested: expected, actual: event.videoID, token: token)
            }
            return
        }

        if let reason = OfficialBridge.rejectionReason(
            body: body,
            expectedToken: expectedToken,
            expectedGeneration: expectedGeneration,
            expectedVideoID: expectedVideoID,
            lastSequence: lastBridgeSequence
        ) {
            // An opaque rejection is unactionable, and every one of these has a different fix.
            onStatus?("Rejected an official-player bridge event: \(reason).")
            Diagnostics.note(.officialPlayback, "event-rejected", ["reason": reason])
            return
        }
        // "Requested", "ready", and "confirmed" are three different things, and only this one
        // means the renderer is actually producing the track Goosic asked for. Without the
        // distinction a load that silently never started looks identical to one that did.
        if isLoading {
            Diagnostics.note(.officialPlayback, "playback-confirmed", [
                "generation": "\(event.generation)",
                "sequence": "\(event.sequence)",
                "advertisement": "\(event.isAdvertisement)",
            ])
        }
        lastBridgeSequence = event.sequence
        strayReports = 0
        if event.state == "playing" {
            // An advertisement counts: the page is working, and the track follows it.
            heardPlaying = true
        } else if event.state == "paused", lastSampleState == "playing", let token = expectedToken {
            noticeIdlePrompt(token: token)
        }
        lastSampleState = event.state
        if PageSwap.heardPlaying(
            state: event.state, duration: event.duration, isAdvertisement: event.isAdvertisement
        ) {
            heardRequested = true
        }
        guard let leaseSequence = sampleSequence.issue(for: event.generation) else {
            onStatus?("Rejected an official-player event with no active playback lease.")
            return
        }
        advertisementActive = event.isAdvertisement
        isLoading = false
        drainTransport()
        onEvent?(OfficialPlaybackEvent(
            generation: event.generation,
            videoID: event.videoID,
            sequence: leaseSequence,
            state: event.state,
            currentTime: event.currentTime,
            duration: event.duration,
            isAdvertisement: event.isAdvertisement,
            volume: event.volume,
            isMuted: event.muted
        ))
    }

    /// Reads the title of the video the page switched to, and hands the decision to the shell.
    ///
    /// The observer in this document pauses any video other than the requested one, so nothing
    /// sounds while this runs. `getVideoData` is the player's own description of what it holds.
    private func resolveEarlySwitch(requested: String, actual: String, token: String) {
        Diagnostics.note(.officialPlayback, "switched-before-playing")
        let mayAdopt = allowsSubstitute
        let report: @MainActor (String) -> Void = { [weak self] title in
            guard let self, self.expectedToken == token else { return }
            self.onPageSwappedBeforePlaying?(OfficialPageSwap(
                requestedVideoID: requested,
                pageVideoID: actual,
                pageTitle: title,
                mayAdopt: mayAdopt
            ))
        }
        readPageTitle(of: actual, requested: requested, token: token, attempt: 0, completion: report)
    }

    /// How many times, half a second apart, the title of a switched-to video is asked for.
    ///
    /// The page switches before it has finished loading, often before its player can describe
    /// the new video at all. Asking once read an empty title, and an empty title can never match,
    /// so another version of the very song asked for was skipped as a refusal.
    private static let pageTitleAttempts = 8

    /// Reads the player's own title for `videoID`, retrying while the player has not caught up.
    private func readPageTitle(
        of videoID: String,
        requested: String,
        token: String,
        attempt: Int,
        completion: @escaping @MainActor (String) -> Void
    ) {
        guard let webView, expectedToken == token else {
            completion("")
            return
        }
        let script = """
        (() => {
          const data = document.getElementById('movie_player')?.getVideoData?.();
          return JSON.stringify({ id: String(data?.video_id || ''), title: String(data?.title || '') });
        })();
        """
        webView.evaluateJavaScript(script) { [weak self] value, _ in
            Task { @MainActor [weak self] in
                guard let self, self.expectedToken == token else { return }
                struct Described: Decodable { let id: String; let title: String }
                let described = (value as? String)
                    .flatMap { $0.data(using: .utf8) }
                    .flatMap { try? JSONDecoder().decode(Described.self, from: $0) }
                // Only a description of the switched-to video counts; the player can still be
                // describing the one it is leaving.
                if described?.id == requested {
                    // The player is on the requested song after all: the switch was the page
                    // passing through another video on its way, not a swap or a refusal.
                    Diagnostics.note(.officialPlayback, "switch-was-transient", ["attempt": "\(attempt)"])
                    self.pageSwapHandled = false
                    self.strayReports = 0
                } else if let described, described.id == videoID, !described.title.isEmpty {
                    Diagnostics.note(.officialPlayback, "switched-title", [
                        "attempt": "\(attempt)", "title": described.title,
                    ])
                    completion(described.title)
                } else if attempt + 1 < Self.pageTitleAttempts {
                    try? await Task.sleep(nanoseconds: 500_000_000)
                    self.readPageTitle(
                        of: videoID, requested: requested, token: token,
                        attempt: attempt + 1, completion: completion)
                } else {
                    Diagnostics.note(.officialPlayback, "switched-title-unread", [
                        "attempts": "\(Self.pageTitleAttempts)",
                    ])
                    completion("")
                }
            }
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
        decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
    ) {
        let decision = OfficialNavigationPolicy.decide(
            url: navigationAction.request.url,
            frame: NavigationFrame(navigationAction.targetFrame)
        )
        decisionHandler(decision == .allow ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor [weak self] in
            self?.isLoading = false
            self?.onStatus?("Official host is ready; waiting for a validated player event.")
            Diagnostics.note(.officialPlayback, "page-ready", [
                "origin": Diagnostics.origin(of: webView.url),
            ])
            self?.probePage()
            self?.turnOffAutomix(token: self?.expectedToken)
            self?.beginStartWatch()
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        let failure = error as NSError
        Task { @MainActor [weak self] in
            self?.navigationFailed(failure)
        }
    }

    /// A load that fails before the page commits -- no connection, a DNS failure, a timeout --
    /// arrives here rather than at `didFail`. It was not handled at all, so the host stayed
    /// "loading" with nothing in the log, and the track sat silent.
    func webView(
        _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error
    ) {
        let failure = error as NSError
        Task { @MainActor [weak self] in
            self?.navigationFailed(failure)
        }
    }

    private func navigationFailed(_ failure: NSError) {
        // A navigation replaced by the next load, or by the page's own rewrite of its location,
        // is not a failure.
        let superseded = (failure.domain == NSURLErrorDomain && failure.code == NSURLErrorCancelled)
            || (failure.domain == "WebKitErrorDomain" && failure.code == 102)
        guard !superseded else { return }
        isLoading = false
        Diagnostics.note(.officialPlayback, "navigation-failed", [
            "domain": failure.domain, "code": "\(failure.code)",
        ])
        onStatus?("Official host navigation failed: \(failure.localizedDescription)")
        // Watched from here too, so a page that never arrives is given up on rather than left
        // silent, and the queue moves on.
        beginStartWatch()
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        // Official playback is single-renderer. Popups must not create a second media owner.
        nil
    }

}

#endif
