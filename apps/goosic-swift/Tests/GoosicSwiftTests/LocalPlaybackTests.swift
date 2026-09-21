#if os(Linux)
import CGStreamer
import XCTest

@testable import GoosicSwift

/// These run the real decoder. `prepare` leaves the pipeline paused, which decodes far enough to
/// know the duration without opening the audio device, so the test is silent and needs no
/// display — the only part of the Linux port that can be checked end to end without a person
/// listening to it.
///
/// The isolation goes inside a `MainActor.run` rather than on the test methods. A `@MainActor`
/// test method is a signature swift-corelibs-xctest cannot cast during discovery, and it aborts
/// the whole run rather than failing that one test.
final class LocalPlaybackHostTests: XCTestCase {
    /// 8 kHz mono 8-bit silence: a real RIFF file small enough to write inline.
    private static func writeSilentWAV(seconds: Int) throws -> String {
        let rate = 8000
        let samples = rate * seconds
        var file = Data()
        func ascii(_ text: String) { file.append(contentsOf: Array(text.utf8)) }
        func u32(_ value: Int) { withUnsafeBytes(of: UInt32(value).littleEndian) { file.append(contentsOf: $0) } }
        func u16(_ value: Int) { withUnsafeBytes(of: UInt16(value).littleEndian) { file.append(contentsOf: $0) } }
        ascii("RIFF"); u32(36 + samples); ascii("WAVE")
        ascii("fmt "); u32(16); u16(1); u16(1); u32(rate); u32(rate); u16(1); u16(8)
        // Unsigned 8-bit PCM puts silence at 128, not at zero.
        ascii("data"); u32(samples)
        file.append(Data(repeating: 128, count: samples))

        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("goosic-local-\(UUID().uuidString).wav").path
        try file.write(to: URL(fileURLWithPath: path))
        return path
    }

    private static func requirePlaybin() throws {
        gst_init(nil, nil)
        let element = gst_element_factory_make("playbin3", nil) ?? gst_element_factory_make("playbin", nil)
        guard let element else {
            throw XCTSkip("GStreamer's playbin is not installed here, so there is nothing to decode with")
        }
        gst_object_unref(UnsafeMutableRawPointer(element))
    }

    func testAPathThatIsNotOnDiskIsRefused() async throws {
        try await MainActor.run {
            let host = LocalPlaybackHost()
            XCTAssertThrowsError(
                try host.prepare(
                    localFile: "/nonexistent/goosic/decoded.wav",
                    videoID: "dQw4w9WgXcQ", generation: 1
                )
            )
            XCTAssertFalse(host.isLoaded)
            XCTAssertNil(host.loadedVideoID)
        }
    }

    func testARealFileIsOpenedAndItsDurationBecomesKnown() async throws {
        try Self.requirePlaybin()
        let path = try Self.writeSilentWAV(seconds: 2)
        defer { try? FileManager.default.removeItem(atPath: path) }

        try await MainActor.run {
            let host = LocalPlaybackHost()
            var last: LocalPlaybackEvent?
            host.onEvent = { last = $0 }
            try host.prepare(localFile: path, videoID: "dQw4w9WgXcQ", generation: 7)
            XCTAssertTrue(host.isLoaded)
            XCTAssertEqual(host.loadedVideoID, "dQw4w9WgXcQ")

            // Prerolling is asynchronous, so the duration is not known the instant the file
            // opens. `pause` emits a sample each time, which also exercises the emit path.
            var resolved = false
            for _ in 0..<60 {
                host.pause()
                if let last, last.duration > 0 { resolved = true; break }
                usleep(50_000)
            }
            XCTAssertTrue(resolved, "the decoder never reported a duration")
            let event = try XCTUnwrap(last)
            XCTAssertEqual(event.duration, 2, accuracy: 0.25)
            XCTAssertEqual(event.generation, 7)
            XCTAssertEqual(event.videoID, "dQw4w9WgXcQ")
            XCTAssertEqual(event.state, "paused")
            host.stop()
            XCTAssertFalse(host.isLoaded)
        }
    }

    func testSamplesNeverRepeatASequenceWithinOneGeneration() async throws {
        try Self.requirePlaybin()
        let path = try Self.writeSilentWAV(seconds: 1)
        defer { try? FileManager.default.removeItem(atPath: path) }

        try await MainActor.run {
            let host = LocalPlaybackHost()
            var sequences: [UInt64] = []
            host.onEvent = { sequences.append($0.sequence) }
            try host.prepare(localFile: path, videoID: "dQw4w9WgXcQ", generation: 3)
            for _ in 0..<5 { host.pause() }

            XCTAssertEqual(sequences, sequences.sorted())
            XCTAssertEqual(Set(sequences).count, sequences.count, "a sequence was reused")

            // A replacement inside the same generation must continue the counter, because Rust
            // rejects a sample that does not advance past the last one it accepted.
            let carried = sequences.last ?? 0
            host.stopForReplacement()
            try host.prepare(localFile: path, videoID: "oHg5SJYRHA0", generation: 3)
            host.pause()
            XCTAssertGreaterThan(try XCTUnwrap(sequences.last), carried)
            host.stop()
        }
    }
}
#endif
