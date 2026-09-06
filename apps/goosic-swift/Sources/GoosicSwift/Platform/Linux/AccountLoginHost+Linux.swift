#if os(Linux)
import CWebKitGTK
import Foundation
import Gtk

/// The Linux sign-in surface: a window of its own holding a renderer whose cookies land in a
/// staging directory, never in the profile a signed-in account will eventually use.
///
/// It mirrors the macOS host, including the part that matters most. Nothing about the sign-in is
/// promoted until Rust has accepted the account, so an abandoned or failed login leaves a
/// directory that is deleted rather than a profile that half exists. The rules that decide where
/// the window may navigate and when a sign-in has completed are the shared ones in
/// `AccountLoginValidation`: a second copy would be a second answer about where a password may
/// be typed.
@MainActor
final class AccountLoginHost {
    var onCompleted: ((AccountLoginResult, AccountLoginHost) -> Void)?
    var onCancelled: (() -> Void)?

    private var window: Gtk.Window?
    private var webView: WebKitWebViewWidget?
    private var accountId: UUID?
    private var profileId: UUID?
    private var stagingDirectory: String?
    private var promotionCommitted = false
    private var awaitingPromotion = false
    private var closing = false
    private var completionDelivered = false
    private var navigationToken: UInt64 = 0
    private var pollingTask: Task<Void, Never>?

    func start() {
        guard window == nil else { return }
        // Both identifiers are generated before the surface opens and are never derived from
        // provider data. They are stable for this staged login and distinct by construction.
        let accountId = UUID()
        var profileId = UUID()
        while profileId == accountId { profileId = UUID() }
        self.accountId = accountId
        self.profileId = profileId

        let staging = Self.stagingDirectory(for: profileId)
        stagingDirectory = staging

        let webView = WebKitWebViewWidget(profileDirectory: staging)
        webView.guardNavigation(.loginHosts)
        // Deliberately no bridge: a login is a browser surface, not a playback channel. Native
        // code only evaluates the bounded metadata projection, and only at exact completion.
        webView.observeLoad { [weak self] in
            MainActor.assumeIsolated { self?.startCompletionPolling() }
        }
        self.webView = webView

        let window = Gtk.Window()
        window.title = "Sign in to YouTube Music"
        window.defaultSize = Size(width: 720, height: 640)
        window.setChild(webView)
        self.window = window
        Self.observeClose(of: window, host: self)
        window.present()

        var components = URLComponents()
        components.scheme = "https"
        components.host = "accounts.google.com"
        components.path = "/ServiceLogin"
        components.queryItems = [URLQueryItem(name: "continue", value: "https://music.youtube.com")]
        guard let url = components.url else { return cancel() }
        webView.loadPage(url: url.absoluteString)
    }

    func close() {
        guard !closing else { return }
        closing = true
        pollingTask?.cancel()
        pollingTask = nil
        navigationToken &+= 1
        webView?.stopLoading()
        webView?.removeInstalledScripts()
        if !awaitingPromotion && !promotionCommitted { discardStaging() }
        window?.destroy()
        webView = nil
        window = nil
    }

    /// Rust calls this only after `accounts.upsert` and `accounts.activate` both succeed. Until
    /// then the profile is a staged directory and can be deleted on any failure.
    func commitPromotion() {
        guard awaitingPromotion else { return }
        promotionCommitted = true
        awaitingPromotion = false
        stagingDirectory = nil
    }

    func discardStaging() {
        guard !promotionCommitted else { return }
        awaitingPromotion = false
        deleteStagingDirectory()
        stagingDirectory = nil
    }

    private func cancel() {
        close()
        onCancelled?()
    }

    /// Deletes the whole staged tree rather than asking WebKit to clear its data. The directory
    /// belongs to this login and to nothing else, so removing it cannot take anything with it,
    /// and it leaves nothing behind if the process dies mid-clear.
    private func deleteStagingDirectory() {
        guard let staging = stagingDirectory else { return }
        try? FileManager.default.removeItem(atPath: staging)
    }

    private static func stagingDirectory(for profileId: UUID) -> String {
        let environment = ProcessInfo.processInfo.environment
        let root = environment["XDG_DATA_HOME"].flatMap { $0.isEmpty ? nil : $0 }
            ?? FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".local/share").path
        return root + "/goosic/staging/" + profileId.uuidString
    }

    // MARK: - Completion

    /// Polls the loaded page for the marker that says the account menu is really there.
    ///
    /// Arriving at `music.youtube.com` is not enough on its own — the signed-out shell lives at
    /// the same address — so the check runs against the live document and the origin is verified
    /// again each time, not once at the start.
    private func startCompletionPolling() {
        pollingTask?.cancel()
        navigationToken &+= 1
        let token = navigationToken
        pollingTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let deadline = Date().addingTimeInterval(AccountLoginValidation.completionTimeout)
            while !Task.isCancelled, Date() < deadline {
                if !self.closing, token == self.navigationToken, let webView = self.webView,
                   AccountLoginValidation.isExactCompletionOrigin(webView.currentURL) {
                    self.evaluateCompletion(token: token, deadline: deadline)
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
            if !Task.isCancelled, token == self.navigationToken, !self.closing,
               !self.completionDelivered {
                self.cancel()
            }
        }
    }

    private func evaluateCompletion(token: UInt64, deadline: Date) {
        guard let webView else { return }
        webView.evaluate(AccountLoginValidation.completionScript) { json, _ in
            MainActor.assumeIsolated { [weak self] in
                guard let self, let json else { return }
                self.acceptCompletion(json: json, token: token, deadline: deadline)
            }
        }
    }

    private func acceptCompletion(json: String, token: UInt64, deadline: Date) {
        // A script result arrives as JSON, so the string the page returned is a quoted literal.
        guard let payload = try? JSONDecoder().decode(String.self, from: Data(json.utf8)),
              !payload.isEmpty else { return }
        guard !closing, token == navigationToken, !completionDelivered,
              let webView, let accountId, let profileId,
              AccountLoginValidation.isExactCompletionOrigin(webView.currentURL) else { return }
        let data = Data(payload.utf8)
        guard let summary = AccountLoginValidation.sanitizeMetadata(data),
              AccountLoginPollingDecision.decide(
                token: token, activeToken: navigationToken, now: Date(), deadline: deadline,
                exactOrigin: true, summary: summary
              ) == .accept,
              let result = AccountLoginValidation.makeResult(
                accountId: accountId, profileId: profileId,
                metadata: data, pageURL: webView.currentURL
              ) else { return }
        completionDelivered = true
        awaitingPromotion = true
        close()
        onCompleted?(result, self)
    }

    // MARK: - Window lifetime

    /// The user closing the window is a cancellation, and it has to reach the model: Rust is
    /// holding nothing yet, but the shell is waiting for an answer.
    private static func observeClose(of window: Gtk.Window, host: AccountLoginHost) {
        g_signal_connect_data(
            UnsafeMutableRawPointer(window.widgetPointer),
            "close-request",
            unsafeBitCast(closeRequested, to: GCallback.self),
            Unmanaged.passUnretained(host).toOpaque(),
            nil, GConnectFlags(rawValue: 0)
        )
    }

    private static let closeRequested: @convention(c) (
        UnsafeMutableRawPointer?, UnsafeMutableRawPointer?
    ) -> gboolean = { _, userData in
        guard let userData else { return gboolean(0) }
        let host = Unmanaged<AccountLoginHost>.fromOpaque(userData).takeUnretainedValue()
        MainActor.assumeIsolated {
            guard !host.closing else { return }
            host.cancel()
        }
        // Let GTK carry on and destroy the window; `close()` has already torn down the renderer.
        return gboolean(0)
    }
}
#endif
