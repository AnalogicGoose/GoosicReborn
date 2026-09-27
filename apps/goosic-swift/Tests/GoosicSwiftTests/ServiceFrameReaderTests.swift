import XCTest

@testable import GoosicSwift

/// A pipe read returns whatever bytes are available, which is unrelated to where responses begin
/// and end. These are the shapes that arrive in practice and the ones that used to have no cover
/// at all: the client's framing and its child process were one type, and a process cannot be asked
/// to split a response across two reads on cue.
final class ServiceFrameReaderTests: XCTestCase {
    private func line(id: String, ok: Bool = true) -> String {
        """
        {"protocolVersion":"\(goosicProtocolVersion)","requestId":"\(id)","ok":\(ok),\
        "payload":{}}
        """
    }

    private func data(_ string: String) -> Data { Data(string.utf8) }

    func testACompleteFrameIsReturned() throws {
        var reader = ServiceFrameReader()
        reader.append(data(line(id: "1") + "\n"))
        XCTAssertEqual(try reader.next()?.requestId, "1")
        XCTAssertNil(try reader.next())
    }

    /// The service answers concurrently now, so several responses genuinely do land in one read.
    /// Parsing one and leaving the rest in the buffer is a request that waits forever for an
    /// answer that already arrived.
    func testEveryResponseInOneReadIsReturned() throws {
        var reader = ServiceFrameReader()
        reader.append(data(line(id: "1") + "\n" + line(id: "2") + "\n" + line(id: "3") + "\n"))
        XCTAssertEqual(try reader.next()?.requestId, "1")
        XCTAssertEqual(try reader.next()?.requestId, "2")
        XCTAssertEqual(try reader.next()?.requestId, "3")
        XCTAssertNil(try reader.next())
    }

    func testAResponseSplitAcrossReadsIsHeldUntilItIsWhole() throws {
        var reader = ServiceFrameReader()
        let whole = Array(data(line(id: "42") + "\n"))
        for byte in whole.dropLast() {
            reader.append(Data([byte]))
            XCTAssertNil(try reader.next(), "a partial frame is not a response")
        }
        reader.append(Data([whole[whole.count - 1]]))
        XCTAssertEqual(try reader.next()?.requestId, "42")
    }

    func testATrailingPartialFrameSurvivesTheResponseBeforeIt() throws {
        var reader = ServiceFrameReader()
        reader.append(data(line(id: "1") + "\n" + #"{"protocolVersion":"#))
        XCTAssertEqual(try reader.next()?.requestId, "1")
        XCTAssertNil(try reader.next())
        reader.append(data(#""\#(goosicProtocolVersion)","requestId":"2","ok":true,"payload":{}}"# + "\n"))
        XCTAssertEqual(try reader.next()?.requestId, "2")
    }

    /// A remote failure is a well-formed frame. Throwing here would tear the process down over a
    /// request that simply did not succeed.
    func testAFailedResponseIsReturnedRatherThanThrown() throws {
        var reader = ServiceFrameReader()
        reader.append(data("""
        {"protocolVersion":"\(goosicProtocolVersion)","requestId":"7","ok":false,\
        "error":{"code":"nope","message":"no"}}
        """ + "\n"))
        let response = try reader.next()
        XCTAssertEqual(response?.requestId, "7")
        XCTAssertEqual(response?.ok, false)
    }

    func testAnUndecodableLineIsFatal() {
        var reader = ServiceFrameReader()
        reader.append(data("this is not json\n"))
        XCTAssertThrowsError(try reader.next()) { error in
            guard case ServiceClientError.invalidResponse = error else {
                return XCTFail("expected invalidResponse, got \(error)")
            }
        }
    }

    func testAMismatchedProtocolVersionIsFatal() {
        var reader = ServiceFrameReader()
        reader.append(data(#"{"protocolVersion":"0.0.1","requestId":"1","ok":true,"payload":{}}"# + "\n"))
        XCTAssertThrowsError(try reader.next()) { error in
            guard case ServiceClientError.protocolVersionMismatch = error else {
                return XCTFail("expected protocolVersionMismatch, got \(error)")
            }
        }
    }

    /// Without a delimiter there is nothing to parse, so the only defence against a stream that
    /// never frames again is the byte count.
    func testAFrameThatNeverEndsIsRefusedRatherThanBufferedForever() {
        var reader = ServiceFrameReader()
        reader.append(Data(repeating: 0x20, count: ServiceFrameReader.maxFrameBytes + 1))
        XCTAssertThrowsError(try reader.next()) { error in
            guard case ServiceClientError.responseTooLarge = error else {
                return XCTFail("expected responseTooLarge, got \(error)")
            }
        }
    }
}
