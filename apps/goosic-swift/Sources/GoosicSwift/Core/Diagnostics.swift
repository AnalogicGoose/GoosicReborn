import Foundation

/// Local diagnostics: what took how long, what became ready, and why something failed.
///
/// Two rules shape this, and both are about what must *not* end up in a line of text.
///
/// The first is the protocol. stdout is the NDJSON channel between the shell and the service, and
/// a stray `print` there is not a cosmetic problem — it is a frame the other side will try to
/// parse. Diagnostics go to stderr, always, and this type is the only thing in the shell that
/// writes them, so there is one place to check rather than a habit to remember.
///
/// The second is the security boundary. Cookies, bridge tokens, signing keys, media URLs, and raw
/// account responses stay in platform-secure storage and are never logged. The way that rule gets
/// broken is not by logging a password on purpose; it is by logging a URL for context and not
/// thinking about its query string, which is where YouTube puts identifiers and where a media URL
/// puts its signature. So a URL cannot be logged whole: `origin(of:)` reduces one to scheme and
/// host, and a field value is truncated. Making the safe form the convenient one is the only
/// version of this that survives contact with a hurry.
enum Diagnostics {
    /// Which part of the shell is speaking. Kept small on purpose: a category nobody can name is
    /// a category nobody filters on.
    enum Subsystem: String {
        case service
        case personalCatalog = "personal-catalog"
        case accountLogin = "account-login"
        case officialPlayback = "official-playback"
        /// What the shell showed the listener, so a report can be read beside what they saw.
        case shell
    }

    /// Diagnostics are on by default because the questions they answer — why is this slow, what
    /// did the page do before it failed — are asked after the fact, by someone who cannot
    /// reproduce the problem. `GOOSIC_DIAGNOSTICS=0` silences them.
    private static let enabled: Bool = {
        ProcessInfo.processInfo.environment["GOOSIC_DIAGNOSTICS"] != "0"
    }()

    /// A field value long enough to be a payload is not a field value.
    private static let maxFieldLength = 120

    static func note(_ subsystem: Subsystem, _ event: String, _ fields: [String: String] = [:]) {
        guard enabled else { return }
        let text = line(subsystem, event, fields)
        FileHandle.standardError.write(Data(text.utf8))
        appendToLog(text)
    }

    /// Where the same lines are kept on disk, so Settings can open the folder a report needs.
    /// They pass through the same redaction as stderr; the file holds nothing stderr would not.
    static let logDirectory: URL? = FileManager.default
        .urls(for: .cachesDirectory, in: .userDomainMask).first?
        .appendingPathComponent("Goosic", isDirectory: true)
        .appendingPathComponent("Logs", isDirectory: true)

    private static let logQueue = DispatchQueue(label: "goosic.diagnostics.log")
    /// A log that has grown past this starts over, so it can never fill a disk.
    private static let maxLogBytes: UInt64 = 2_000_000

    private static func appendToLog(_ text: String) {
        guard let directory = logDirectory else { return }
        logQueue.async {
            let file = directory.appendingPathComponent("goosic.log")
            let manager = FileManager.default
            try? manager.createDirectory(at: directory, withIntermediateDirectories: true)
            let size = ((try? manager.attributesOfItem(atPath: file.path))?[.size] as? NSNumber)?.uint64Value ?? 0
            if size > maxLogBytes || !manager.fileExists(atPath: file.path) {
                manager.createFile(atPath: file.path, contents: nil)
            }
            guard let handle = try? FileHandle(forWritingTo: file) else { return }
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: Data(text.utf8))
        }
    }

    /// Built separately from the writing so the format can be asserted on.
    static func line(
        _ subsystem: Subsystem,
        _ event: String,
        _ fields: [String: String] = [:]
    ) -> String {
        // Sorted so a line is stable between runs and diffable between two of them.
        let rendered = fields.keys.sorted().map { key in
            " \(key)=\(clip(fields[key] ?? ""))"
        }.joined()
        return "[\(subsystem.rawValue)] \(clip(event))\(rendered)\n"
    }

    /// What is safe to say about a URL: where it went, not what it carried.
    ///
    /// The path is dropped along with the query because a YouTube Music path is itself an
    /// identifier, and a media URL's signature lives in both.
    static func origin(of url: URL?) -> String {
        guard let url, let host = url.host else { return "none" }
        guard let scheme = url.scheme else { return host }
        return "\(scheme)://\(host)"
    }

    static func milliseconds(since start: Date) -> String {
        "\(Int(Date().timeIntervalSince(start) * 1_000))ms"
    }

    /// Reduces an error to something worth reading without letting a message that quotes a
    /// response body through at full length.
    static func reason(_ error: Error) -> String {
        clip(error.localizedDescription)
    }

    private static func clip(_ value: String) -> String {
        // Newlines would forge a second line; a field is one line by construction.
        let flattened = value.replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
        guard flattened.count > maxFieldLength else { return flattened }
        return flattened.prefix(maxFieldLength) + "…"
    }
}
