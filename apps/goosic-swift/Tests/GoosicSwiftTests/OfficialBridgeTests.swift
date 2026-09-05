import XCTest

@testable import GoosicSwift

/// The observer script and `OfficialBridge.Event` are one contract split across two languages.
/// Nothing but these tests notices when a rename on the JavaScript side stops matching the
/// Swift side, because the mismatch shows up as a silent decode failure at runtime.
final class OfficialBridgeWireTests: XCTestCase {
    /// Exactly the shape `observerScript` posts.
    private static func payload(
        token: String = "token",
        generation: UInt64 = 7,
        videoId: String = "dQw4w9WgXcQ",
        sequence: UInt64 = 1,
        advertisement: Bool = false
    ) -> Data {
        Data("""
        {"version":2,"token":"\(token)","generation":\(generation),"videoId":"\(videoId)",
         "sequence":\(sequence),"state":"playing","currentTime":12.5,"duration":214,
         "isAdvertisement":\(advertisement),"volume":0.8,"muted":false}
        """.utf8)
    }

    private func decoded(_ data: Data) throws -> OfficialBridge.Event {
        try JSONDecoder().decode(OfficialBridge.Event.self, from: data)
    }

    func testTheObserverPayloadDecodesIntoTheEventTheHostsExpect() throws {
        let event = try decoded(Self.payload())
        XCTAssertEqual(event.version, 2)
        // The JavaScript writes `videoId`; Swift reads `videoID`. The mapping is the contract.
        XCTAssertEqual(event.videoID, "dQw4w9WgXcQ")
        XCTAssertEqual(event.generation, 7)
        XCTAssertEqual(event.currentTime, 12.5)
        XCTAssertFalse(event.muted)
    }

    func testAnEventMatchingTheActiveLoadIsAccepted() throws {
        let event = try decoded(Self.payload())
        XCTAssertNil(OfficialBridge.rejectionReason(
            for: event, expectedToken: "token", expectedGeneration: 7,
            expectedVideoID: "dQw4w9WgXcQ", lastSequence: 0
        ))
    }

    func testAnEventFromASupersededDocumentIsRefused() throws {
        let event = try decoded(Self.payload(token: "stale"))
        XCTAssertEqual(OfficialBridge.rejectionReason(
            for: event, expectedToken: "token", expectedGeneration: 7,
            expectedVideoID: "dQw4w9WgXcQ", lastSequence: 0
        ), "it came from a superseded document")
    }

    func testAnEventCarryingAnotherLeaseIsRefused() throws {
        let event = try decoded(Self.payload(generation: 6))
        XCTAssertEqual(OfficialBridge.rejectionReason(
            for: event, expectedToken: "token", expectedGeneration: 7,
            expectedVideoID: "dQw4w9WgXcQ", lastSequence: 0
        ), "generation 6 is not the active lease")
    }

    func testASequenceThatDidNotAdvanceIsRefused() throws {
        let event = try decoded(Self.payload(sequence: 4))
        XCTAssertEqual(OfficialBridge.rejectionReason(
            for: event, expectedToken: "token", expectedGeneration: 7,
            expectedVideoID: "dQw4w9WgXcQ", lastSequence: 4
        ), "sequence 4 did not advance past 4")
    }

    /// The observer only ever posts to a handler it can see, and on Linux that handler lives in
    /// a script world the page cannot reach. Both sides have to agree on the name for the
    /// isolation to mean anything.
    func testTheObserverPostsToTheHandlerNameBothHostsRegister() {
        let script = OfficialBridge.observerScript(token: "t", generation: 1, videoID: "v")
        XCTAssertTrue(script.contains("window.webkit.messageHandlers.\(OfficialBridge.name).postMessage"))
    }

    func testTheObserverCarriesTheIdentityOfItsOwnLoad() {
        let script = OfficialBridge.observerScript(token: "abc", generation: 42, videoID: "xyz")
        XCTAssertTrue(script.contains("const generation = 42;"))
        XCTAssertTrue(script.contains("\"abc\""))
        XCTAssertTrue(script.contains("\"xyz\""))
    }
}