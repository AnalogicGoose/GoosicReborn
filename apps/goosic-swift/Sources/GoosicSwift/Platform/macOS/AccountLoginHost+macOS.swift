#if os(macOS)
import Foundation

import AppKit
import WebKit

@MainActor
final class AccountLoginHost: NSObject, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate {
    nonisolated private static let allowedHosts: Set<String> = [
        "accounts.google.com", "www.youtube.com", "music.youtube.com",
        "myaccount.google.com", "consent.google.com", "ogs.google.com"
    ]

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
    private let completionTimeout: TimeInterval = 30
    var onCompleted: ((AccountLoginResult, AccountLoginHost) -> Void)?
    var onCancelled: (() -> Void)?

    func start() {
        guard window == nil else { return }
        // Both UUIDs are generated before the login surface opens and are never derived from
        // provider data. They are stable for this staged login and distinct by construction.
        let accountId = UUID()
        var profileId = UUID()
        while profileId == accountId { profileId = UUID() }
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
        window.contentView = webView
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
        webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
    }

    func close() {
        guard !closing else { return }
        closing = true
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
        guard let store = stagingStore ?? webView?.configuration.websiteDataStore else { return }
        store.removeData(ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(), modifiedSince: Date(timeIntervalSince1970: 0)) { }
    }

    func windowWillClose(_ notification: Notification) {
        guard !closing else { return }
        cancel()
    }

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard navigationAction.targetFrame?.isMainFrame == true else { return decisionHandler(.cancel) }
        decisionHandler(Self.isAllowedMainFrameURL(navigationAction.request.url) ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        startCompletionPolling(for: webView)
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        cancel()
    }

    private func startCompletionPolling(for webView: WKWebView) {
        pollingTask?.cancel()
        navigationToken &+= 1
        let token = navigationToken
        pollingTask = Task { @MainActor [weak self, weak webView] in
            guard let self else { return }
            let deadline = Date().addingTimeInterval(self.completionTimeout)
            while !Task.isCancelled, Date() < deadline {
                guard !self.closing, token == self.navigationToken, let webView,
                      AccountLoginValidation.isExactCompletionOrigin(webView.url) else {
                    try? await Task.sleep(for: .milliseconds(250))
                    continue
                }
                webView.evaluateJavaScript(Self.completionScript) { [weak self, weak webView] value, _ in
                    Task { @MainActor [weak self, weak webView] in
                        guard let self, let webView, !self.closing,
                              token == self.navigationToken,
                              AccountLoginValidation.isExactCompletionOrigin(webView.url),
                              let value = value as? String, let data = value.data(using: .utf8),
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
                self.cancel()
            }
        }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        nil
    }

    nonisolated static func isAllowedMainFrameURL(_ url: URL?) -> Bool {
        guard let url else { return false }
        guard url.scheme == "https", url.user == nil, url.password == nil,
              url.host != nil, url.port == nil || url.port == 443 else { return false }
        return allowedHosts.contains(url.host!.lowercased())
    }

    /// Runs in the page only after exact-origin navigation. The marker requires an account-menu
    /// affordance in the authenticated shell; arriving at music.youtube.com alone is insufficient.
    private static let completionScript = """
    (() => {
      if (location.origin !== 'https://music.youtube.com') return '';
      const marker = document.querySelector('#avatar-btn, button[aria-label*="Account"], [aria-label*="Google Account"]');
      if (!marker) return '';
      const clip = (value, limit) => (value || '').trim().slice(0, limit);
      const text = (selector, limit) => clip(document.querySelector(selector)?.textContent, limit);
      const attr = (selector, name, limit) => clip(document.querySelector(selector)?.getAttribute(name), limit);
      const visible = (element) => {
        if (!element) return false;
        const style = getComputedStyle(element);
        const rect = element.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
      };
      const signInVisible = Array.from(document.querySelectorAll('a,button,[role="button"]'))
        .some((element) => visible(element) && /sign\\s*in|log\\s*in/i.test(element.textContent || element.getAttribute('aria-label') || ''));
      if (signInVisible) return '';
      const identity = clip(marker.getAttribute('aria-label') || marker.getAttribute('title') || '', 320);
      const signedOut = /sign\\s*in|log\\s*in|not\\s*signed/i.test(identity);
      const email = text('#account-email', 320);
      const channel = text('ytmusic-account-menu-renderer #channel-title', 128);
      const avatarUrl = attr('#avatar-btn img', 'src', 2048);
      if (signedOut) return '';
      if (!identity && !email && !channel) {
        try { marker.click(); } catch (_) {}
        return '';
      }
      return JSON.stringify({
        displayName: text('#account-name', 128) || clip(identity, 128),
        email, channel, avatarUrl
      });
    })();
    """
}
#endif
