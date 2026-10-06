#if os(macOS) && !GOOSIC_PREVIEW_NO_WEBKIT
import Foundation

import AppKit
import WebKit

@MainActor
final class AccountLoginHost: NSObject, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate {
    private var window: NSWindow?
    private var webView: WKWebView?
    private var accountId: UUID?
    private var profileId: UUID?
    private var stagingStore: WKWebsiteDataStore?
    private var promotionCommitted = false
    private var awaitingPromotion = false
    private var closing = false
    private var completionDelivered = false
    private var navigationToken: UInt64 = 0
    private var pollingTask: Task<Void, Never>?
    private var completionProbeInFlight = false
    private var navigationWatchdog: Task<Void, Never>?
    private var navigationLoadID: UInt64 = 0
    private var statusLabel: NSTextField?
    private var signInURL: URL?
    var onCompleted: ((AccountLoginResult, AccountLoginHost) -> Void)?
    var onCancelled: (() -> Void)?

    /// Diagnostics for the staged sign-in go to stderr, never to the protocol. Nothing here is
    /// a credential: origins, decisions, and the length of the metadata projection only. A
    /// sign-in URL in particular is never logged whole — see `Diagnostics`.
    private func note(_ event: String, _ fields: [String: String] = [:]) {
        Diagnostics.note(.accountLogin, event, fields)
    }

    /// Set when signing an existing account in again: the profile is the account's own, not a
    /// staging store, so a cancelled or failed attempt must leave it exactly as it was.
    private var reusingProfile = false

    func start() {
        // Both UUIDs are generated before the login surface opens and are never derived from
        // provider data. They are stable for this staged login and distinct by construction.
        let accountId = UUID()
        var profileId = UUID()
        while profileId == accountId { profileId = UUID() }
        open(accountId: accountId, profileId: profileId)
    }

    /// Signs an account whose session ended in again, into its own profile. Nothing is staged:
    /// the account and its profile already exist, and only their cookies are renewed.
    func start(reusing accountId: UUID, profileId: UUID) {
        reusingProfile = true
        open(accountId: accountId, profileId: profileId)
    }

    private func open(accountId: UUID, profileId: UUID) {
        guard window == nil else { return }
        self.accountId = accountId
        self.profileId = profileId

        let configuration = WKWebViewConfiguration()
        let store = WKWebsiteDataStore(forIdentifier: profileId)
        configuration.websiteDataStore = store
        stagingStore = store
        configuration.applicationNameForUserAgent = "Version/18.5 Safari/605.1.15"
        configuration.mediaTypesRequiringUserActionForPlayback = [.audio, .video]
        // Deliberately no script message handler: login is a browser surface, not a playback
        // bridge. Native code only evaluates a bounded metadata projection at exact completion.
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        self.webView = webView

        let frame = NSRect(x: 0, y: 0, width: 720, height: 640)
        let window = NSWindow(contentRect: frame, styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Sign in to YouTube Music"
        let content = NSView()
        let status = NSTextField(labelWithString: "Loading sign-in page…")
        status.textColor = .secondaryLabelColor
        status.font = .systemFont(ofSize: 12)
        status.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        statusLabel = status
        let retry = NSButton(title: "Try Again", target: self, action: #selector(retrySignIn))
        let footer = NSStackView(views: [status, retry])
        footer.orientation = .horizontal
        footer.spacing = 12
        for view in [webView, footer] {
            view.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(view)
        }
        NSLayoutConstraint.activate([
            webView.topAnchor.constraint(equalTo: content.topAnchor),
            webView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            webView.bottomAnchor.constraint(equalTo: footer.topAnchor, constant: -8),
            footer.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 12),
            footer.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -12),
            footer.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -8),
        ])
        window.contentView = content
        window.delegate = self
        window.isReleasedWhenClosed = false
        self.window = window
        window.center()
        window.makeKeyAndOrderFront(nil)

        var components = URLComponents()
        components.scheme = "https"
        components.host = "accounts.google.com"
        components.path = "/ServiceLogin"
        components.queryItems = [URLQueryItem(name: "continue", value: "https://music.youtube.com")]
        guard let url = components.url else { return cancel() }
        signInURL = url
        webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
    }

    func close() {
        guard !closing else { return }
        closing = true
        navigationWatchdog?.cancel()
        navigationWatchdog = nil
        pollingTask?.cancel()
        pollingTask = nil
        navigationToken &+= 1
        webView?.stopLoading()
        webView?.navigationDelegate = nil
        webView?.uiDelegate = nil
        window?.delegate = nil
        window?.close()
        if !awaitingPromotion && !promotionCommitted { discardStaging() }
        webView = nil
        window = nil
    }

    /// Rust calls this only after accounts.upsert and accounts.activate both succeed. Until then
    /// the profile remains a staged store and can be deleted on any failure.
    func commitPromotion() {
        guard awaitingPromotion else { return }
        promotionCommitted = true
        awaitingPromotion = false
        stagingStore = nil
    }

    func discardStaging() {
        guard !promotionCommitted else { return }
        awaitingPromotion = false
        deleteStagingStore()
        stagingStore = nil
    }

    private func cancel() {
        close()
        onCancelled?()
    }

    private func deleteStagingStore() {
        guard !reusingProfile else { return }
        guard let store = stagingStore ?? webView?.configuration.websiteDataStore else { return }
        store.removeData(ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(), modifiedSince: Date(timeIntervalSince1970: 0)) { }
    }

    func windowWillClose(_ notification: Notification) {
        guard !closing else { return }
        cancel()
    }

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
        let decision = LoginNavigationPolicy.decide(
            url: navigationAction.request.url,
            frame: NavigationFrame(navigationAction.targetFrame)
        )
        if decision == .cancel {
            note("navigation-refused", ["origin": Diagnostics.origin(of: navigationAction.request.url), "frame": "\(NavigationFrame(navigationAction.targetFrame))"])
        }
        decisionHandler(decision == .allow ? .allow : .cancel)
    }

    @objc private func retrySignIn() {
        guard !closing, !completionDelivered, let webView, let signInURL else { return }
        note("retry")
        pollingTask?.cancel()
        pollingTask = nil
        navigationToken &+= 1
        completionProbeInFlight = false
        webView.stopLoading()
        webView.load(URLRequest(url: signInURL, cachePolicy: .reloadIgnoringLocalCacheData))
    }

    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
        guard webView === self.webView else { return }
        navigationWatchdog?.cancel()
        navigationLoadID &+= 1
        let loadID = navigationLoadID
        statusLabel?.stringValue = "Loading sign-in page…"
        navigationWatchdog = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(30))
            guard !Task.isCancelled, let self, !self.closing, self.navigationLoadID == loadID else { return }
            self.note("navigation-stalled")
            self.statusLabel?.stringValue = "The page is taking too long. Click Try Again to reload it."
        }
    }

    func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
        if AccountLoginValidation.isExactCompletionOrigin(webView.url) {
            statusLabel?.stringValue = "Finishing sign-in…"
            startCompletionPolling(for: webView)
        }
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        guard webView === self.webView else { return }
        note("page-process-terminated")
        navigationWatchdog?.cancel()
        pollingTask?.cancel()
        pollingTask = nil
        navigationToken &+= 1
        completionProbeInFlight = false
        statusLabel?.stringValue = "The sign-in page stopped. Click Try Again to reload it."
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        navigationWatchdog?.cancel()
        statusLabel?.stringValue = AccountLoginValidation.isExactCompletionOrigin(webView.url)
            ? "Finishing sign-in…" : "Complete your Google sign-in below."
        note("navigation-finished", ["origin": Diagnostics.origin(of: webView.url), "atCompletionOrigin": "\(AccountLoginValidation.isExactCompletionOrigin(webView.url))"])
        if pollingTask == nil { startCompletionPolling(for: webView) }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        let failure = error as NSError
        // A load superseded by the next redirect in Google's sign-in chain is not a failure.
        if failure.domain == NSURLErrorDomain, failure.code == NSURLErrorCancelled {
            note("navigation-superseded", ["origin": Diagnostics.origin(of: webView.url)])
            return
        }
        note("navigation-failed", ["domain": failure.domain, "code": "\(failure.code)", "origin": Diagnostics.origin(of: webView.url)])
        navigationWatchdog?.cancel()
        pollingTask?.cancel()
        pollingTask = nil
        navigationToken &+= 1
        completionProbeInFlight = false
        statusLabel?.stringValue = failure.code == NSURLErrorNotConnectedToInternet
            ? "You’re offline. Reconnect, then click Try Again."
            : "Could not load the sign-in page. Click Try Again to reload it."
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
                 withError error: Error) {
        self.webView(webView, didFail: navigation, withError: error)
    }

    private func startCompletionPolling(for webView: WKWebView) {
        pollingTask?.cancel()
        navigationToken &+= 1
        let token = navigationToken
        completionProbeInFlight = false
        pollingTask = Task { @MainActor [weak self, weak webView] in
            guard let self else { return }
            let deadline = Date().addingTimeInterval(AccountLoginValidation.completionTimeout)
            while !Task.isCancelled, Date() < deadline {
                guard !self.closing, token == self.navigationToken, let webView,
                      AccountLoginValidation.isExactCompletionOrigin(webView.url) else {
                    try? await Task.sleep(for: .milliseconds(250))
                    continue
                }
                if self.completionProbeInFlight {
                    try? await Task.sleep(for: .milliseconds(250))
                    continue
                }
                self.completionProbeInFlight = true
                webView.callAsyncJavaScript(AccountLoginValidation.completionScript, arguments: [:], in: nil, in: .page) { [weak self, weak webView] result in
                    var failure: String?
                    var value: String?
                    switch result {
                    case .success(let any): value = any as? String
                    case .failure(let error): failure = error.localizedDescription
                    }
                    Task { @MainActor [weak self, weak webView] in
                        guard let self, !self.closing, token == self.navigationToken else { return }
                        self.completionProbeInFlight = false
                        if failure != nil { self.note("completion-script-failed") }
                        else if let text = value {
                            let summary = text.data(using: .utf8).flatMap(AccountLoginValidation.sanitizeMetadata)
                            self.note("completion-probe", ["marker": text.isEmpty ? "absent" : "\(text.utf8.count) bytes", "decision": summary.map { LoginCompletionDecision.from($0) == .wait ? "wait" : "accept" } ?? "wait"])
                        }
                        guard let webView, !self.closing,
                              token == self.navigationToken,
                              AccountLoginValidation.isExactCompletionOrigin(webView.url),
                              let value, let data = value.data(using: .utf8),
                              let accountId = self.accountId, let profileId = self.profileId,
                              let summary = AccountLoginValidation.sanitizeMetadata(data),
                              AccountLoginPollingDecision.decide(token: token, activeToken: self.navigationToken,
                                                                 now: Date(), deadline: deadline, exactOrigin: true,
                                                                 summary: summary) == .accept,
                              let result = AccountLoginValidation.makeResult(accountId: accountId, profileId: profileId, metadata: data, pageURL: webView.url),
                              !self.completionDelivered else { return }
                        self.completionDelivered = true
                        self.awaitingPromotion = true
                        self.close()
                        self.onCompleted?(result, self)
                    }
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
            if !Task.isCancelled, token == self.navigationToken, !self.closing {
                self.note("timeout", ["after": "\(Int(AccountLoginValidation.completionTimeout))s", "origin": Diagnostics.origin(of: self.webView?.url)])
                self.statusLabel?.stringValue = "Could not confirm sign-in. Click Try Again, or close this window."
            }
        }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        nil
    }

}
#endif
