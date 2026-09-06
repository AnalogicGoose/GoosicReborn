import Foundation

enum ServiceClientError: LocalizedError {
    case unavailable(String)
    case invalidResponse
    case protocolVersionMismatch(expected: String, actual: String)
    case requestIDMismatch(expected: String, actual: String)
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
        case .requestIDMismatch(let expected, let actual):
            return "Response request ID mismatch (expected \(expected), got \(actual))."
        case .remote(let error): return "\(error.code): \(error.message)"
        case .timedOut: return "The service did not respond before the timeout."
        case .processExited(let status): return "goosic-service exited with status \(status)."
        case .endOfFile: return "goosic-service closed its output."
        case .responseTooLarge: return "The service response exceeded the frame limit."
        }
    }
}

/// Serial, asynchronous request/response client for the Rust NDJSON authority.
/// All pipe I/O occurs off the UI thread and each response has a bounded wait.
///
/// `@unchecked` because the guarantee is the serial `requestQueue`, not the type: every mutable
/// field is read and written only from inside it. The compiler cannot see that, so touching this
/// state from anywhere else silently breaks the claim.
final class GoosicServiceClient: @unchecked Sendable {
    /// Catalog pages are the only large responses; the service clamps a page well below this.
    private static let maxFrameBytes = 256 * 1024
    private static let responseTimeout: TimeInterval = 5
    /// Catalog commands reach a third-party service, so they get a wait long enough to cover the
    /// service's own upstream timeout instead of tearing down the child process mid-request.
    private static let catalogResponseTimeout: TimeInterval = 20
    /// A first play may decode a full WebM/Opus file into the local WAV cache. That work is
    /// intentionally off the UI thread but can exceed the ordinary command timeout on large
    /// tracks; later plays reuse the cache and return immediately.
    private static let downloadPreparationTimeout: TimeInterval = 120

    private static func timeout(for command: String) -> TimeInterval {
        if command == "downloads.prepare" { return downloadPreparationTimeout }
        return command.hasPrefix("catalog.") ? catalogResponseTimeout : responseTimeout
    }
    private let process: Process
    private let input: FileHandle
    private let output: FileHandle
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private let requestQueue = DispatchQueue(label: "com.goosic.service-client")
    private var requestNumber: UInt64 = 0
    private var pendingOutput = Data()
    private var invalidated = false

    init() throws {
        let configuredPath = ProcessInfo.processInfo.environment["GOOSIC_SERVICE_PATH"] ?? "goosic-service"
        process = Process()
        if configuredPath.contains("/") {
            process.executableURL = URL(fileURLWithPath: configuredPath)
            process.arguments = []
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [configuredPath]
        }

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
    }

    deinit {
        invalidateProcess()
    }

    func send(
        command: String,
        payload: GoosicRequestPayload = .init(),
        completion: @escaping @MainActor @Sendable (Result<GoosicResponse, Error>) -> Void
    ) {
        requestQueue.async { [self] in
            let result: Result<GoosicResponse, Error>
            do {
                guard !invalidated else {
                    throw ServiceClientError.unavailable("goosic-service is no longer available.")
                }
                requestNumber += 1
                let requestID = "swift-\(requestNumber)"
                let request = GoosicRequest(
                    requestId: requestID,
                    command: command,
                    payload: payload
                )
                var data = try encoder.encode(request)
                data.append(0x0A)
                try input.write(contentsOf: data)
                let response = try readResponse(requestID: requestID, timeout: Self.timeout(for: command))
                result = .success(response)
            } catch {
                result = .failure(error)
            }
            DispatchQueue.main.async {
                MainActor.assumeIsolated { completion(result) }
            }
        }
    }

    private func readResponse(requestID: String, timeout: TimeInterval) throws -> GoosicResponse {
        let deadline = Date().addingTimeInterval(timeout)
        while true {
            if let newline = pendingOutput.firstIndex(of: 0x0A) {
                // `newline` is zero-based, so include the delimiter in the frame limit.
                if newline >= Self.maxFrameBytes {
                    invalidateProcess()
                    throw ServiceClientError.responseTooLarge
                }
                let line = Data(pendingOutput[..<newline])
                pendingOutput.removeSubrange(...newline)
                let response: GoosicResponse
                do {
                    response = try decoder.decode(GoosicResponse.self, from: line)
                } catch {
                    invalidateProcess()
                    throw ServiceClientError.invalidResponse
                }
                guard response.protocolVersion == goosicProtocolVersion else {
                    let actual = response.protocolVersion
                    invalidateProcess()
                    throw ServiceClientError.protocolVersionMismatch(
                        expected: goosicProtocolVersion,
                        actual: actual
                    )
                }
                guard response.requestId == requestID else {
                    let actual = response.requestId
                    invalidateProcess()
                    throw ServiceClientError.requestIDMismatch(
                        expected: requestID,
                        actual: actual
                    )
                }
                guard response.ok else {
                    throw ServiceClientError.remote(
                        response.error ?? GoosicError(
                            code: "serviceFailure",
                            message: "unknown service error"
                        )
                    )
                }
                return response
            }
            if pendingOutput.count > Self.maxFrameBytes {
                invalidateProcess()
                throw ServiceClientError.responseTooLarge
            }
            guard process.isRunning else {
                throw ServiceClientError.processExited(process.terminationStatus)
            }
            let remaining = deadline.timeIntervalSinceNow
            if remaining <= 0 {
                invalidateProcess()
                throw ServiceClientError.timedOut
            }
            try waitForOutput(timeout: remaining)
            // `read(upToCount:)` may wait for the requested count on a pipe even after the
            // pipe became readable. Read only the bytes currently available so a short NDJSON
            // response can be parsed immediately; frame-size enforcement remains below.
            let chunk = output.availableData
            if chunk.isEmpty {
                invalidateProcess()
                throw ServiceClientError.endOfFile
            }
            pendingOutput.append(chunk)
        }
    }

    private func waitForOutput(timeout: TimeInterval) throws {
        let ready = DispatchSemaphore(value: 0)
        output.readabilityHandler = { _ in ready.signal() }
        defer { output.readabilityHandler = nil }
        let milliseconds = max(1, Int(timeout * 1_000))
        if ready.wait(timeout: .now() + .milliseconds(milliseconds)) == .timedOut {
            invalidateProcess()
            throw ServiceClientError.timedOut
        }
    }

    /// A timed-out or closed child cannot safely be reused: close both ends and terminate it so a
    /// blocked pipe reader cannot race a later request or leave an orphan service process.
    private func invalidateProcess() {
        invalidated = true
        output.readabilityHandler = nil
        try? input.close()
        try? output.close()
        if process.isRunning { process.terminate() }
    }
}
