import Foundation

/// Downloads and caches catalog artwork.
///
/// SwiftCrossUI's `Image` reads its source synchronously while computing layout, so a remote URL
/// handed to it directly would block the UI on every card. Artwork is therefore fetched here,
/// off the main thread, and `Image` is only ever given a local file.
///
/// The session is ephemeral and carries no cookies: artwork is public CDN content, and a
/// thumbnail request must never become an authenticated one.
@MainActor
final class ArtworkCache {
    /// Hosts YouTube Music serves artwork from. Anything else is refused rather than fetched,
    /// so a catalog response cannot point the shell at an arbitrary server.
    private static let allowedHostSuffixes = [
        "googleusercontent.com",
        "ggpht.com",
        "ytimg.com",
        "youtube.com",
    ]

    /// Artwork is small. Anything larger is not a thumbnail and is discarded.
    private static let maxBytes = 4 * 1024 * 1024
    /// A bound on how many downloads are in flight, so opening a dense screen cannot start
    /// hundreds of connections at once.
    private static let maxConcurrentFetches = 6

    private let directory: URL
    private let session: URLSession
    /// Local files known to exist, keyed by remote URL.
    private var ready: [String: URL] = [:]
    /// Remote URLs currently being fetched.
    private var inFlight: Set<String> = []
    /// Remote URLs waiting for a slot.
    private var pending: [String] = []
    /// URLs that failed or were refused. Kept so a broken image is not retried on every layout.
    private var failed: Set<String> = []

    /// Called when new artwork becomes available, so the shell can re-render.
    var onArtworkLoaded: (() -> Void)?

    init(directory: URL? = nil) {
        let base = directory ?? Self.defaultDirectory()
        self.directory = base
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)

        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.timeoutIntervalForRequest = 15
        session = URLSession(configuration: configuration)
    }

    private static func defaultDirectory() -> URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        return caches.appendingPathComponent("com.goosic/artwork", isDirectory: true)
    }

    /// Whether this build is willing to fetch `url` at all.
    ///
    /// Pure, so it is callable off the main actor and directly testable.
    nonisolated static func isAllowed(_ url: URL) -> Bool {
        guard url.scheme?.lowercased() == "https", let host = url.host?.lowercased() else {
            return false
        }
        return allowedHostSuffixes.contains { suffix in
            host == suffix || host.hasSuffix("." + suffix)
        }
    }

    /// A stable, collision-resistant file name for a remote URL.
    ///
    /// Two independent FNV-1a passes give 128 bits, which is far more than enough to keep two
    /// thumbnails from sharing a cache file, without pulling in a hashing dependency.
    nonisolated static func cacheKey(for remote: String) -> String {
        func fnv1a(_ bytes: [UInt8], seed: UInt64) -> UInt64 {
            var hash = seed
            for byte in bytes {
                hash ^= UInt64(byte)
                hash = hash &* 0x100_0000_01b3
            }
            return hash
        }
        let bytes = Array(remote.utf8)
        let low = fnv1a(bytes, seed: 0xcbf2_9ce4_8422_2325)
        let high = fnv1a(bytes.reversed(), seed: 0x9dc5_bb15_8f2c_1e37)
        return String(format: "%016lx%016lx", low, high)
    }

    /// The local file for `remote`, if it has already been fetched.
    ///
    /// Returns `nil` and schedules a fetch otherwise, so callers can render a placeholder now
    /// and the real artwork once it arrives. Safe to call from `body`.
    func localFile(for remote: String?) -> URL? {
        guard let remote, !remote.isEmpty else { return nil }
        if let known = ready[remote] { return known }
        guard !failed.contains(remote) else { return nil }

        let destination = directory.appendingPathComponent("\(Self.cacheKey(for: remote)).img")
        if FileManager.default.fileExists(atPath: destination.path) {
            ready[remote] = destination
            return destination
        }
        schedule(remote, destination: destination)
        return nil
    }

    private func schedule(_ remote: String, destination: URL) {
        guard !inFlight.contains(remote), !pending.contains(remote) else { return }
        guard let url = URL(string: remote), Self.isAllowed(url) else {
            failed.insert(remote)
            return
        }
        guard inFlight.count < Self.maxConcurrentFetches else {
            pending.append(remote)
            return
        }
        inFlight.insert(remote)
        Task { [weak self] in
            await self?.fetch(remote: remote, url: url, destination: destination)
        }
    }

    private func fetch(remote: String, url: URL, destination: URL) async {
        defer { finish(remote) }
        do {
            let (data, response) = try await session.data(from: url)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
                failed.insert(remote)
                return
            }
            guard !data.isEmpty, data.count <= Self.maxBytes else {
                failed.insert(remote)
                return
            }
            // Written beside the destination and renamed, so a half-written file can never be
            // picked up as a valid cache entry by a later layout pass.
            let partial = destination.appendingPathExtension("partial")
            try data.write(to: partial, options: .atomic)
            _ = try? FileManager.default.replaceItemAt(destination, withItemAt: partial)
            if FileManager.default.fileExists(atPath: destination.path) {
                ready[remote] = destination
                onArtworkLoaded?()
            } else {
                failed.insert(remote)
            }
        } catch {
            // Artwork is decoration. A failure leaves the placeholder in place.
            failed.insert(remote)
        }
    }

    private func finish(_ remote: String) {
        inFlight.remove(remote)
        while inFlight.count < Self.maxConcurrentFetches, !pending.isEmpty {
            let next = pending.removeFirst()
            let destination = directory.appendingPathComponent("\(Self.cacheKey(for: next)).img")
            guard let url = URL(string: next), Self.isAllowed(url) else {
                failed.insert(next)
                continue
            }
            inFlight.insert(next)
            Task { [weak self] in
                await self?.fetch(remote: next, url: url, destination: destination)
            }
        }
    }
}
