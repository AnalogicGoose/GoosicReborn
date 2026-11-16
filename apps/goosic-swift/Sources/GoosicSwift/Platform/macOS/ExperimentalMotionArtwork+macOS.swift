#if os(macOS)
import AVFoundation
import AppKit
import SwiftUI

// Matching and manifest parsing adapted from Sung (MIT), copyright 2026 Sung contributors.
// See Resources/Sung-Motion-Artwork-LICENSE.txt for the complete permission notice.

/// Independent presentation-only experiment. No account profile or service transport is used.
struct MotionArtworkQuery: Hashable, Sendable {
    let title: String
    let artist: String
    let album: String
    let seconds: Double

    init(track: GoosicTrack) {
        title = track.title
        artist = track.artist.replacingOccurrences(of: " - Topic", with: "")
        album = track.album
        seconds = track.duration.split(separator: ":").reduce(0) { $0 * 60 + (Double($1) ?? 0) }
    }

    init(title: String, artist: String, album: String, seconds: Double) {
        self.title = title; self.artist = artist; self.album = album; self.seconds = seconds
    }
}

enum MotionArtworkParsing {
    static func normal(_ value: String) -> String {
        value.lowercased().components(separatedBy: CharacterSet.alphanumerics.inverted)
            .filter { !$0.isEmpty }.joined(separator: " ")
    }

    static func candidates(_ results: [[String: Any]], query: MotionArtworkQuery) -> [[String: Any]] {
        guard !normal(query.title).isEmpty, !normal(query.artist).isEmpty else { return [] }
        let title = query.title.replacingOccurrences(
            of: #"\s*[\[(](?:official (?:music )?(?:video|audio)|lyrics?|lyric video)[\])]\s*"#,
            with: " ", options: .regularExpression)
        let found = results.prefix(200).filter {
            normal($0["trackName"] as? String ?? "") == normal(title)
                && normal($0["artistName"] as? String ?? "") == normal(query.artist)
                && (query.album.isEmpty || normal($0["collectionName"] as? String ?? "") == normal(query.album))
                && MotionArtworkPolicy.durationMatches(query.seconds, (($0["trackTimeMillis"] as? NSNumber)?.doubleValue ?? 0) / 1000)
                && ($0["collectionId"] as? NSNumber) != nil
        }
        guard !query.album.isEmpty || Set(found.map { normal($0["collectionName"] as? String ?? "") }).count == 1 else { return [] }
        var seen = Set<String>()
        return Array(found.filter { seen.insert(($0["collectionId"] as? NSNumber)?.stringValue ?? "").inserted }.prefix(3))
    }

    static func motionURL(page: String, candidate: [String: Any]) -> URL? {
        guard let regex = try? NSRegularExpression(pattern: #"<script[^>]*id="serialized-server-data"[^>]*>(.*?)</script>"#, options: .dotMatchesLineSeparators),
              let match = regex.firstMatch(in: page, range: NSRange(page.startIndex..., in: page)),
              let range = Range(match.range(at: 1), in: page),
              let bytes = String(page[range]).data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              let id = candidate["collectionId"] as? NSNumber else { return nil }
        for entry in (payload["data"] as? [[String: Any]] ?? []).prefix(4) {
            for section in ((entry["data"] as? [String: Any])?["sections"] as? [[String: Any]] ?? []).prefix(12) {
                for item in (section["items"] as? [[String: Any]] ?? []).prefix(20) {
                    let artists = (item["subtitleLinks"] as? [[String: Any]] ?? []).map { normal($0["title"] as? String ?? "") }
                    guard item["id"] as? String == "album-detail-header - " + id.stringValue,
                          normal(item["title"] as? String ?? "") == normal(candidate["collectionName"] as? String ?? ""),
                          artists == [normal(candidate["collectionArtistName"] as? String ?? candidate["artistName"] as? String ?? "")] else { continue }
                    let dictionary = (item["videoArtwork"] as? [String: Any])?["dictionary"] as? [String: Any]
                    let raw = (dictionary?["motionDetailSquare"] as? [String: Any])?["video"] as? String
                    guard let raw, let url = URL(string: raw), MotionArtworkPolicy.allows(url) else { return nil }
                    return url
                }
            }
        }
        return nil
    }

    static func attributes(_ line: String) -> [String: String] {
        guard let regex = try? NSRegularExpression(pattern: #"([A-Z-]+)=("[^"]*"|[^,]*)"#) else { return [:] }
        var result: [String: String] = [:]
        for match in regex.matches(in: line, range: NSRange(line.startIndex..., in: line)) {
            guard let key = Range(match.range(at: 1), in: line), let val = Range(match.range(at: 2), in: line) else { continue }
            result[String(line[key])] = String(line[val]).trimmingCharacters(in: CharacterSet(charactersIn: "\""))
        }
        return result
    }

    static func resolve(_ raw: String, base: URL) -> URL? {
        guard let url = URL(string: raw, relativeTo: base)?.absoluteURL, MotionArtworkPolicy.allows(url) else { return nil }
        return url
    }

    static func variants(_ manifest: String, base: URL) -> [URL] {
        let lines = manifest.components(separatedBy: .newlines)
        var found: [(Int, URL)] = []
        for index in lines.indices.dropLast() where lines[index].hasPrefix("#EXT-X-STREAM-INF:") {
            let info = attributes(lines[index])
            let size = (info["RESOLUTION"] ?? "").split(separator: "x").compactMap { Int($0) }
            guard size.count == 2, size[0] == size[1], (128...1080).contains(size[0]),
                  (info["CODECS"] ?? "").hasPrefix("avc1"), (info["VIDEO-RANGE"] ?? "SDR") == "SDR",
                  let rate = Double(info["BANDWIDTH"] ?? ""), rate > 0, rate <= 8_000_000,
                  let fps = Double(info["FRAME-RATE"] ?? "30"), fps > 0, fps <= 30,
                  !lines[index + 1].hasPrefix("#"), let url = resolve(lines[index + 1], base: base) else { continue }
            found.append((size[0], url))
        }
        return found.sorted { $0.0 > $1.0 }.map(\.1)
    }

    static func movie(_ manifest: String, base: URL) -> URL? {
        let lines = manifest.components(separatedBy: .newlines)
        guard lines.count <= 512, lines.contains("#EXT-X-ENDLIST") else { return nil }
        var urls = Set<URL>(), duration = 0.0, segments = 0, size = 0
        for line in lines {
            if line.hasPrefix("#EXT-X-KEY:"), attributes(line)["METHOD"] != "NONE" { return nil }
            if line.hasPrefix("#EXTINF:") {
                guard let seconds = Double(line.dropFirst(8).split(separator: ",").first ?? ""), seconds.isFinite, seconds > 0 else { return nil }
                duration += seconds; segments += 1
            }
            if line.hasPrefix("#EXT-X-BYTERANGE:") {
                guard let length = Int(line.dropFirst(17).split(separator: "@").first ?? ""), length > 0, length <= MotionArtworkPolicy.videoLimit else { return nil }
                size += length
            }
            if line.hasPrefix("#EXT-X-MAP:") {
                guard let raw = attributes(line)["URI"], let url = resolve(raw, base: base) else { return nil }
                urls.insert(url)
            } else if !line.isEmpty && !line.hasPrefix("#") {
                guard let url = resolve(line, base: base) else { return nil }; urls.insert(url)
            }
        }
        guard (1...64).contains(segments), duration > 0, duration <= MotionArtworkPolicy.durationLimit,
              size <= MotionArtworkPolicy.videoLimit, urls.count == 1, let url = urls.first, url.pathExtension == "mp4" else { return nil }
        return url
    }
}

/// Redirects get the same host check as initial requests. A fresh session cannot inherit cookies.
private final class MotionArtworkRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(request.url.map(MotionArtworkPolicy.allows) == true ? request : nil)
    }
}

actor ExperimentalMotionResolver {
    static let shared = ExperimentalMotionResolver()
    private let session: URLSession

    init() {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.timeoutIntervalForRequest = 10
        configuration.timeoutIntervalForResource = 30
        session = URLSession(configuration: configuration, delegate: MotionArtworkRedirects(), delegateQueue: nil)
    }

    private func fetch(_ url: URL, limit: Int = MotionArtworkPolicy.metadataLimit) async throws -> Data {
        guard MotionArtworkPolicy.allows(url) else { throw URLError(.unsupportedURL) }
        var request = URLRequest(url: url)
        request.setValue("Goosic-Motion-Experiment/1.0", forHTTPHeaderField: "User-Agent")
        let (bytes, response) = try await session.bytes(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              response.expectedContentLength <= limit else { throw URLError(.badServerResponse) }
        var data = Data()
        for try await byte in bytes {
            if data.count % 65536 == 0 { try Task.checkCancellation() }
            guard data.count < limit else { throw URLError(.dataLengthExceedsMaximum) }
            data.append(byte)
        }
        return data
    }

    func resolve(_ query: MotionArtworkQuery) async throws -> URL? {
        var search = URLComponents(string: "https://itunes.apple.com/search")!
        search.queryItems = [URLQueryItem(name: "term", value: "\(query.artist) \(query.title) \(query.album)"),
                            URLQueryItem(name: "entity", value: "song"), URLQueryItem(name: "media", value: "music"),
                            URLQueryItem(name: "country", value: "us"), URLQueryItem(name: "limit", value: "50")]
        let data = try await fetch(search.url!)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        for candidate in MotionArtworkParsing.candidates(object?["results"] as? [[String: Any]] ?? [], query: query) {
            try Task.checkCancellation()
            guard let id = candidate["collectionId"] as? NSNumber else { continue }
            let page = try await fetch(URL(string: "https://music.apple.com/us/album/\(id.stringValue)")!)
            guard let master = MotionArtworkParsing.motionURL(page: String(decoding: page, as: UTF8.self), candidate: candidate) else { continue }
            let manifest = try await fetch(master)
            for variant in MotionArtworkParsing.variants(String(decoding: manifest, as: UTF8.self), base: master).prefix(3) {
                let playlist = try await fetch(variant)
                guard let movie = MotionArtworkParsing.movie(String(decoding: playlist, as: UTF8.self), base: variant) else { continue }
                let video = try await fetch(movie, limit: MotionArtworkPolicy.videoLimit)
                let file = FileManager.default.temporaryDirectory.appendingPathComponent("goosic-motion-\(UUID().uuidString).mp4")
                do {
                    try Task.checkCancellation()
                    try video.write(to: file, options: .atomic)
                    let asset = AVURLAsset(url: file)
                    let audio = try await asset.loadTracks(withMediaType: .audio)
                    let tracks = try await asset.loadTracks(withMediaType: .video)
                    let duration = try await asset.load(.duration).seconds
                    guard audio.isEmpty, tracks.count == 1, duration.isFinite, duration > 0,
                          duration <= MotionArtworkPolicy.durationLimit, let track = tracks.first else { throw URLError(.cannotDecodeContentData) }
                    let size = try await track.load(.naturalSize)
                    let fps = try await track.load(.nominalFrameRate)
                    let formats = try await track.load(.formatDescriptions)
                    guard size.width == size.height, (128...1080).contains(size.width), fps > 0, fps <= 30,
                          formats.allSatisfy({ CMFormatDescriptionGetMediaSubType($0) == kCMVideoCodecType_H264 }) else { throw URLError(.cannotDecodeContentData) }
                    try Task.checkCancellation()
                    return file
                } catch {
                    try? FileManager.default.removeItem(at: file)
                    throw error
                }
            }
        }
        return nil
    }
}

@MainActor
private final class ExperimentalMotionController: ObservableObject {
    @Published var player: AVQueuePlayer?
    @Published var ready = false
    private var looper: AVPlayerLooper?
    private var file: URL?
    private var observation: NSKeyValueObservation?
    private var generation = UUID()
    private var shouldPlay = false

    func setPlaying(_ value: Bool) {
        shouldPlay = value
        if value { player?.play() } else { player?.pause() }
    }

    func clear() {
        generation = UUID()
        observation = nil; ready = false
        player?.pause(); looper?.disableLooping(); player?.removeAllItems()
        looper = nil; player = nil
        if let file { try? FileManager.default.removeItem(at: file) }; file = nil
    }

    func install(_ file: URL) {
        clear(); self.file = file
        let item = AVPlayerItem(url: file)
        item.preferredPeakBitRate = 8_000_000
        let queue = AVQueuePlayer()
        queue.isMuted = true; queue.volume = 0
        looper = AVPlayerLooper(player: queue, templateItem: item)
        player = queue
        if shouldPlay { queue.play() }
    }

    func observe(_ layer: AVPlayerLayer) {
        let token = generation
        observation = layer.observe(\.isReadyForDisplay, options: [.initial, .new]) { [weak self] layer, _ in
            let value = layer.isReadyForDisplay
            Task { @MainActor [weak self] in
                guard self?.generation == token else { return }
                self?.ready = value
            }
        }
    }
}

private struct MotionArtworkLayer: NSViewRepresentable {
    let controller: ExperimentalMotionController
    func makeNSView(context: Context) -> NSView {
        let view = NSView(); view.wantsLayer = true
        let layer = AVPlayerLayer(player: controller.player); layer.videoGravity = .resizeAspectFill
        view.layer = layer; controller.observe(layer)
        return view
    }
    func updateNSView(_ view: NSView, context: Context) { (view.layer as? AVPlayerLayer)?.player = controller.player }
}

private struct MotionArtworkVisibility: NSViewRepresentable {
    let changed: @MainActor (Bool) -> Void
    final class Probe: NSView {
        var changed: (@MainActor (Bool) -> Void)?
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            NotificationCenter.default.removeObserver(self)
            for name in [NSWindow.didChangeOcclusionStateNotification, NSWindow.didMiniaturizeNotification,
                         NSWindow.didDeminiaturizeNotification, NSApplication.didHideNotification,
                         NSApplication.didUnhideNotification] {
                NotificationCenter.default.addObserver(self, selector: #selector(updateVisibility), name: name, object: nil)
            }
            updateVisibility()
        }
        @objc private func updateVisibility() {
            Task { @MainActor [weak self] in
                guard let self else { return }
                changed?(window?.isVisible == true && window?.isMiniaturized == false
                         && window?.occlusionState.contains(.visible) == true && !NSApp.isHidden)
            }
        }
    }
    func makeNSView(context: Context) -> Probe {
        let view = Probe(); view.changed = changed; return view
    }
    func updateNSView(_ view: Probe, context: Context) { view.changed = changed }
}

struct ExperimentalMotionArtwork: View {
    let query: MotionArtworkQuery?
    let playing: Bool
    let enabled: Bool
    @Environment(\.goosicReduceMotion) private var reduceMotion
    @StateObject private var controller = ExperimentalMotionController()
    @SwiftUI.State private var visible: Bool = false
    @SwiftUI.State private var windowVisible: Bool = false

    private var active: Bool { enabled && !reduceMotion && visible && windowVisible  }
    private var request: MotionArtworkQuery? { active ? query : nil }

    var body: some View {
        ZStack {
            Color.clear
            if controller.player != nil {
                MotionArtworkLayer(controller: controller).opacity(controller.ready ? 1 : 0)
            }
        }
        .allowsHitTesting(false).accessibilityHidden(true)
        .onAppear { visible = true }
        .onDisappear { visible = false; controller.clear() }
        .background { MotionArtworkVisibility { windowVisible = $0 } }
        .task(id: request) {
            controller.clear()
            guard let request else { return }
            do {
                guard let file = try await ExperimentalMotionResolver.shared.resolve(request) else { return }
                guard !Task.isCancelled else { try? FileManager.default.removeItem(at: file); return }
                controller.install(file)
            } catch is CancellationError { } catch {
                if !Task.isCancelled { fputs("Experimental motion artwork unavailable; keeping still cover.\n", stderr) }
            }
        }
        .onChange(of: playing, initial: true) { _, value in controller.setPlaying(value) }
    }
}
#endif
