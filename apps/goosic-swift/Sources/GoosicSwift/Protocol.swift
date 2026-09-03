import Foundation

let goosicProtocolVersion = "0.2.0"

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
    var marker: String?
    var query: String?
    var filter: String?
    var catalogId: String?
    var limit: UInt32?

    init(
        owner: GoosicOwner? = nil,
        generation: UInt64? = nil,
        sequence: UInt64? = nil,
        accountId: String? = nil,
        marker: String? = nil,
        query: String? = nil,
        filter: String? = nil,
        catalogId: String? = nil,
        limit: UInt32? = nil
    ) {
        self.owner = owner
        self.generation = generation
        self.sequence = sequence
        self.accountId = accountId
        self.marker = marker
        self.query = query
        self.filter = filter
        self.catalogId = catalogId
        self.limit = limit
    }
}

struct GoosicRequest: Codable {
    var protocolVersion: String = goosicProtocolVersion
    var requestId: String
    var command: String
    var payload: GoosicRequestPayload = .init()
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
}

struct GoosicResponse: Codable {
    var protocolVersion: String
    var requestId: String
    var ok: Bool
    var payload: GoosicResponsePayload?
    var error: GoosicError?
}
