import Foundation

let goosicProtocolVersion = "0.3.0"

enum GoosicOwner: String, Codable {
    case none
    case officialWebView
    case localDownloadedFile
}

struct GoosicRequestPayload: Codable {
    var owner: GoosicOwner?
    var generation: UInt64?
    var sequence: UInt64?
    var accountId: String?
    var account: GoosicAccountUpsert?
    var marker: String?
    var query: String?
    var filter: String?
    var catalogId: String?
    var limit: UInt32?
    var preferences: GoosicPreferencesPatch?
    var lyrics: GoosicLyricsQuery?

    init(
        owner: GoosicOwner? = nil,
        generation: UInt64? = nil,
        sequence: UInt64? = nil,
        accountId: String? = nil,
        account: GoosicAccountUpsert? = nil,
        marker: String? = nil,
        query: String? = nil,
        filter: String? = nil,
        catalogId: String? = nil,
        limit: UInt32? = nil,
        preferences: GoosicPreferencesPatch? = nil,
        lyrics: GoosicLyricsQuery? = nil
    ) {
        self.owner = owner
        self.generation = generation
        self.sequence = sequence
        self.accountId = accountId
        self.account = account
        self.marker = marker
        self.query = query
        self.filter = filter
        self.catalogId = catalogId
        self.limit = limit
        self.preferences = preferences
        self.lyrics = lyrics
    }
}

/// What to look lyrics up by.
struct GoosicLyricsQuery: Codable {
    var title: String
    var artist: String
    var album: String
    /// Track length in seconds; the lyrics database matches on it, so the right recording is
    /// returned rather than a same-titled cover.
    var durationSeconds: UInt32?
}

struct GoosicLyricsLine: Codable, Hashable, Identifiable {
    /// Milliseconds into the track, or negative when the lyrics are not synced.
    var atMs: Int64
    var text: String

    /// `ForEach` needs a stable identity, and repeated lyric lines share their text.
    var id: String { "\(atMs)-\(text)" }
}

struct GoosicLyrics: Codable, Hashable {
    var source: String
    var synced: Bool
    var lines: [GoosicLyricsLine]
    var truncated: Bool?
}

/// A partial preference update. Absent fields are left as they are.
struct GoosicPreferencesPatch: Codable {
    var theme: String?
    var volume: Double?
    var muted: Bool?
    var autoplay: Bool?
    var lastRoute: String?
    var queueVisible: Bool?
    var shuffle: Bool?
    var repeatMode: String?

    init(
        theme: String? = nil,
        volume: Double? = nil,
        muted: Bool? = nil,
        autoplay: Bool? = nil,
        lastRoute: String? = nil,
        queueVisible: Bool? = nil,
        shuffle: Bool? = nil,
        repeatMode: String? = nil
    ) {
        self.theme = theme
        self.volume = volume
        self.muted = muted
        self.autoplay = autoplay
        self.lastRoute = lastRoute
        self.queueVisible = queueVisible
        self.shuffle = shuffle
        self.repeatMode = repeatMode
    }
}

struct GoosicRequest: Codable {
    var protocolVersion: String = goosicProtocolVersion
    var requestId: String
    var command: String
    var payload: GoosicRequestPayload = .init()
}

/// Metadata-only account records. Authentication state remains in the platform WebKit profile.
struct GoosicAccountSummary: Codable, Identifiable, Hashable {
    var id: String
    var webkitProfileId: String
    var displayName: String
    var email: String?
    var channel: String?
    var avatarUrl: String?

    init(id: String, webkitProfileId: String, displayName: String, email: String? = nil, channel: String? = nil, avatarUrl: String? = nil) {
        self.id = id; self.webkitProfileId = webkitProfileId; self.displayName = displayName
        self.email = email; self.channel = channel; self.avatarUrl = avatarUrl
    }
}

struct GoosicAccountUpsert: Codable, Hashable {
    var id: String?
    var webkitProfileId: String
    var displayName: String
    var email: String?
    var channel: String?
    var avatarUrl: String?

    init(id: String? = nil, webkitProfileId: String, displayName: String, email: String? = nil, channel: String? = nil, avatarUrl: String? = nil) {
        self.id = id; self.webkitProfileId = webkitProfileId; self.displayName = displayName
        self.email = email; self.channel = channel; self.avatarUrl = avatarUrl
    }
}

struct GoosicAccountsSnapshot: Codable, Equatable {
    var accounts: [GoosicAccountSummary]
    var activeAccountId: String?
    var epoch: UInt64

    init(accounts: [GoosicAccountSummary] = [], activeAccountId: String? = nil, epoch: UInt64 = 0) {
        self.accounts = accounts; self.activeAccountId = activeAccountId; self.epoch = epoch
    }
}

struct GoosicPlaybackState: Codable {
    var accountId: String?
    var owner: GoosicOwner
    var generation: UInt64
    var sampleSequence: UInt64
}

struct GoosicError: Codable {
    var code: String
    var message: String
}

/// What a catalog row is. Mirrors `goosic_protocol::CatalogItemKind`.
///
/// Decoding is tolerant: an unrecognized kind from a newer service becomes `.unknown` rather
/// than failing the whole page.
enum GoosicCatalogKind: String, Codable {
    case song
    case video
    case album
    case artist
    case playlist
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = GoosicCatalogKind(rawValue: raw) ?? .unknown
    }
}

struct GoosicCatalogItem: Codable {
    var kind: GoosicCatalogKind
    var id: String
    var title: String
    var subtitle: String
    var artist: String?
    var artistId: String?
    var album: String?
    var albumId: String?
    var duration: String?
    var thumbnail: String?
    /// Present only when this row can be handed to the official player.
    var videoId: String?
    var explicit: Bool?
}

struct GoosicCatalogShelf: Codable {
    var id: String
    var title: String
    var items: [GoosicCatalogItem]
}

struct GoosicCatalogPage: Codable {
    var id: String
    var title: String
    var subtitle: String?
    var shelves: [GoosicCatalogShelf]?
    var tracks: [GoosicCatalogItem]?
    var thumbnail: String?
    /// True when the service clamped the upstream page to fit one protocol frame.
    var truncated: Bool?
}

struct GoosicResponsePayload: Codable {
    var message: String?
    var state: GoosicPlaybackState?
    var owner: GoosicOwner?
    var generation: UInt64?
    var sequence: UInt64?
    var markerAccepted: Bool?
    var catalog: GoosicCatalogPage?
    var settings: GoosicSettings?
    var accounts: GoosicAccountsSnapshot?
    /// Tracks already present on disk. The service resolves availability for every list request.
    var downloads: [GoosicDownloadedTrack]?
    /// The decoded cache path returned only after Rust has prepared a local track.
    var localFile: String?
    var lyrics: GoosicLyrics?
}

struct GoosicDownloadedTrack: Codable, Identifiable, Hashable {
    var videoId: String
    var title: String
    var artist: String
    var bytes: UInt64
    var available: Bool
    var imported: Bool

    var id: String { videoId }
    var subtitle: String {
        let parts = [artist].filter { !$0.isEmpty }
        return parts.isEmpty ? "Downloaded file" : parts.joined(separator: " · ")
    }
}

struct GoosicSettings: Codable {
    var theme: String
    var volume: Double
    var muted: Bool
    var autoplay: Bool
    var lastRoute: String
    var queueVisible: Bool
    var shuffle: Bool
    /// `off`, `all`, or `one`.
    var repeatMode: String
    /// Whether preferences from a previous Goosic install have been imported.
    var importedFromLegacy: Bool
    /// Whether a previous Goosic install's preferences are present to import.
    var legacyAvailable: Bool
}

struct GoosicResponse: Codable {
    var protocolVersion: String
    var requestId: String
    var ok: Bool
    var payload: GoosicResponsePayload?
    var error: GoosicError?
}
