import Foundation

/// Shelves describe discovery, never an ordered listening session.
enum PlaybackLaunchIntent: Equatable {
    case station(GoosicTrack)
    case ordered(GoosicTrack, [GoosicTrack])

    static func resolve(track: GoosicTrack, explicitTracks: [GoosicTrack]) -> Self {
        explicitTracks.isEmpty ? .station(track) : .ordered(track, explicitTracks)
    }
}

/// A cursor belongs to the reader and account that issued it, even if authentication changes.
struct RadioRequestIdentity: Equatable {
    let revision: UInt64
    let accountID: String?

    var usesPersonalCatalog: Bool { accountID != nil }

    func accepts(revision: UInt64, accountID: String?) -> Bool {
        self.revision == revision && self.accountID == accountID
    }
}
