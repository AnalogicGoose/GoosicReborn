import XCTest

@testable import GoosicSwift

/// The client's reason for existing in its current shape is that a slow answer must not delay a
/// fast one. Proving that needs a service whose timing the test controls: the real service is slow
/// only when a third-party host is, and a test may not depend on that. So these run against a stub
/// that speaks the same NDJSON protocol and sleeps exactly when asked to.
///
/// What is under test is the client, not the stub. The stub answers out of order on purpose,
/// because that is what the real service does now that catalog and lyrics reads are answered off
/// its main loop.
final class ServiceClientConcurrencyTests: XCTestCase {
    private var stubDirectory: URL?

    override func tearDownWithError() throws {
        if let stubDirectory { try? FileManager.default.removeItem(at: stubDirectory) }
        stubDirectory = nil
    }

    /// A service that answers `slow.*` after a delay and everything else at once, each on its own
    /// thread, so a delayed answer holds nothing behind it.
    private func makeStubService(delay: Double) throws -> String {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("goosic-stub-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        stubDirectory = directory
        let script = directory.appendingPathComponent("stub-service")
        try """
        #!/usr/bin/env python3
        import json, sys, threading, time

        VERSION = "\(goosicProtocolVersion)"
        write = threading.Lock()

        def answer(request):
            if request["command"].startswith("slow."):
                time.sleep(\(delay))
            line = json.dumps({
                "protocolVersion": VERSION,
                "requestId": request["requestId"],
                "ok": True,
                "payload": {"message": request["command"]},
            })
            with write:
                sys.stdout.write(line + "\\n")
                sys.stdout.flush()

        threads = []
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            thread = threading.Thread(target=answer, args=(json.loads(line),))
            thread.start()
            threads.append(thread)
        for thread in threads:
            thread.join()
        """.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return script.path
    }

    /// The failure this exists for: a catalog browse waiting on a third-party host used to hold
    /// every later command behind it, so pause and seek did nothing until it finished or the
    /// client killed the service out from under playback.
    func testASlowRequestDoesNotDelayTheCommandsBehindIt() throws {
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 2))

        let slowAnswered = expectation(description: "the slow request is eventually answered")
        let fastAnswered = expectation(description: "the fast request is answered")
        let started = Date()
        nonisolated(unsafe) var fastElapsed: TimeInterval?

        client.send(command: "slow.browse") { result in
            if case .failure(let error) = result { XCTFail("slow request failed: \(error)") }
            slowAnswered.fulfill()
        }
        client.send(command: "playback.sample") { result in
            if case .failure(let error) = result { XCTFail("fast request failed: \(error)") }
            fastElapsed = Date().timeIntervalSince(started)
            fastAnswered.fulfill()
        }

        wait(for: [fastAnswered], timeout: 5)
        let elapsed = try XCTUnwrap(fastElapsed)
        XCTAssertLessThan(
            elapsed, 1,
            "the transport command waited \(elapsed)s behind a 2s read instead of being answered at once"
        )
        wait(for: [slowAnswered], timeout: 10)
    }

    /// Answers are matched by request id, which is what makes the order they arrive in irrelevant.
    func testEveryOutstandingRequestGetsItsOwnAnswer() throws {
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 0.4))

        let commands = ["slow.a", "playback.claim", "slow.b", "state.get", "slow.c"]
        let answered = commands.map { expectation(description: "answered \($0)") }
        for (index, command) in commands.enumerated() {
            client.send(command: command) { result in
                switch result {
                case .success(let response):
                    // The stub echoes the command it was asked for, so a response delivered to the
                    // wrong completion is visible rather than merely miscounted.
                    XCTAssertEqual(response.payload?.message, command)
                case .failure(let error):
                    XCTFail("\(command) failed: \(error)")
                }
                answered[index].fulfill()
            }
        }
        wait(for: answered, timeout: 10)
    }

    /// A request that outlives its deadline is failed on its own. It used to take the child
    /// process with it, which meant one unanswered catalog read stopped playback.
    func testATimedOutRequestDoesNotTakeTheServiceDown() throws {
        // Longer than the five-second timeout the client gives an ordinary command.
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 8))

        let timedOut = expectation(description: "the stalled request gives up")
        client.send(command: "slow.stalled") { result in
            guard case .failure(let error) = result, case ServiceClientError.timedOut = error else {
                return XCTFail("expected a timeout, got \(result)")
            }
            timedOut.fulfill()
        }
        wait(for: [timedOut], timeout: 15)

        let stillWorking = expectation(description: "the service still answers")
        client.send(command: "state.get") { result in
            if case .failure(let error) = result {
                XCTFail("the service was torn down by an unrelated timeout: \(error)")
            }
            stillWorking.fulfill()
        }
        wait(for: [stillWorking], timeout: 5)
    }
}
