import Foundation

enum PlaylistPrivacy: String, CaseIterable, Identifiable {
    case `private` = "PRIVATE"
    case unlisted = "UNLISTED"
    case `public` = "PUBLIC"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .private: return "Private"
        case .unlisted: return "Unlisted"
        case .public: return "Public"
        }
    }
}

enum TrackRating: String {
    case liked = "LIKE"
    case disliked = "DISLIKE"
    case none = "INDIFFERENT"
}

/// A change to the signed-in account, named rather than described.
///
/// The shell can ask for one of these and nothing else. It cannot hand the reader an endpoint and
/// a body, because that would put the shape of an authenticated request — and therefore the
/// question of what may be sent as the user — on the side of the boundary that renders things.
/// Every case here is an operation somebody chose to expose; a new one is a deliberate edit to
/// this file, not an incidental consequence of a screen wanting something.
enum PersonalMutation: Equatable {
    case rateTrack(videoID: String, rating: TrackRating)
    /// Saving a playlist or an album into the library. YouTube Music models this as a rating on
    /// the playlist rather than a library edit, which is why an album is saved by its
    /// `OLAK5uy_…` audio-playlist id through the same call.
    case savePlaylist(playlistID: String, saved: Bool)
    case followArtist(channelID: String, followed: Bool)

    case addToPlaylist(playlistID: String, videoID: String)
    /// `entryID` is the playlist's own per-entry identifier. A bare video id would be ambiguous
    /// in a playlist holding the same track twice, and upstream would remove the wrong one.
    case removeFromPlaylist(playlistID: String, videoID: String, entryID: String)
    /// `afterEntryID` is the entry the moved one should follow; `nil` moves it to the front.
    case movePlaylistItem(playlistID: String, entryID: String, afterEntryID: String?)

    case createPlaylist(title: String, description: String?, privacy: PlaylistPrivacy, videoIDs: [String])
    case renamePlaylist(playlistID: String, title: String)
    case setPlaylistDescription(playlistID: String, description: String)
    case setPlaylistPrivacy(playlistID: String, privacy: PlaylistPrivacy)
    case deletePlaylist(playlistID: String)

    case listUserPlaylists

    /// The name the reader dispatches on. Kept beside the arguments so the two cannot disagree.
    var operation: String {
        switch self {
        case .rateTrack: return "rateTrack"
        case .savePlaylist: return "ratePlaylist"
        case .followArtist: return "setSubscription"
        case .addToPlaylist: return "addToPlaylist"
        case .removeFromPlaylist: return "removeFromPlaylist"
        case .movePlaylistItem: return "movePlaylistItem"
        case .createPlaylist: return "createPlaylist"
        case .renamePlaylist: return "renamePlaylist"
        case .setPlaylistDescription: return "setPlaylistDescription"
        case .setPlaylistPrivacy: return "setPlaylistPrivacy"
        case .deletePlaylist: return "deletePlaylist"
        case .listUserPlaylists: return "listUserPlaylists"
        }
    }

    var arguments: [String: Any] {
        switch self {
        case .rateTrack(let videoID, let rating):
            return ["videoId": videoID, "status": rating.rawValue]
        case .savePlaylist(let playlistID, let saved):
            return ["playlistId": playlistID, "saved": saved]
        case .followArtist(let channelID, let followed):
            return ["channelId": channelID, "subscribed": followed]
        case .addToPlaylist(let playlistID, let videoID):
            return ["playlistId": playlistID, "videoId": videoID]
        case .removeFromPlaylist(let playlistID, let videoID, let entryID):
            return ["playlistId": playlistID, "videoId": videoID, "setVideoId": entryID]
        case .movePlaylistItem(let playlistID, let entryID, let afterEntryID):
            var arguments: [String: Any] = ["playlistId": playlistID, "setVideoId": entryID]
            // Sent only when there is one: an absent predecessor means "to the front", which is
            // a different request from a predecessor that happens to be empty.
            if let afterEntryID { arguments["predecessorSetVideoId"] = afterEntryID }
            return arguments
        case .createPlaylist(let title, let description, let privacy, let videoIDs):
            var arguments: [String: Any] = ["title": title, "privacy": privacy.rawValue]
            if let description, !description.isEmpty { arguments["description"] = description }
            if !videoIDs.isEmpty { arguments["videoIds"] = videoIDs }
            return arguments
        case .renamePlaylist(let playlistID, let title):
            return ["playlistId": playlistID, "title": title]
        case .setPlaylistDescription(let playlistID, let description):
            return ["playlistId": playlistID, "description": description]
        case .setPlaylistPrivacy(let playlistID, let privacy):
            return ["playlistId": playlistID, "privacy": privacy.rawValue]
        case .deletePlaylist(let playlistID):
            return ["playlistId": playlistID]
        case .listUserPlaylists:
            return [:]
        }
    }

    /// Identity used to coalesce a repeat of a request already in flight.
    ///
    /// This is not idempotency in the sense a server offers it — InnerTube has no such notion, and
    /// claiming otherwise here would be a lie the caller might rely on. It is narrower and honest:
    /// while one of these is outstanding, an identical one joins it instead of becoming a second
    /// request. That covers what people actually do, which is press Like twice because the first
    /// press did not visibly do anything yet, and it means the second press cannot race the first
    /// into a rating that ends up the opposite of what they chose.
    ///
    /// Two mutations that differ in any argument have different keys and both run — including a
    /// like and an immediate unlike, which is a genuine change of mind and must not be swallowed.
    var coalescingKey: String {
        let rendered = arguments.keys.sorted().map { key in
            "\(key)=\(arguments[key].map { "\($0)" } ?? "")"
        }.joined(separator: "&")
        return "\(operation)?\(rendered)"
    }

    /// Whether the account's own view of things is different afterwards, and so what is cached
    /// about it can no longer be trusted. A listing is not.
    var changesLibrary: Bool {
        if case .listUserPlaylists = self { return false }
        return true
    }

    static func == (lhs: PersonalMutation, rhs: PersonalMutation) -> Bool {
        lhs.coalescingKey == rhs.coalescingKey
    }
}

/// A playlist this account owns and may edit.
struct PersonalPlaylistSummary: Decodable, Identifiable, Hashable {
    let id: String
    let title: String
    let subtitle: String
    let thumbnail: String?
}

/// What a mutation answered. Every operation returns an object so that "nothing to report" and
/// "no answer at all" cannot decode to the same thing.
struct PersonalMutationResult: Decodable {
    /// Set by `createPlaylist`, so the caller can open what it just made.
    let playlistId: String?
    /// Set by `listUserPlaylists`.
    let value: [PersonalPlaylistSummary]?

    var playlists: [PersonalPlaylistSummary] { value ?? [] }
}
