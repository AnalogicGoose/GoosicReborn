import Foundation

enum ServiceClientError: LocalizedError {
    case unavailable(String)
    case invalidResponse
    case protocolVersionMismatch(expected: String, actual: String)
    case remote(GoosicError)
    case timedOut
    case processExited(Int32)
    case endOfFile
    case responseTooLarge

    var errorDescription: String? {
        switch self {
        case .unavailable(let message): return message
        case .invalidResponse: return "The service returned an invalid protocol response."
        case .protocolVersionMismatch(let expected, let actual):
            return "Protocol version mismatch (expected \(expected), got \(actual))."
        case .remote(let error): return "\(error.code): \(error.message)"
        case .timedOut: return "The service did not respond before the timeout."
        case .processExited(let status): return "goosic-service exited with status \(status)."
        case .endOfFile: return "goosic-service closed its output."
        case .responseTooLarge: return "The service response exceeded the frame limit."
        }
    }
}

/// Request/response coordinator for the Rust NDJSON authority.
///
/// Several requests may be outstanding at once and each completes on its own. That is not a
/// throughput optimisation; it is what keeps the transport controls working. This client used to
/// write a request and then block its one queue reading until that request's answer arrived, so a
/// catalog browse reaching a third-party service held every later command behind it for as long as
/// twenty seconds — pause, seek, and release included. Requests are matched by request id, so the
/// order answers arrive in does not matter and no longer has to.
///
/// A timeout now fails one request rather than tearing down the child process. The old behaviour
/// had to kill the service, because a reader blocked on an answer that never came could not be
/// recovered any other way; that meant a single slow catalog read could take playback down with
/// it. Teardown is reserved for the failures that genuinely corrupt the stream: an undecodable
/// line, a protocol version mismatch, an oversized frame, or end of file.
///
/// `@unchecked` because the guarantee is the `lock`, not the type: every mutable field below is
/// read and written only while it is held, and no I/O is performed inside it. The compiler cannot
/// see that, so touching this state from anywhere else silently breaks the claim.
final class GoosicServiceClient: @unchecked Sendable {
    private static let responseTimeout: TimeInterval = 5
    /// Catalog commands reach a third-party service, so they get a wait long enough to cover the
    /// service's own upstream timeout instead of failing a request that is still coming.
    private static let catalogResponseTimeout: TimeInterval = 20
    /// A first play may decode a full WebM/Opus file into the local WAV cache. That work is
    /// intentionally off the UI thread but can exceed the ordinary command timeout on large
    /// tracks; later plays reuse the cache and return immediately.
    private static let downloadPreparationTimeout: TimeInterval = 120

    private static func timeout(for command: String) -> TimeInterval {
        if command == "downloads.prepare" { return downloadPreparationTimeout }
        if command.hasPrefix("catalog.") || command.hasPrefix("lyrics.") {
            return catalogResponseTimeout
        }
        return responseTimeout
    }

    private typealias Completion = @MainActor @Sendable (Result<GoosicResponse, Error>) -> Void

    private let process: Process
    private let input: FileHandle
    private let output: FileHandle
    private let encoder = JSONEncoder()
    /// Writes are serialised here rather than under `lock`, so a full pipe buffer cannot stall a
    /// completion that only needs to look at the pending table.
    private let writeQueue = DispatchQueue(label: "com.goosic.service-client.write")
    private let readQueue = DispatchQueue(label: "com.goosic.service-client.read")
    private let timerQueue = DispatchQueue(label: "com.goosic.service-client.timeout")

    private let lock = NSLock()
    private var requestNumber: UInt64 = 0
    private var pending: [String: Completion] = [:]
    private var frames = ServiceFrameReader()
    private var invalidated = false

    /// `executablePath` exists for tests, which need a service whose timing they control: the
    /// property worth proving is that a slow answer does not delay a fast one, and the real
    /// service is only slow when a third-party host is, which is not something a test may depend
    /// on. Production passes nothing and the environment decides, as before.
    init(executablePath: String? = nil) throws {
        let configuredPath = executablePath
            ?? ProcessInfo.processInfo.environment["GOOSIC_SERVICE_PATH"]
            ?? "goosic-service"
        process = Process()
        let launch = ServiceLaunch.plan(for: configuredPath)
        process.executableURL = launch.url
        process.arguments = launch.arguments

        let stdin = Pipe()
        let stdout = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = FileHandle.standardError
        do {
            try process.run()
        } catch {
            throw ServiceClientError.unavailable("Could not launch goosic-service: \(error.localizedDescription)")
        }
        input = stdin.fileHandleForWriting
        output = stdout.fileHandleForReading
        startReading()
    }

    deinit {
        invalidate(with: ServiceClientError.unavailable("goosic-service is no longer available."))
    }

    func send(
        command: String,
        payload: GoosicRequestPayload = .init(),
        completion: @escaping @MainActor @Sendable (Result<GoosicResponse, Error>) -> Void
    ) {
        let requestID: String
        lock.lock()
        if invalidated {
            lock.unlock()
            finish(completion, .failure(ServiceClientError.unavailable("goosic-service is no longer available.")))
            return
        }
        requestNumber += 1
        requestID = "swift-\(requestNumber)"
        let started = Date()
        // Wrapped rather than logged at the call sites: how long a command took is a property of
        // the request, and there is exactly one place that knows both ends of it.
        pending[requestID] = { result in
            switch result {
            case .success:
                Diagnostics.note(.service, "answered", [
                    "command": command, "elapsed": Diagnostics.milliseconds(since: started),
                ])
            case .failure(let error):
                Diagnostics.note(.service, "failed", [
                    "command": command, "elapsed": Diagnostics.milliseconds(since: started),
                    "reason": Diagnostics.reason(error),
                ])
            }
            completion(result)
        }
        lock.unlock()

        timerQueue.asyncAfter(deadline: .now() + Self.timeout(for: command)) { [weak self] in
            // A request still in the table at its deadline gets an answer of its own. The service
            // keeps running: one read that took too long says nothing about the next command.
            self?.fail(requestID, with: ServiceClientError.timedOut)
        }

        writeQueue.async { [self] in
            do {
                let request = GoosicRequest(requestId: requestID, command: command, payload: payload)
                var data = try encoder.encode(request)
                data.append(0x0A)
                try input.write(contentsOf: data)
            } catch {
                fail(requestID, with: error)
            }
        }
    }

    // MARK: - Reading

    private func startReading() {
        readQueue.async { [self] in
            while true {
                let chunk = output.availableData
                if chunk.isEmpty {
                    // A closed pipe is the end of the conversation, whether the child exited or
                    // the handle was closed underneath us.
                    let status = process.isRunning ? nil : process.terminationStatus
                    invalidate(with: status.map(ServiceClientError.processExited) ?? ServiceClientError.endOfFile)
                    return
                }
                lock.lock()
                frames.append(chunk)
                lock.unlock()
                if !drainFrames() { return }
            }
        }
    }

    /// Returns false once the stream has been invalidated and reading should stop.
    private func drainFrames() -> Bool {
        while true {
            lock.lock()
            if invalidated {
                lock.unlock()
                return false
            }
            let frame: GoosicResponse?
            do {
                frame = try frames.next()
            } catch {
                lock.unlock()
                invalidate(with: error)
                return false
            }
            lock.unlock()
            // One read can carry several responses, so this keeps taking them until the buffer
            // holds only a partial frame.
            guard let frame else { return true }
            deliver(frame)
        }
    }

    private func deliver(_ response: GoosicResponse) {
        lock.lock()
        // A response with no waiting request is one whose caller already gave up. Dropping it is
        // correct; it is not evidence the stream is corrupt, and treating it as such is how a
        // single timeout used to take the service down.
        guard let completion = pending.removeValue(forKey: response.requestId) else {
            lock.unlock()
            return
        }
        lock.unlock()
        guard response.ok else {
            finish(completion, .failure(ServiceClientError.remote(
                response.error ?? GoosicError(code: "serviceFailure", message: "unknown service error")
            )))
            return
        }
        finish(completion, .success(response))
    }

    // MARK: - Failing

    private func fail(_ requestID: String, with error: Error) {
        lock.lock()
        let completion = pending.removeValue(forKey: requestID)
        lock.unlock()
        guard let completion else { return }
        finish(completion, .failure(error))
    }

    /// The stream cannot be trusted any further: close both ends, terminate the child so a blocked
    /// reader cannot race a later request or leave an orphan process, and give every outstanding
    /// request the same answer rather than leaving it waiting for a service that is gone.
    private func invalidate(with error: Error) {
        lock.lock()
        if invalidated {
            lock.unlock()
            return
        }
        invalidated = true
        let outstanding = pending
        pending.removeAll()
        lock.unlock()

        try? input.close()
        try? output.close()
        if process.isRunning { process.terminate() }
        for completion in outstanding.values { finish(completion, .failure(error)) }
    }

    private func finish(_ completion: @escaping Completion, _ result: Result<GoosicResponse, Error>) {
        DispatchQueue.main.async {
            MainActor.assumeIsolated { completion(result) }
        }
    }
}
