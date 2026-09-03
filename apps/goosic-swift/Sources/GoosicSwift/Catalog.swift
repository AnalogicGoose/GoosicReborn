import Foundation

/// Identifies one catalog page the shell has asked Rust for.
///
/// Pages are cached under this key, so a slow response can never land on the screen the user
/// has since navigated away from — it lands on its own key and is simply not displayed.
enum CatalogKey: Hashable {
    case route(GoosicRoute)
    case search(query: String, filter: String)
    case album(String)
    case artist(String)
    case playlist(String)

    static func entity(_ reference: GoosicEntityReference) -> CatalogKey {
        switch reference {
        case .album(let id): return .album(id)
        case .artist(let id): return .artist(id)
        case .playlist(let id): return .playlist(id)
        }
    }
}

enum CatalogLoadState {
    case idle
    case loading
    case loaded(CatalogPageView)
    case failed(code: String, message: String)
}

/// A catalog page in the shape the screens render.
struct CatalogPageView: Hashable {
    let id: String
    let title: String
    let subtitle: String
    let shelves: [GoosicShelf]
    let tracks: [GoosicTrack]
    /// The service clamped this page to fit one protocol frame.
    let truncated: Bool

    var isEmpty: Bool { shelves.isEmpty && tracks.isEmpty }

    /// Every playable row on the page, in display order, for queueing.
    var playableTracks: [GoosicTrack] {
        tracks + shelves.flatMap { shelf in
            shelf.cards.compactMap { card in
                if case .play(let track) = card.action { return track }
                return nil
            }
        }
    }
}

extension GoosicTrack {
    /// Builds a track from a catalog row, or `nil` when the row is not directly playable.
    init?(catalog item: GoosicCatalogItem) {
        guard let videoID = item.videoId, !videoID.isEmpty else { return nil }
        self.init(
            id: videoID,
            title: item.title,
            subtitle: item.subtitle,
            artist: item.artist ?? "",
            artistID: item.artistId,
            album: item.album ?? "",
            albumID: item.albumId,
            duration: item.duration ?? "",
            videoID: videoID,
            explicit: item.explicit ?? false
        )
    }
}

extension GoosicCard {
    init(catalog item: GoosicCatalogItem) {
        let action: GoosicCardAction?
        switch item.kind {
        case .song, .video:
            action = GoosicTrack(catalog: item).map(GoosicCardAction.play)
        case .album:
            action = .show(.album(item.id))
        case .artist:
            action = .show(.artist(item.id))
        case .playlist:
            action = .show(.playlist(item.id))
        case .unknown:
            // A row this build does not understand stays visible but inert rather than
            // navigating somewhere the shell cannot render.
            action = nil
        }
        self.init(id: item.id, title: item.title, subtitle: item.subtitle, action: action)
    }
}

extension CatalogPageView {
    init(wire page: GoosicCatalogPage) {
        var seenShelfIDs = Set<String>()
        let shelves: [GoosicShelf] = (page.shelves ?? []).enumerated().map { index, shelf in
            // Upstream ids are not guaranteed unique within a page, and `ForEach` needs them to
            // be, so collisions are disambiguated by position rather than silently merged.
            var id = shelf.id
            if !seenShelfIDs.insert(id).inserted {
                id = "\(shelf.id)-\(index)"
            }
            return GoosicShelf(
                id: id,
                title: shelf.title,
                cards: GoosicShelf.uniqued((shelf.items).map(GoosicCard.init(catalog:)))
            )
        }
        self.init(
            id: page.id,
            title: page.title,
            subtitle: page.subtitle ?? "",
            shelves: shelves,
            tracks: (page.tracks ?? []).compactMap(GoosicTrack.init(catalog:)),
            truncated: page.truncated ?? false
        )
    }
}

extension GoosicShelf {
    /// The shelf as an ordered track list, when every row in it is playable.
    ///
    /// Search returns songs and albums in the same page shape; songs read far better as rows
    /// than as artwork cards, so the shelf decides its own presentation.
    var trackList: [GoosicTrack]? {
        let tracks = cards.compactMap { card -> GoosicTrack? in
            if case .play(let track) = card.action { return track }
            return nil
        }
        return tracks.count == cards.count && !tracks.isEmpty ? tracks : nil
    }

    /// Keeps the first card for each id. A search page can legitimately return the same album
    /// twice, and duplicate `ForEach` ids render unpredictably.
    static func uniqued(_ cards: [GoosicCard]) -> [GoosicCard] {
        var seen = Set<String>()
        return cards.filter { seen.insert($0.id).inserted }
    }
}

/// The search filter tabs the shell offers, and their protocol names.
enum CatalogSearchFilter: String, CaseIterable {
    case all = "All"
    case songs = "Songs"
    case albums = "Albums"
    case artists = "Artists"
    case playlists = "Playlists"
    case videos = "Videos"

    /// The wire value understood by `catalog.search`.
    var protocolName: String {
        switch self {
        case .all: return "all"
        case .songs: return "songs"
        case .albums: return "albums"
        case .artists: return "artists"
        case .playlists: return "playlists"
        case .videos: return "videos"
        }
    }
}

extension CatalogLoadState {
    /// Accessors so screens can branch with plain `if let` rather than pattern matching inside
    /// a view builder.
    var page: CatalogPageView? {
        if case .loaded(let page) = self { return page }
        return nil
    }

    var isLoading: Bool {
        if case .loading = self { return true }
        return false
    }

    var failure: (code: String, message: String)? {
        if case .failed(let code, let message) = self { return (code, message) }
        return nil
    }
}

/// The subtitle for an album, artist, or playlist page.
///
/// Upstream headers often already lead with the kind ("Playlist • 2026"), so prefixing blindly
/// produces "Playlist · Playlist • 2026".
func detailSubtitle(kindLabel: String, pageSubtitle: String) -> String {
    let trimmed = pageSubtitle.trimmingCharacters(in: .whitespaces)
    guard !trimmed.isEmpty else { return kindLabel }
    guard !trimmed.lowercased().hasPrefix(kindLabel.lowercased()) else { return trimmed }
    return "\(kindLabel) · \(trimmed)"
}

/// Turns a service error code into something worth showing a person.
func catalogFailureText(code: String, message: String, subject: String) -> (title: String, detail: String) {
    switch code {
    case "catalogEmpty":
        return ("No results", "\(subject) returned nothing to show.")
    case "catalogUnavailable":
        return ("Catalog unreachable", "Could not reach YouTube Music. Check your connection and try again.")
    case "catalogUpstreamError":
        return ("Catalog rejected the request", message)
    case "catalogDecodeError":
        return ("Unreadable catalog response", "YouTube Music answered in a shape this build does not understand.")
    case "offline":
        return ("Service not connected", message)
    default:
        return ("Could not load", message)
    }
}
