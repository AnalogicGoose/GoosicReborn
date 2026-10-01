import Foundation

/// Sorting and finding in a playlist, as Spotify and Apple Music offer them.
///
/// Both are views over rows already loaded: nothing here changes a playlist. Custom order is the
/// playlist's own order, which is the order the rows arrived in. The Windows shell holds the same
/// rules in `Presentation/ListRules.cs`, and the two are kept to the same cases.
enum TrackSortOrder: String, CaseIterable, Identifiable {
    case custom
    case title
    case artist
    case album
    case duration

    var id: String { rawValue }

    var label: String {
        switch self {
        case .custom: return "Custom order"
        case .title: return "Title"
        case .artist: return "Artist"
        case .album: return "Album"
        case .duration: return "Duration"
        }
    }

    /// `tracks` in this order; ties keep the playlist's own order.
    func sorted(_ tracks: [GoosicTrack]) -> [GoosicTrack] {
        guard self != .custom else { return tracks }
        let indexed = Array(tracks.enumerated())
        return indexed.sorted { lhs, rhs in
            switch compare(lhs.element, rhs.element) {
            case .orderedAscending: return true
            case .orderedDescending: return false
            case .orderedSame: return lhs.offset < rhs.offset
            }
        }.map(\.element)
    }

    private func compare(_ lhs: GoosicTrack, _ rhs: GoosicTrack) -> ComparisonResult {
        switch self {
        case .custom:
            return .orderedSame
        case .title:
            return lhs.title.localizedCaseInsensitiveCompare(rhs.title)
        case .artist:
            return Self.artistLine(lhs).localizedCaseInsensitiveCompare(Self.artistLine(rhs))
        case .album:
            return lhs.album.localizedCaseInsensitiveCompare(rhs.album)
        case .duration:
            // A row whose length is unknown goes last rather than first.
            let left = GoosicAppModel.durationSeconds(lhs.duration) ?? .max
            let right = GoosicAppModel.durationSeconds(rhs.duration) ?? .max
            return left == right ? .orderedSame : (left < right ? .orderedAscending : .orderedDescending)
        }
    }

    private static func artistLine(_ track: GoosicTrack) -> String {
        track.artist.isEmpty ? track.subtitle : track.artist
    }

    /// Whether a row matches what was typed in the find box: title, artist line or album.
    static func matches(_ track: GoosicTrack, _ typed: String) -> Bool {
        let text = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return true }
        return track.title.localizedCaseInsensitiveContains(text)
            || track.artist.localizedCaseInsensitiveContains(text)
            || track.subtitle.localizedCaseInsensitiveContains(text)
            || track.album.localizedCaseInsensitiveContains(text)
    }
}

/// The search box's memory of what was searched. It lives in the shell's own preferences and
/// never reaches the service or YouTube Music.
enum RecentSearches {
    static let limit = 8

    /// `query` first, without a second copy of it, at most `limit`.
    static func remember(_ earlier: [String], _ query: String) -> [String] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return Array(earlier.prefix(limit)) }
        let rest = earlier.filter { $0.caseInsensitiveCompare(trimmed) != .orderedSame }
        return Array(([trimmed] + rest).prefix(limit))
    }

    /// What to offer while `typed` is in the box: every recent search when it is empty, those
    /// containing it otherwise, and never the text already there.
    static func suggest(_ recent: [String], _ typed: String) -> [String] {
        let text = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        return recent.filter { item in
            item.caseInsensitiveCompare(text) != .orderedSame
                && (text.isEmpty || item.localizedCaseInsensitiveContains(text))
        }
    }
}

/// Where the queue came from, so Up next can say so and a page's Play button can tell whether
/// the music playing is its own.
struct QueueSource: Equatable {
    /// "Liked Music", "Song radio", a playlist's title.
    let title: String
    /// The page that queued it, or `nil` for a queue no page owns, such as a radio.
    let key: CatalogKey?

    static func radio(_ track: GoosicTrack) -> QueueSource {
        QueueSource(title: "\(track.title) radio", key: nil)
    }
}

/// Words the player shows about the queue, a list's length, and the sleep timer.
enum PlayerText {
    /// "From Liked Music · 99 songs": where the queue came from, then what is left.
    static func upNext(source: String?, remaining: Int, findingMore: Bool) -> String {
        let prefix = (source?.isEmpty == false) ? "From \(source!) · " : ""
        let count: String
        switch remaining {
        case 0 where findingMore: count = "Finding songs like this…"
        case 0: count = "Nothing after this"
        case 1: count = "1 song"
        default: count = "\(remaining) songs"
        }
        return prefix + count
    }

    /// "100 songs", or "100+ songs" while a continuation is still pending: a partial list must
    /// not present itself as complete.
    static func songCount(_ count: Int, hasMore: Bool) -> String {
        let number = hasMore ? "\(count)+" : "\(count)"
        return count == 1 && !hasMore ? "1 song" : "\(number) songs"
    }

    /// The sleep timer's menu label: off, at the end of the song, or the minutes left.
    static func sleepTimer(remaining: TimeInterval?, endOfSong: Bool) -> String {
        if endOfSong { return "Sleep timer: end of song" }
        guard let remaining else { return "Sleep timer" }
        // Rounded up, so the last half-minute reads "1 min left" rather than "0".
        return "Sleep timer: \(max(1, Int((remaining / 60).rounded(.up)))) min left"
    }
}

/// When the sleep timer stops the music.
enum SleepTimerChoice: Equatable, Hashable {
    case minutes(Int)
    case endOfSong

    static let menu: [SleepTimerChoice] = [.minutes(15), .minutes(30), .minutes(45), .minutes(60), .endOfSong]

    var label: String {
        switch self {
        case .minutes(let minutes): return "\(minutes) minutes"
        case .endOfSong: return "End of this song"
        }
    }
}

/// Splits an album or playlist header's upstream subtitle into Music's two lines: the byline
/// (the artist, or the playlist's owner) shown large in the accent colour, and the quieter facts
/// after it, such as the year.
///
/// Upstream writes one line such as `Album • Metro Boomin • 2023`. The kind word is already the
/// page's own kind, so it is dropped; a year or a count is a fact, never a byline.
struct CollectionHeaderLines: Equatable {
    let byline: String?
    let facts: [String]

    private static let kindWords: Set<String> = [
        "album", "single", "ep", "playlist", "auto playlist", "liked music", "song", "video",
    ]

    init(subtitle: String, kind: String) {
        let parts = subtitle
            .components(separatedBy: CharacterSet(charactersIn: "•·"))
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        let meaningful = parts.filter {
            !Self.kindWords.contains($0.lowercased()) && $0.caseInsensitiveCompare(kind) != .orderedSame
        }
        let isFact: (String) -> Bool = { part in part.first?.isNumber ?? false }
        let byline = meaningful.first { !isFact($0) }
        self.byline = byline
        self.facts = meaningful.filter { $0 != byline }
    }
}
