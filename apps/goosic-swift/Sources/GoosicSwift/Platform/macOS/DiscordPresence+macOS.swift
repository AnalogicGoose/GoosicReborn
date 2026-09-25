#if os(macOS)
import Darwin
import Foundation

/// What Discord shows while a song plays.
struct DiscordActivity: Equatable, Sendable {
    let title: String
    let artist: String
    let album: String?
    let artworkURL: String?
    let started: Date?
    let ends: Date?
}

/// Shows the playing song on the listener's Discord profile, through the Discord app's local IPC
/// socket (`$TMPDIR/discord-ipc-N`).
///
/// Nothing leaves this machine from here: the Discord desktop app, already signed in, publishes
/// the status. No token, cookie, or account detail is read or sent — only the song's title,
/// artist, album, cover address and times, and only while the listener has this turned on. The
/// Windows shell does the same over a named pipe (`Service/DiscordPresence.cs`) with the same
/// application id, which is public: every client that shows a status sends it.
///
/// Updates are coalesced: the latest one wins and is sent at most every few seconds, because
/// Discord ignores a client that changes its status faster than about five times in twenty.
actor DiscordPresence {
    /// The Goosic application on Discord's developer portal.
    static let applicationID = "1526113133570293800"
    private static let minimumInterval: Duration = .seconds(4)

    private var socket: Int32 = -1
    private var latest: DiscordActivity??
    private var sending = false

    /// Shows a song, or clears the status when `activity` is `nil`.
    func update(_ activity: DiscordActivity?) {
        latest = .some(activity)
        guard !sending else { return }
        sending = true
        Task { await self.drain() }
    }

    private func drain() async {
        // Let a burst of changes (skip, skip, skip) settle into the one that stays.
        try? await Task.sleep(for: Self.minimumInterval)
        while let pending = latest {
            latest = nil
            send(pending)
            if latest != nil { try? await Task.sleep(for: Self.minimumInterval) }
        }
        sending = false
    }

    private func send(_ activity: DiscordActivity?) {
        if socket < 0 { connect() }
        guard socket >= 0 else { return }
        var args: [String: Any] = ["pid": Int(getpid())]
        args["activity"] = activity.map(Self.describe) ?? NSNull()
        let payload: [String: Any] = ["cmd": "SET_ACTIVITY", "nonce": UUID().uuidString, "args": args]
        guard write(opcode: 1, payload), readFrame(step: activity == nil ? "cleared" : "status") else {
            // Discord is closed or restarted; the next update reconnects.
            disconnect()
            return
        }
    }

    private static func describe(_ activity: DiscordActivity) -> [String: Any] {
        var description: [String: Any] = [
            // 2 is "Listening to", the way music players appear.
            "type": 2,
            "details": field(activity.title),
            "state": field(activity.artist.isEmpty ? "Goosic" : activity.artist),
        ]
        if let started = activity.started {
            var timestamps: [String: Any] = ["start": Int64(started.timeIntervalSince1970 * 1_000)]
            if let ends = activity.ends { timestamps["end"] = Int64(ends.timeIntervalSince1970 * 1_000) }
            description["timestamps"] = timestamps
        }
        if let artwork = activity.artworkURL, artwork.hasPrefix("https://") {
            description["assets"] = ["large_image": artwork, "large_text": field(activity.album ?? activity.title)]
        }
        return description
    }

    /// Discord refuses a text field shorter than two or longer than 128 characters.
    private static func field(_ text: String) -> String {
        var text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.count > 128 { text = String(text.prefix(127)) + "…" }
        while text.count < 2 { text += "\u{2800}" }
        return text
    }

    private func connect() {
        let base = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
        for index in 0..<10 {
            let path = (base as NSString).appendingPathComponent("discord-ipc-\(index)")
            let candidate = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
            guard candidate >= 0 else { return }
            var timeout = timeval(tv_sec: 2, tv_usec: 0)
            setsockopt(candidate, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
            setsockopt(candidate, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
            var noSigPipe: Int32 = 1
            setsockopt(candidate, SOL_SOCKET, SO_NOSIGPIPE, &noSigPipe, socklen_t(MemoryLayout<Int32>.size))
            var address = sockaddr_un()
            address.sun_family = sa_family_t(AF_UNIX)
            let capacity = MemoryLayout.size(ofValue: address.sun_path)
            guard path.utf8.count < capacity else { Darwin.close(candidate); continue }
            withUnsafeMutableBytes(of: &address.sun_path) { buffer in
                buffer.copyBytes(from: path.utf8)
                buffer[path.utf8.count] = 0
            }
            let connected = withUnsafePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    Darwin.connect(candidate, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
                }
            }
            guard connected == 0 else { Darwin.close(candidate); continue }
            socket = candidate
            if write(opcode: 0, ["v": 1, "client_id": Self.applicationID]), readFrame(step: "connected") {
                return
            }
            disconnect()
        }
    }

    private func disconnect() {
        if socket >= 0 { Darwin.close(socket) }
        socket = -1
    }

    private func write(opcode: Int32, _ body: [String: Any]) -> Bool {
        guard let json = try? JSONSerialization.data(withJSONObject: body) else { return false }
        var frame = Data()
        withUnsafeBytes(of: opcode.littleEndian) { frame.append(contentsOf: $0) }
        withUnsafeBytes(of: Int32(json.count).littleEndian) { frame.append(contentsOf: $0) }
        frame.append(json)
        return frame.withUnsafeBytes { buffer in
            var offset = 0
            while offset < buffer.count {
                let written = Darwin.write(socket, buffer.baseAddress! + offset, buffer.count - offset)
                if written <= 0 { return false }
                offset += written
            }
            return true
        }
    }

    private func readExactly(_ count: Int) -> Data? {
        var data = Data(count: count)
        let ok = data.withUnsafeMutableBytes { buffer -> Bool in
            var offset = 0
            while offset < count {
                let got = Darwin.read(socket, buffer.baseAddress! + offset, count - offset)
                if got <= 0 { return false }
                offset += got
            }
            return true
        }
        return ok ? data : nil
    }

    private func readFrame(step: String) -> Bool {
        guard let header = readExactly(8) else { return false }
        let length = header.withUnsafeBytes { Int32(littleEndian: $0.loadUnaligned(fromByteOffset: 4, as: Int32.self)) }
        guard length >= 0, length <= 64 * 1024, readExactly(Int(length)) != nil else { return false }
        // Only the step is logged: Discord's READY answer also describes the signed-in Discord
        // user, which has no business in Goosic's log.
        Diagnostics.note(.shell, "discord.\(step)")
        return true
    }
}

/// Follows the model and tells Discord what is playing, only when something Discord shows has
/// changed and only while the listener has it turned on.
@MainActor
final class DiscordPresenceBridge {
    private let presence = DiscordPresence()
    private var lastKey: String?

    func refresh(from model: GoosicAppModel) {
        let track = model.discordStatus && !model.isPaused && !model.isAdvertisement
            ? model.currentTrack : nil
        let key = track.map { "\($0.videoID)|\(Int(model.duration))" } ?? "none"
        guard key != lastKey else { return }
        // Never having shown anything, there is nothing to clear.
        if lastKey == nil && track == nil { lastKey = key; return }
        lastKey = key
        let activity = track.map { track in
            let started = Date().addingTimeInterval(-model.currentTime)
            return DiscordActivity(
                title: track.title,
                artist: track.artist,
                album: track.album.isEmpty ? nil : track.album,
                artworkURL: track.thumbnail,
                started: started,
                ends: model.duration > 0 ? started.addingTimeInterval(model.duration) : nil
            )
        }
        let presence = presence
        Task { await presence.update(activity) }
    }
}
#endif
