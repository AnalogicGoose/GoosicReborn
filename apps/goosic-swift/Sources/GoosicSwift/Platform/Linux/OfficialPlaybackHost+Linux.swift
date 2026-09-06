#if os(Linux)
import Foundation
import SwiftCrossUI

/// Owns the single WebKitGTK renderer and the boundary around its bridge.
///
/// The Linux counterpart of the macOS host. What counts as a trustworthy event is deliberately
/// not decided again here: `OfficialBridge` answers that for both platforms, so "may this play"
/// has one answer. What differs is only how a script is run and how a message arrives.
@MainActor
final class OfficialPlaybackHost {
    private var widget: WebKitWebViewWidget?
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

    /// Hands out the one renderer, creating it the first time it is asked for.
    ///
    /// The host owns the widget rather than the representable that displays it. A representable
    /// is rebuilt during layout, and a renderer built there would be a second media owner while
    /// the first is still loaded and still playing.
    func makeWidget() -> WebKitWebViewWidget {
        if let widget { return widget }
        let widget = WebKitWebViewWidget()
        attach(widget)
        return widget
    }

    /// Binds this host to the renderer it drives. The widget goes on to live in the SwiftCrossUI
    /// hierarchy, which is why nothing here ever destroys it.
    ///
    /// Both callbacks capture `self` weakly. They are stored on the widget, and the host holds
    /// the widget, so a strong capture would be a cycle that keeps a superseded renderer — signal
    /// handlers, scripts and all — alive for the life of the process.
    func attach(_ widget: WebKitWebViewWidget) {
        guard self.widget !== widget else { return }
        self.widget = widget
        widget.guardNavigation()
        widget.observeLoad { [weak self] in
            // GTK emits its signals on the main loop thread, which is this actor's thread.
            MainActor.assumeIsolated {
                // The blank document `blankRenderer` loads finishes loading too, and calling
                // that "ready" right after a detach would be a lie.
                guard let self, self.loadedVideoID != nil else { return }
                self.isLoading = false
                self.onStatus?("Official host is ready; waiting for a validated player event.")
                self.probePage()
            }
        }
        widget.openBridge { [weak self] json in
            // Synchronous on purpose: the sequence check depends on samples arriving in the
            // order the observer sent them, and hopping through a task does not promise that.
            MainActor.assumeIsolated { self?.handleMessage(json) }
        }
    }

    /// Records the profile and says plainly that it is not isolated yet.
    ///
    /// Account isolation needs a `WebKitNetworkSession` per profile, and a session is given to a
    /// web view when it is created rather than swapped into one, so honouring this means
    /// rebuilding the renderer. Until that exists, staying quiet would let a caller believe it
    /// is playing under an account while it plays under guest storage.
    func bind(profile: OfficialPlaybackProfile) {
        guard profile != activeProfile else { return }
        activeProfile = profile
        if profile != .guest {
            onStatus?("Official playback on Linux runs under guest storage; account profiles are not isolated yet.")
        }
    }

    func load(videoID: String, generation: UInt64) {
        guard let widget else {
            onStatus?("Official host is not attached to the native view.")
            return
        }
        guard OfficialBridge.isValidVideoID(videoID) else {
            onStatus?("Enter a valid YouTube Music video ID (11 characters).")
            return
        }

        // The nonce is injected into this load's observer script. It is not a credential: it
        // prevents an old document's messages from being accepted after a same-video reload.
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
        components.host = OfficialBridge.allowedHost
        components.path = "/watch"
        components.queryItems = [URLQueryItem(name: "v", value: videoID)]
        guard let url = components.url else {
            onStatus?("Could not create the official YouTube Music route.")
            return
        }

        // Each load gets its own observer carrying this load's identity, so a document from a
        // previous load can never satisfy the checks in `handleMessage`.
        widget.removeInstalledScripts()
        widget.install(pageScript: OfficialBridge.mediaSessionGuardScript)
        widget.install(script: OfficialBridge.observerScript(
            token: token, generation: generation, videoID: videoID
        ))
        widget.loadPage(url: url.absoluteString)
        onStatus?("Official host loading \(videoID)…")
    }

    func play() {
        evaluateMediaScript("media => { media.play(); return 'play-requested'; }")
    }

    func pause() {
        evaluateMediaScript("media => { media.pause(); return 'pause-requested'; }")
    }

    /// Requests a position change. Like play and pause this is a request: the position is not
    /// treated as moved until the player reports it back through the bridge.
    func seek(to seconds: Double) {
        guard seconds.isFinite, seconds >= 0 else { return }
        guard !advertisementActive else {
            onStatus?("Seeking is unavailable while the official player is showing an advertisement.")
            return
        }
        guard let widget else {
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
        widget.evaluate(script) { json, failure in
            MainActor.assumeIsolated { [weak self] in
                guard let self else { return }
                if let failure {
                    self.onStatus?("Official player seek was rejected: \(failure).")
                } else if let json, Self.text(fromJSON: json) == "seek-unsupported" {
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

    func quiesce(completion: @escaping @MainActor () -> Void) {
        guard let widget else {
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
        widget.evaluate(script) { _, _ in
            MainActor.assumeIsolated { finish() }
        }
        // A hung web process must not hold Rust's lease forever. Invalidating the bridge after
        // this bounded wait still prevents late samples from being accepted.
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            finish()
        }
    }

    /// Clears the lease-bound event identity. Call after media is quiesced and before releasing
    /// Rust's lease, so a late event from the old document cannot be forwarded.
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
            self?.blankRenderer()
            completion?()
        }
    }

    func detach() { detach(completion: nil) }

    /// Returns the renderer to a document that can play nothing, without destroying it. The
    /// widget belongs to the view hierarchy, and tearing it down here would leave the shell
    /// holding a dead child; dropping the scripts is what actually ends the bridge.
    private func blankRenderer() {
        widget?.stopLoading()
        widget?.removeInstalledScripts()
        widget?.load(html: "<!doctype html><title>Goosic</title>")
        loadedVideoID = nil
        isLoading = false
    }

    /// Reports what the loaded page contains.
    ///
    /// When no bridge event ever arrives the cause is almost always the page — a consent wall, a
    /// sign-in redirect, or a player that never created a media element — and that is invisible
    /// in a host rendered at one pixel.
    func probePage() {
        guard let widget else { return }
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
        widget.evaluate(script) { json, failure in
            MainActor.assumeIsolated { [weak self] in
                guard let self else { return }
                if let failure {
                    self.onDiagnostics?("Page probe failed: \(failure).")
                } else if let json {
                    self.onDiagnostics?(Self.text(fromJSON: json))
                } else {
                    self.onDiagnostics?("Page probe returned nothing.")
                }
            }
        }
    }

    private func evaluateMediaScript(_ functionBody: String) {
        guard let widget else {
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
        widget.evaluate(script) { _, failure in
            guard let failure else { return }
            MainActor.assumeIsolated { [weak self] in
                self?.onStatus?("Official host command was rejected: \(failure).")
            }
        }
    }

    /// A script's result arrives as JSON, so a returned string is a quoted literal.
    private static func text(fromJSON json: String) -> String {
        (try? JSONDecoder().decode(String.self, from: Data(json.utf8))) ?? json
    }

    /// Formats a validated, finite `Double` as a JavaScript numeric literal, with no locale
    /// separators and no exponent notation.
    private static func javaScriptNumber(_ value: Double) -> String {
        String(format: "%.3f", value)
    }

    private func handleMessage(_ json: String) {
        // macOS checks `frameInfo.securityOrigin` after the fact because the page can post and
        // be rejected. Here the handler lives in an isolated world and the observer is injected
        // only into the top frame of one origin, so a page script has nothing to post to.
        let body = Data(json.utf8)
        guard body.count <= OfficialBridge.maxBodyBytes,
              let event = try? JSONDecoder().decode(OfficialBridge.Event.self, from: body) else {
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

        if let reason = OfficialBridge.rejectionReason(
            for: event,
            expectedToken: expectedToken,
            expectedGeneration: expectedGeneration,
            expectedVideoID: expectedVideoID,
            lastSequence: lastSequence
        ) {
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
}

/// Mounts the one renderer and hands it to the host. The macOS side does the same job with
/// `NSViewRepresentable` and a container view.
struct OfficialPlaybackSurface: View {
    let model: GoosicAppModel

    var body: some View {
        WebKitSurface(widget: model.officialPlaybackHost.makeWidget())
    }
}
#endif