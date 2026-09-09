#if os(macOS)
import Foundation
import WebKit

/// Reads account-scoped public catalog metadata inside the active account's WebKit profile.
/// Cookies remain in WebKit: the evaluated program returns only the same normalized titles,
/// identifiers, artwork URLs, and durations carried by the anonymous catalog protocol.
///
/// The program is `Resources/PersonalCatalog.js`, the InnerTube client and shelf parsers
/// ported from the previous Goosic (GPL-3.0). It runs as the body of an async function so it
/// can await the same-origin fetch; the session cookie it hashes for authorization is read
/// from the document and never leaves it.
///
/// One page serves every request for an account. Loading YouTube Music's application takes
/// tens of seconds in an off-screen view, so it is loaded once when the account is bound and
/// kept; each read is then a single fetch from that page. Requests made before the page is
/// ready wait for it rather than starting a page of their own.
@MainActor
final class PersonalCatalogHost: NSObject, WKNavigationDelegate {
    private struct Request {
        let id: UUID
        let browseID: String
        let title: String
        let continuation: String?
        let submittedAt: Date
        let completion: (Result<GoosicCatalogPage, Error>) -> Void
    }

    private static let program: String? = {
        guard let url = Bundle.module.url(forResource: "PersonalCatalog", withExtension: "js") else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }()

    /// How long a request may wait, page load included, before it fails instead of leaving a
    /// screen on "Loading…".
    private static let requestTimeout: TimeInterval = 45

    private var profileIdentifier: UUID?
    private var webView: WKWebView?
    private var pageReady = false
    private var waiting: [Request] = []
    private var running: [UUID: Request] = [:]

    /// Diagnostics go to stderr, never to the protocol: browse ids, origins, byte counts. See
    /// `Diagnostics` for why a URL never appears whole.
    private func note(_ event: String, _ fields: [String: String] = [:]) {
        Diagnostics.note(.personalCatalog, event, fields)
    }

    func bind(profileIdentifier: UUID?) {
        guard self.profileIdentifier != profileIdentifier else { return }
        note("bind", ["signedIn": "\(profileIdentifier != nil)"])
        self.profileIdentifier = profileIdentifier
        failAll(with: PersonalCatalogError.accountChanged)
        destroyPage()
        // Warm the page now so the first Library or Home read after sign-in does not pay for
        // the application load.
        if profileIdentifier != nil { ensurePage() }
    }

    func load(
        section: PersonalLibrarySection,
        continuation: String? = nil,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        loadBrowse(
            browseID: section.browseID,
            title: section.rawValue,
            continuation: continuation,
            completion: completion
        )
    }

    func loadBrowse(
        browseID: String,
        title: String,
        continuation: String? = nil,
        completion: @escaping (Result<GoosicCatalogPage, Error>) -> Void
    ) {
        guard profileIdentifier != nil else {
            completion(.failure(PersonalCatalogError.signedOut))
            return
        }
        let request = Request(
            id: UUID(), browseID: browseID, title: title, continuation: continuation,
            submittedAt: Date(), completion: completion
        )
        note("request", ["browse": browseID, "continuation": "\(continuation != nil)", "pageReady": "\(pageReady)"])
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(Self.requestTimeout))
            guard let self else { return }
            if let index = self.waiting.firstIndex(where: { $0.id == request.id }) {
                let stale = self.waiting.remove(at: index)
                self.note("timeout", ["browse": stale.browseID, "phase": "waiting-for-page"])
                stale.completion(.failure(PersonalCatalogError.timedOut))
                // A page that has not become ready in this long is not going to; start over.
                self.destroyPage()
                if !self.waiting.isEmpty { self.ensurePage() }
            } else if let stale = self.running.removeValue(forKey: request.id) {
                self.note("timeout", ["browse": stale.browseID, "phase": "running"])
                stale.completion(.failure(PersonalCatalogError.timedOut))
            }
        }
        if pageReady, let webView {
            run(request, in: webView)
        } else {
            waiting.append(request)
            ensurePage()
        }
    }

    // MARK: - Page lifecycle

    private func ensurePage() {
        guard webView == nil, let profileIdentifier else { return }
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = WKWebsiteDataStore(forIdentifier: profileIdentifier)
        configuration.applicationNameForUserAgent = "Version/18.5 Safari/605.1.15"
        configuration.mediaTypesRequiringUserActionForPlayback = [.audio, .video]
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        self.webView = webView
        pageReady = false
        note("page-loading")
        webView.load(URLRequest(url: URL(string: "https://music.youtube.com/")!))
    }

    private func destroyPage() {
        webView?.stopLoading()
        webView?.navigationDelegate = nil
        webView = nil
        pageReady = false
    }

    private func failAll(with error: Error) {
        let pending = waiting + Array(running.values)
        waiting.removeAll()
        running.removeAll()
        for request in pending { request.completion(.failure(error)) }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard webView === self.webView else { return }
        note("page-ready", ["origin": Diagnostics.origin(of: webView.url), "queued": "\(waiting.count)"])
        pageReady = true
        let queued = waiting
        waiting.removeAll()
        for request in queued { run(request, in: webView) }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        pageFailed(webView, error: error)
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        pageFailed(webView, error: error)
    }

    private func pageFailed(_ webView: WKWebView, error: Error) {
        guard webView === self.webView else { return }
        let nsError = error as NSError
        if nsError.domain == NSURLErrorDomain, nsError.code == NSURLErrorCancelled { return }
        note("page-failed", ["domain": nsError.domain, "code": "\(nsError.code)"])
        failAll(with: error)
        destroyPage()
    }

    // MARK: - Running a request on the ready page

    private func run(_ request: Request, in webView: WKWebView) {
        guard let program = Self.program else {
            request.completion(.failure(PersonalCatalogError.programMissing))
            return
        }
        running[request.id] = request
        let script = program + "\nreturn await GoosicPersonalCatalog.browse(browseId, title, continuation);"
        let arguments: [String: Any] = [
            "browseId": request.browseID,
            "title": request.title,
            "continuation": request.continuation ?? NSNull(),
        ]
        let id = request.id
        webView.callAsyncJavaScript(script, arguments: arguments, in: nil, in: .page) { [weak self] result in
            let json: String?
            let failure: String?
            switch result {
            case .success(let value):
                json = value as? String
                failure = nil
            case .failure(let error):
                json = nil
                failure = error.localizedDescription
            }
            Task { @MainActor [weak self] in
                guard let self, let request = self.running.removeValue(forKey: id) else { return }
                let elapsed = Int(Date().timeIntervalSince(request.submittedAt) * 1000)
                if let failure {
                    self.note("failed", ["browse": request.browseID, "elapsed": "\(elapsed)ms", "reason": failure])
                    request.completion(.failure(PersonalCatalogError.scriptFailed(failure)))
                    return
                }
                guard let json, let data = json.data(using: .utf8) else {
                    self.note("empty-answer", ["browse": request.browseID, "elapsed": "\(elapsed)ms"])
                    request.completion(.failure(PersonalCatalogError.invalidResponse))
                    return
                }
                self.note("answered", ["browse": request.browseID, "bytes": "\(data.count)", "elapsed": "\(elapsed)ms"])
                do {
                    request.completion(.success(try JSONDecoder().decode(GoosicCatalogPage.self, from: data)))
                } catch {
                    request.completion(.failure(error))
                }
            }
        }
    }
}

private enum PersonalCatalogError: LocalizedError {
    case signedOut
    case accountChanged
    case invalidResponse
    case scriptFailed(String)
    case programMissing
    case timedOut

    var errorDescription: String? {
        switch self {
        case .signedOut: return "Sign in to load your personal library."
        case .accountChanged: return "The active account changed while the library was loading."
        case .invalidResponse: return "YouTube Music returned an unreadable personal library."
        case .scriptFailed(let message): return message
        case .programMissing: return "The personal catalog program is missing from the app bundle."
        case .timedOut: return "YouTube Music did not answer in time. Try again."
        }
    }
}
#endif
