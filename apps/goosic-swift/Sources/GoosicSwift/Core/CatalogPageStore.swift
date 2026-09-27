import Foundation

/// The last copy of each page the shell showed, kept on disk per account profile.
///
/// Measured, every visit fetched its page afresh: from a fifth of a second to over a second, and
/// seconds more whenever the account's reader page had to start. A page shown from its last copy
/// opens at once, and the fresh answer replaces it behind the listener only if it differs. The
/// Windows shell does the same in `Service/PageCache.cs`.
///
/// What is kept is catalog metadata and nothing else: titles, ids, artwork URLs. The continuation
/// cursor is left out, because it changes on every answer and a stale one would continue the
/// wrong list; the refresh that follows every copy brings a current one. Searches are not kept.
/// A profile's copies are deleted when it signs out.
struct CatalogPageStore {
    private let root: URL?

    init(root: URL? = nil) {
        self.root = root ?? FileManager.default
            .urls(for: .cachesDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Goosic", isDirectory: true)
            .appendingPathComponent("pages", isDirectory: true)
    }

    /// The file name a page is kept under, or `nil` for pages that are not kept.
    static func name(for key: CatalogKey) -> String? {
        let raw: String
        switch key {
        case .route(let route): raw = "route-\(route.rawValue)"
        case .search: return nil
        case .album(let id): raw = "album-\(id)"
        case .artist(let id): raw = "artist-\(id)"
        case .playlist(let id): raw = "playlist-\(id)"
        case .category(let id): raw = "category-\(id)"
        case .library(let section): raw = "library-\(section)"
        }
        // Ids are upstream text; anything but letters and digits is escaped so no id can name a
        // path outside the store.
        return raw.addingPercentEncoding(withAllowedCharacters: .alphanumerics)
    }

    private func directory(scope: String) -> URL? {
        guard let root, let safe = scope.addingPercentEncoding(withAllowedCharacters: .alphanumerics) else {
            return nil
        }
        return root.appendingPathComponent(safe, isDirectory: true)
    }

    private func file(for key: CatalogKey, scope: String) -> URL? {
        guard let name = Self.name(for: key), let directory = directory(scope: scope) else { return nil }
        return directory.appendingPathComponent(name + ".json")
    }

    func load(_ key: CatalogKey, scope: String) -> GoosicCatalogPage? {
        guard let url = file(for: key, scope: scope), let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(GoosicCatalogPage.self, from: data)
    }

    func save(_ page: GoosicCatalogPage, for key: CatalogKey, scope: String) {
        guard let url = file(for: key, scope: scope) else { return }
        var copy = page
        copy.nextCursor = nil
        guard let data = try? JSONEncoder().encode(copy) else { return }
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        try? data.write(to: url, options: .atomic)
    }

    func removeScope(_ scope: String) {
        guard let directory = directory(scope: scope) else { return }
        try? FileManager.default.removeItem(at: directory)
    }
}
