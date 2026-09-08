#if os(macOS)
import Foundation
import WebKit

/// Reads account-scoped public catalog metadata inside the active account's WebKit profile.
/// Cookies remain in WebKit: the evaluated program returns only the same normalized titles,
/// identifiers, artwork URLs, and durations carried by the anonymous catalog protocol.
@MainActor
final class PersonalCatalogHost: NSObject, WKNavigationDelegate {
    private struct Request {
        let webView: WKWebView
        let browseID: String
        let title: String
        let continuation: String?
        let completion: (Result<GoosicCatalogPage, Error>) -> Void
    }

    private var profileIdentifier: UUID?
    private var requests: [ObjectIdentifier: Request] = [:]

    func bind(profileIdentifier: UUID?) {
        guard self.profileIdentifier != profileIdentifier else { return }
        self.profileIdentifier = profileIdentifier
        let pending = requests.values
        requests.removeAll()
        for request in pending {
            request.webView.stopLoading()
            request.webView.navigationDelegate = nil
            request.completion(.failure(PersonalCatalogError.accountChanged))
        }
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
        guard let profileIdentifier else {
            completion(.failure(PersonalCatalogError.signedOut))
            return
        }

        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = WKWebsiteDataStore(forIdentifier: profileIdentifier)
        configuration.applicationNameForUserAgent = "Version/18.5 Safari/605.1.15"
        configuration.mediaTypesRequiringUserActionForPlayback = [.audio, .video]
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        requests[ObjectIdentifier(webView)] = Request(
            webView: webView,
            browseID: browseID,
            title: title,
            continuation: continuation,
            completion: completion
        )
        webView.load(URLRequest(url: URL(string: "https://music.youtube.com/")!))
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        let key = ObjectIdentifier(webView)
        guard let request = requests[key] else { return }
        let script = Self.script(
            browseID: request.browseID,
            title: request.title,
            continuation: request.continuation
        )
        webView.evaluateJavaScript(script) { [weak self, weak webView] value, error in
            Task { @MainActor [weak self, weak webView] in
                guard let self, let webView,
                      let request = self.requests.removeValue(forKey: ObjectIdentifier(webView)) else { return }
                webView.navigationDelegate = nil
                if let error {
                    request.completion(.failure(error))
                    return
                }
                guard let json = value as? String, let data = json.data(using: .utf8) else {
                    request.completion(.failure(PersonalCatalogError.invalidResponse))
                    return
                }
                do {
                    request.completion(.success(try JSONDecoder().decode(GoosicCatalogPage.self, from: data)))
                } catch {
                    request.completion(.failure(error))
                }
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        finish(webView, result: .failure(error))
    }

    func webView(
        _ webView: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        finish(webView, result: .failure(error))
    }

    private func finish(_ webView: WKWebView, result: Result<GoosicCatalogPage, Error>) {
        guard let request = requests.removeValue(forKey: ObjectIdentifier(webView)) else { return }
        webView.navigationDelegate = nil
        request.completion(result)
    }

    private static func script(browseID: String, title: String, continuation: String?) -> String {
        let browse = javascriptString(browseID)
        let pageTitle = javascriptString(title)
        let cursor = continuation.map(javascriptString) ?? "null"
        return """
        (async () => {
          const walk = (value, key, out = []) => {
            if (!value || typeof value !== 'object') return out;
            if (Object.prototype.hasOwnProperty.call(value, key)) out.push(value[key]);
            for (const child of Object.values(value)) walk(child, key, out);
            return out;
          };
          const text = value => {
            const runs = value?.runs;
            if (Array.isArray(runs)) return runs.map(run => run?.text || '').join('');
            return value?.simpleText || '';
          };
          const largestThumbnail = node => {
            const all = walk(node, 'thumbnails').flat().filter(item => item?.url);
            all.sort((a, b) => ((a.width || 0) * (a.height || 0)) - ((b.width || 0) * (b.height || 0)));
            return all.at(-1)?.url || null;
          };
          const destination = endpoint => {
            const browse = endpoint?.browseEndpoint;
            if (!browse?.browseId) return null;
            const pageType = browse?.browseEndpointContextSupportedConfigs?.browseEndpointContextMusicConfig?.pageType || '';
            let kind = pageType.includes('ARTIST') || pageType.includes('USER_CHANNEL') ? 'artist'
              : pageType.includes('ALBUM') ? 'album'
              : pageType.includes('PLAYLIST') ? 'playlist' : null;
            return { id: browse.browseId, kind };
          };
          const references = nodes => {
            let artist = null, album = null;
            for (const node of nodes) for (const run of node?.runs || []) {
              const target = destination(run?.navigationEndpoint);
              if (target?.kind === 'artist' && !artist) artist = { name: run.text || '', id: target.id };
              if (target?.kind === 'album' && !album) album = { name: run.text || '', id: target.id };
            }
            return { artist, album };
          };
          const responsive = row => {
            const flex = (row?.flexColumns || []).map(column => column?.musicResponsiveListItemFlexColumnRenderer?.text).filter(Boolean);
            const fixed = (row?.fixedColumns || []).map(column => column?.musicResponsiveListItemFixedColumnRenderer?.text).filter(Boolean);
            const title = text(flex[0]);
            if (!title) return null;
            const endpoint = flex[0]?.runs?.[0]?.navigationEndpoint || row?.navigationEndpoint;
            const watch = walk(row, 'watchEndpoint').find(item => item?.videoId);
            const target = destination(endpoint);
            const refs = references(flex.slice(1));
            const descriptor = flex.slice(1).map(text).find(Boolean) || '';
            const duration = [...fixed, ...flex.slice(1)].map(text).find(value => /^\\d{1,2}:\\d{2}(?::\\d{2})?$/.test(value)) || null;
            let kind = watch?.videoId ? 'song' : target?.kind;
            let id = watch?.videoId || target?.id;
            if (!kind || !id) return null;
            return { kind, id, title, subtitle: descriptor, artist: refs.artist?.name || null,
              artistId: refs.artist?.id || null, album: refs.album?.name || null,
              albumId: refs.album?.id || null, duration, thumbnail: largestThumbnail(row),
              videoId: watch?.videoId || null, explicit: walk(row, 'iconType').includes('MUSIC_EXPLICIT_BADGE') };
          };
          const twoRow = row => {
            const title = text(row?.title);
            if (!title) return null;
            const endpoint = row?.navigationEndpoint || row?.title?.runs?.[0]?.navigationEndpoint;
            const watch = endpoint?.watchEndpoint;
            const target = destination(endpoint);
            const playlistId = endpoint?.watchPlaylistEndpoint?.playlistId || watch?.playlistId;
            const kind = watch?.videoId ? 'song' : (target?.kind || (playlistId ? 'playlist' : null));
            const id = watch?.videoId || target?.id || playlistId;
            if (!kind || !id) return null;
            const refs = references([row?.subtitle]);
            return { kind, id, title, subtitle: text(row?.subtitle), artist: refs.artist?.name || null,
              artistId: refs.artist?.id || null, album: null, albumId: null, duration: null,
              thumbnail: largestThumbnail(row), videoId: watch?.videoId || null, explicit: false };
          };
          const context = window.ytcfg?.get?.('INNERTUBE_CONTEXT') || window.ytcfg?.data_?.INNERTUBE_CONTEXT;
          const key = window.ytcfg?.get?.('INNERTUBE_API_KEY') || window.ytcfg?.data_?.INNERTUBE_API_KEY;
          if (!context || !key) throw new Error('YouTube Music session context is unavailable');
          const response = await fetch('/youtubei/v1/browse?prettyPrint=false&key=' + encodeURIComponent(key), {
            method: 'POST', credentials: 'include', headers: {'content-type': 'application/json'},
            body: JSON.stringify(\(cursor) ? {context, continuation: \(cursor)} : {context, browseId: \(browse)})
          });
          if (!response.ok) throw new Error('YouTube Music returned HTTP ' + response.status);
          const root = await response.json();
          const tracks = walk(root, 'musicResponsiveListItemRenderer').map(responsive).filter(Boolean);
          const shelves = [];
          for (const carousel of walk(root, 'musicCarouselShelfRenderer')) {
            const items = [
              ...walk(carousel, 'musicTwoRowItemRenderer').map(twoRow),
              ...walk(carousel, 'musicResponsiveListItemRenderer').map(responsive)
            ].filter(Boolean);
            if (items.length) shelves.push({id: 'library-' + shelves.length, title: text(walk(carousel?.header, 'title')[0]) || \(pageTitle), items});
          }
          for (const shelf of walk(root, 'musicShelfRenderer')) {
            const items = walk(shelf, 'musicResponsiveListItemRenderer').map(responsive).filter(Boolean);
            if (items.length) shelves.push({id: 'library-' + shelves.length, title: text(shelf?.title) || \(pageTitle), items});
          }
          if (!shelves.length) {
            const cards = walk(root, 'musicTwoRowItemRenderer').map(twoRow).filter(Boolean);
            if (cards.length) shelves.push({id: 'library-0', title: \(pageTitle), items: cards});
          }
          const cursor = walk(root, 'continuationItemRenderer').map(item => item?.continuationEndpoint?.continuationCommand?.token).find(Boolean) || null;
          return JSON.stringify({id: 'library:' + \(browse), title: \(pageTitle), subtitle: '', shelves,
            tracks: \(pageTitle) === 'Songs' && !shelves.length ? tracks : [], thumbnail: null, nextCursor: cursor, truncated: false});
        })()
        """
    }

    private static func javascriptString(_ value: String) -> String {
        let data = try! JSONEncoder().encode(value)
        return String(decoding: data, as: UTF8.self)
    }
}

private enum PersonalCatalogError: LocalizedError {
    case signedOut
    case accountChanged
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .signedOut: return "Sign in to load your personal library."
        case .accountChanged: return "The active account changed while the library was loading."
        case .invalidResponse: return "YouTube Music returned an unreadable personal library."
        }
    }
}
#endif
