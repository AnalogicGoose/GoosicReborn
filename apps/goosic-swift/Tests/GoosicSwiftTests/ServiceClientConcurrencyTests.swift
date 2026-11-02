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
        let body = """
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
        """

        #if os(Windows)
        // Windows has no shebang. An extensionless text file is not executable there whatever
        // its permissions say, so the interpreter the first line would have chosen is named by a
        // launcher instead, and that is what the client is given to run.
        let script = directory.appendingPathComponent("stub-service.py")
        try body.write(to: script, atomically: true, encoding: .utf8)
        let launcher = directory.appendingPathComponent("stub-service.cmd")
        try """
        @echo off
        python "%~dp0stub-service.py" %*
        """.write(to: launcher, atomically: true, encoding: .utf8)
        return launcher.path
        #else
        let script = directory.appendingPathComponent("stub-service")
        try body.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return script.path
        #endif
    }

    /// The failure this exists for: a catalog browse waiting on a third-party host used to hold
    /// every later command behind it, so pause and seek did nothing until it finished or the
    /// client killed the service out from under playback.
    func testASlowRequestDoesNotDelayTheCommandsBehindIt() throws {
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 2))

        let slowAnswered = expectation(description: "the slow request is eventually answered")
        let fastAnswered = expectation(description: "the fast request is answered")
        let started = Date()

        client.send(command: "slow.browse") { result in
            if case .failure(let error) = result { XCTFail("slow request failed: \(error)") }
            slowAnswered.fulfill()
        }
        // Timed and judged inside the answer rather than through a variable the test reads
        // afterwards: that variable is written on the transport's thread and read on this one,
        // which is a data race whatever the timing makes of it.
        client.send(command: "playback.sample") { result in
            if case .failure(let error) = result { XCTFail("fast request failed: \(error)") }
            let elapsed = Date().timeIntervalSince(started)
            XCTAssertLessThan(
                elapsed, 1,
                "the transport command waited \(elapsed)s behind a 2s read instead of being answered at once"
            )
            fastAnswered.fulfill()
        }

        wait(for: [fastAnswered], timeout: 5)
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

    /// The model drops a client on reconnect. Its read loop keeps it alive while the child runs,
    /// so without an explicit close the old service outlived its replacement.
    func testClosingTheClientEndsTheServiceAndAnswersWaitingRequests() throws {
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 8))

        let failed = expectation(description: "the waiting request is answered")
        client.send(command: "slow.stalled") { result in
            guard case .failure(ServiceClientError.unavailable) = result else {
                return XCTFail("expected the closed client to fail the request, got \(result)")
            }
            failed.fulfill()
        }
        client.close()
        wait(for: [failed], timeout: 2)

        let deadline = Date().addingTimeInterval(5)
        while client.isServiceRunning, Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertFalse(client.isServiceRunning, "the service outlived the client that was closed")

        let refused = expectation(description: "a later request is refused at once")
        client.send(command: "state.get") { result in
            if case .success = result { XCTFail("a closed client sent a request") }
            refused.fulfill()
        }
        wait(for: [refused], timeout: 2)
    }

    #if !os(Windows)
    /// A request written to a service that can no longer read used to raise SIGPIPE, which kills
    /// the whole app rather than failing the request. `close()` made that race routine.
    func testWritingToAServiceThatStoppedReadingFailsOnlyTheRequest() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("goosic-stub-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        stubDirectory = directory
        // Closes its end of the request pipe but keeps running, so nothing reports end of file.
        let script = directory.appendingPathComponent("deaf-service")
        try "#!/bin/sh\nexec 0<&-\nsleep 5\n".write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        let client = try GoosicServiceClient(executablePath: script.path)
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))

        let failed = expectation(description: "the unwritable request fails")
        client.send(command: "state.get") { result in
            if case .success = result { XCTFail("a request reached a service that cannot read") }
            failed.fulfill()
        }
        wait(for: [failed], timeout: 3)
        client.close()
    }
    #endif

    #if canImport(Darwin)
    /// A read loop that never returns never reaches the pool its queue would drain, so each pipe
    /// read used to leave a 16 KB buffer behind for as long as the service ran: past 700 MB in an
    /// afternoon of playback, reachable the whole time, so a leak scan saw nothing. Measured while
    /// the connection is open, because closing it would release exactly what is being looked for.
    func testAnsweredRequestsDoNotAccumulateMemoryWhileTheServiceRuns() throws {
        let client = try GoosicServiceClient(executablePath: try makeStubService(delay: 0))
        defer { client.close() }

        // One at a time, so each answer is a read of its own, as a playback sample is.
        func exchange(_ count: Int) {
            for _ in 0..<count {
                let answered = expectation(description: "answered")
                client.send(command: "playback.sample") { _ in answered.fulfill() }
                wait(for: [answered], timeout: 5)
            }
        }

        exchange(100)
        let before = try Self.physicalFootprint()
        exchange(1_500)
        let growth = try Self.physicalFootprint() - before

        // The defect costs at least 24 MB across these reads. The margin is for the test's own
        // allocations.
        XCTAssertLessThan(growth, 8 * 1024 * 1024, "the read loop retained \(growth) bytes")
    }

    private static func physicalFootprint() throws -> Int64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let status = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        guard status == KERN_SUCCESS else { throw XCTSkip("task_info failed: \(status)") }
        return Int64(info.phys_footprint)
    }
    #endif

    /// Only a broken stream ends the connection. The model used to drop Rust's authority on any
    /// failure but a rejection, so one slow catalog read stopped tracking audio that kept playing.
    func testOnlyTransportFailuresEndTheConnection() {
        let rejection = ServiceClientError.remote(GoosicError(code: "invalidRequest", message: "no"))
        XCTAssertFalse(ServiceClientError.invalidatesConnection(rejection))
        XCTAssertFalse(ServiceClientError.invalidatesConnection(ServiceClientError.timedOut))
        XCTAssertTrue(ServiceClientError.invalidatesConnection(ServiceClientError.endOfFile))
        XCTAssertTrue(ServiceClientError.invalidatesConnection(ServiceClientError.processExited(1)))
        XCTAssertTrue(ServiceClientError.invalidatesConnection(ServiceClientError.invalidResponse))
        XCTAssertTrue(ServiceClientError.invalidatesConnection(ServiceClientError.unavailable("gone")))
    }
}
