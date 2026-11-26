import Foundation
import GoosicShellSupportFFI

/// The listening queue, decided by `QueueEngine` in `goosic-shell-support`.
///
/// The Mac used to keep its own queue, and it drifted from the Windows one: a search result
/// played the search list in order here and started a radio there, shuffle jumped around here
/// and reordered there, autoplay off stopped here after every song. The rules now live once, in
/// Rust, and this type is only the channel to them: commands go in as JSON, and what comes back
/// is what to do and what the queue now is. It holds no rule of its own.
///
/// Not `Sendable`: the model owns it and calls it from the main actor, and the engine's own lock
/// makes a stray call wait rather than corrupt anything.
final class SharedQueue {
    private let raw: OpaquePointer

    init() {
        raw = goosic_queue_new()
    }

    deinit {
        goosic_queue_free(raw)
    }

    /// Applies one command. A command the engine cannot read is a programming error in this file,
    /// so it is reported loudly in debug builds and answered with an unchanged snapshot.
    @discardableResult
    func handle(_ command: QueueCommand) -> QueueOutcome {
        let json = command.json
        let answer: Data? = json.withCString { pointer in
            guard let reply = goosic_queue_handle(raw, pointer) else { return nil }
            defer { goosic_string_free(reply) }
            return Data(String(cString: reply).utf8)
        }
        do {
            let envelope = try JSONDecoder().decode(
                SharedEnvelope<QueueOutcome>.self, from: answer ?? Data())
            if let outcome = envelope.ok { return outcome }
            assertionFailure("queue command refused: \(envelope.error ?? "no answer")")
        } catch {
            assertionFailure("unreadable queue outcome: \(error)")
        }
        if case .snapshot = command { return .empty }
        return handle(.snapshot)
    }
}

/// The stateless shell rules in `goosic-shell-support`, by name.
enum SharedRules {
    /// Answers rule `name` for `arguments`, or `nil` if the rule refused them.
    static func call<Value: Decodable>(_ name: String, _ arguments: [String: Any]) -> Value? {
        guard let input = try? JSONSerialization.data(withJSONObject: arguments),
              let inputText = String(data: input, encoding: .utf8) else { return nil }
        let answer: Data? = name.withCString { namePointer in
            inputText.withCString { inputPointer in
                guard let reply = goosic_rules_call(namePointer, inputPointer) else { return nil }
                defer { goosic_string_free(reply) }
                return Data(String(cString: reply).utf8)
            }
        }
        guard let answer,
              let envelope = try? JSONDecoder().decode(SharedEnvelope<Value>.self, from: answer)
        else { return nil }
        return envelope.ok
    }

    /// How a song chosen on `surface` starts: its list in order, or a station from it.
    static func launchKind(for surface: LaunchSurface) -> LaunchKind {
        call("launchKind", ["surface": surface.rawValue]).flatMap(LaunchKind.init(rawValue:))
            ?? .station
    }

    /// "100 songs", or "100+ songs" while more are still to come.
    static func songCount(_ count: Int, hasMore: Bool) -> String {
        call("songCount", ["count": count, "hasMore": hasMore]) ?? "\(count) songs"
    }

    /// The sleep timer's menu label.
    static func sleepTimer(remaining: TimeInterval?, endOfSong: Bool) -> String {
        var arguments: [String: Any] = ["endOfSong": endOfSong]
        if let remaining { arguments["remainingSeconds"] = remaining }
        return call("sleepTimer", arguments) ?? "Sleep timer"
    }
}

private struct SharedEnvelope<Value: Decodable>: Decodable {
    let ok: Value?
    let error: String?
}

/// Where a song was chosen, which decides how it starts.
enum LaunchSurface: String {
    case album, playlist, likedMusic, artist, search, shelf, queue
}

enum LaunchKind: String {
    case ordered, station
}

/// A song as the engine needs it: what to load and what to call it.
struct QueueTrackRef {
    let videoID: String
    let title: String

    init(_ track: GoosicTrack) {
        videoID = track.videoID
        title = track.title
    }

    var json: [String: Any] { ["videoId": videoID, "title": title] }
}

/// Something that happened, for the engine to decide about.
enum QueueCommand {
    case configure(shuffle: Bool, repeatMode: RepeatMode, autoplay: Bool)
    case setRadioAccount(String?)
    case launch(tracks: [GoosicTrack], start: Int?, kind: LaunchKind, sourceTitle: String?,
                sourceKey: String?, shuffle: Bool?)
    case startRadio(GoosicTrack)
    case next
    case previous(position: Double)
    case ended
    case jump(entry: UInt64)
    case playNext(GoosicTrack)
    case addToQueue(GoosicTrack)
    case remove(entry: UInt64)
    case reorder([UInt64])
    case clearUpcoming(now: TimeInterval)
    case undoClear(now: TimeInterval)
    case toggleShuffle
    case cycleRepeat
    case setAutoplay(Bool)
    case setSleepAtEnd(Bool)
    case refused(videoID: String)
    case radioLoaded(request: UInt64, tracks: [GoosicTrack], nextCursor: String?)
    case radioFailed(request: UInt64)
    case reset
    case snapshot

    /// The tracks this command carries, in order, so the entries the engine creates from them can
    /// be matched back to the shell's own track values by position.
    var carriedTracks: [GoosicTrack] {
        switch self {
        case .launch(let tracks, _, _, _, _, _): return tracks
        case .startRadio(let track), .playNext(let track), .addToQueue(let track): return [track]
        case .radioLoaded(_, let tracks, _): return tracks
        default: return []
        }
    }

    var json: String {
        let object: [String: Any]
        switch self {
        case .configure(let shuffle, let repeatMode, let autoplay):
            object = ["type": "configure", "shuffle": shuffle, "repeat": repeatMode.rawValue,
                      "autoplay": autoplay]
        case .setRadioAccount(let account):
            object = ["type": "setRadioAccount", "accountId": account ?? NSNull()]
        case .launch(let tracks, let start, let kind, let title, let key, let shuffle):
            object = ["type": "launch", "tracks": tracks.map { QueueTrackRef($0).json },
                      "start": start ?? NSNull(), "kind": kind.rawValue,
                      "sourceTitle": title ?? NSNull(), "sourceKey": key ?? NSNull(),
                      "shuffle": shuffle ?? NSNull()]
        case .startRadio(let track):
            object = ["type": "startRadio", "track": QueueTrackRef(track).json]
        case .next: object = ["type": "next"]
        case .previous(let position):
            object = ["type": "previous", "position": position.isFinite ? position : 0]
        case .ended: object = ["type": "ended"]
        case .jump(let entry): object = ["type": "jump", "entry": entry]
        case .playNext(let track): object = ["type": "playNext", "track": QueueTrackRef(track).json]
        case .addToQueue(let track):
            object = ["type": "addToQueue", "track": QueueTrackRef(track).json]
        case .remove(let entry): object = ["type": "remove", "entry": entry]
        case .reorder(let entries): object = ["type": "reorder", "entries": entries]
        case .clearUpcoming(let now): object = ["type": "clearUpcoming", "now": now]
        case .undoClear(let now): object = ["type": "undoClear", "now": now]
        case .toggleShuffle: object = ["type": "toggleShuffle"]
        case .cycleRepeat: object = ["type": "cycleRepeat"]
        case .setAutoplay(let enabled): object = ["type": "setAutoplay", "enabled": enabled]
        case .setSleepAtEnd(let enabled): object = ["type": "setSleepAtEnd", "enabled": enabled]
        case .refused(let videoID): object = ["type": "refused", "videoId": videoID]
        case .radioLoaded(let request, let tracks, let cursor):
            object = ["type": "radioLoaded", "request": request,
                      "tracks": tracks.map { QueueTrackRef($0).json },
                      "nextCursor": cursor ?? NSNull()]
        case .radioFailed(let request): object = ["type": "radioFailed", "request": request]
        case .reset: object = ["type": "reset"]
        case .snapshot: object = ["type": "snapshot"]
        }
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8) else { return #"{"type":"snapshot"}"# }
        return text
    }
}

/// What the shell should do now, in order.
enum QueueEffect: Decodable, Equatable {
    case play(entry: UInt64)
    case seekToStart
    case fetchRadio(request: UInt64, seedVideoID: String, continuation: String?, accountID: String?)
    case stopForSleep
    case savePreferences(shuffle: Bool, repeatMode: RepeatMode)
    case notice(String)
    case unknown

    private enum CodingKeys: String, CodingKey {
        case type, entry, request, seedVideoId, continuation, accountId, shuffle, text
        case repeatMode = "repeat"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "play": self = .play(entry: try container.decode(UInt64.self, forKey: .entry))
        case "seekToStart": self = .seekToStart
        case "fetchRadio":
            self = .fetchRadio(
                request: try container.decode(UInt64.self, forKey: .request),
                seedVideoID: try container.decode(String.self, forKey: .seedVideoId),
                continuation: try container.decodeIfPresent(String.self, forKey: .continuation),
                accountID: try container.decodeIfPresent(String.self, forKey: .accountId))
        case "stopForSleep": self = .stopForSleep
        case "savePreferences":
            self = .savePreferences(
                shuffle: try container.decode(Bool.self, forKey: .shuffle),
                repeatMode: RepeatMode(rawValue: try container.decode(String.self, forKey: .repeatMode)) ?? .off)
        case "notice": self = .notice(try container.decode(String.self, forKey: .text))
        default: self = .unknown
        }
    }
}

struct QueueAdded: Decodable, Equatable {
    let input: Int
    let entry: UInt64
}

/// The queue as the shell draws it.
struct QueueSnapshot: Decodable, Equatable {
    let entries: [UInt64]
    let current: Int?
    let sourceTitle: String?
    let sourceKey: String?
    let shuffle: Bool
    let repeatMode: RepeatMode
    let autoplay: Bool
    let sleepAtEnd: Bool
    let radio: Bool
    let radioLoading: Bool
    let upNextCount: Int
    let upNextText: String
    let canClear: Bool
    let undoDeadline: Double?
    let revision: UInt64

    private enum CodingKeys: String, CodingKey {
        case entries, current, sourceTitle, sourceKey, shuffle, autoplay, sleepAtEnd, radio
        case radioLoading, upNextCount, upNextText, canClear, undoDeadline, revision
        case repeatMode = "repeat"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        entries = try container.decode([UInt64].self, forKey: .entries)
        current = try container.decodeIfPresent(Int.self, forKey: .current)
        sourceTitle = try container.decodeIfPresent(String.self, forKey: .sourceTitle)
        sourceKey = try container.decodeIfPresent(String.self, forKey: .sourceKey)
        shuffle = try container.decode(Bool.self, forKey: .shuffle)
        repeatMode = RepeatMode(rawValue: try container.decode(String.self, forKey: .repeatMode)) ?? .off
        autoplay = try container.decode(Bool.self, forKey: .autoplay)
        sleepAtEnd = try container.decode(Bool.self, forKey: .sleepAtEnd)
        radio = try container.decode(Bool.self, forKey: .radio)
        radioLoading = try container.decode(Bool.self, forKey: .radioLoading)
        upNextCount = try container.decode(Int.self, forKey: .upNextCount)
        upNextText = try container.decode(String.self, forKey: .upNextText)
        canClear = try container.decode(Bool.self, forKey: .canClear)
        undoDeadline = try container.decodeIfPresent(Double.self, forKey: .undoDeadline)
        revision = try container.decode(UInt64.self, forKey: .revision)
    }

    private init() {
        entries = []; current = nil; sourceTitle = nil; sourceKey = nil; shuffle = false
        repeatMode = .off; autoplay = true; sleepAtEnd = false; radio = false; radioLoading = false
        upNextCount = 0; upNextText = "Nothing after this"; canClear = false; undoDeadline = nil
        revision = 0
    }

    static let empty = QueueSnapshot()
}

struct QueueOutcome: Decodable, Equatable {
    let effects: [QueueEffect]
    let added: [QueueAdded]
    let snapshot: QueueSnapshot

    private init(effects: [QueueEffect], added: [QueueAdded], snapshot: QueueSnapshot) {
        self.effects = effects; self.added = added; self.snapshot = snapshot
    }

    static let empty = QueueOutcome(effects: [], added: [], snapshot: .empty)
}

extension LaunchSurface {
    /// The surface a page's rows belong to. A page with no key is a shelf of suggestions.
    init(page key: CatalogKey?) {
        switch key {
        case .album: self = .album
        case .artist: self = .artist
        case .playlist(let id):
            self = GoosicEntityReference.playlist(id).isLikedMusic ? .likedMusic : .playlist
        case .library: self = .playlist
        case .search: self = .search
        case .route, .category, .none: self = .shelf
        }
    }
}
