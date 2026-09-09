import Foundation

/// Turns the service's byte stream into whole protocol responses.
///
/// This is split out of `GoosicServiceClient` because it is the half that can be wrong in ways a
/// process cannot demonstrate on demand: a response arriving in three chunks, two responses in one
/// read, a frame that never ends, a line that is not JSON, a version that does not match. The half
/// that owns the child process cannot be asked to produce those on cue, so for as long as the two
/// were one type neither was tested.
///
/// A pipe read returns whatever bytes happen to be available, which has nothing to do with where
/// responses begin and end. Both halves of that mismatch are real: one read may carry a fragment of
/// a response, and one read may carry several complete ones. Handling only the first is the bug
/// where a screen waits forever for an answer that already arrived, sitting in a buffer behind a
/// response that was parsed and then forgotten.
struct ServiceFrameReader {
    /// A frame is one NDJSON line. Catalog pages are the only large responses and the service
    /// clamps a page well below this; anything past it means the stream is no longer framed.
    static let maxFrameBytes = 256 * 1024

    private var buffer = Data()

    mutating func append(_ chunk: Data) {
        buffer.append(chunk)
    }

    /// Returns the next complete response, or `nil` when more bytes are needed.
    ///
    /// Throwing means the stream itself is unusable and the process should be torn down — not
    /// that one request failed. A response that reports a remote error is a perfectly well-formed
    /// frame and is returned, not thrown.
    mutating func next() throws -> GoosicResponse? {
        guard let newline = buffer.firstIndex(of: 0x0A) else {
            // No delimiter yet. Bytes may still be arriving, unless there are already more of
            // them than any legitimate frame could hold.
            if buffer.count > Self.maxFrameBytes { throw ServiceClientError.responseTooLarge }
            return nil
        }
        // `firstIndex` is measured from the buffer's start index, which stays zero because whole
        // frames are removed rather than sliced around.
        let length = buffer.distance(from: buffer.startIndex, to: newline)
        if length >= Self.maxFrameBytes { throw ServiceClientError.responseTooLarge }
        let line = Data(buffer[buffer.startIndex..<newline])
        buffer.removeSubrange(buffer.startIndex...newline)

        guard let response = try? JSONDecoder().decode(GoosicResponse.self, from: line) else {
            throw ServiceClientError.invalidResponse
        }
        guard response.protocolVersion == goosicProtocolVersion else {
            throw ServiceClientError.protocolVersionMismatch(
                expected: goosicProtocolVersion,
                actual: response.protocolVersion
            )
        }
        return response
    }
}
