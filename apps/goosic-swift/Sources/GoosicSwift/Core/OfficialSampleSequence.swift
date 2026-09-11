import Foundation

/// Translates a renderer-local observer sequence into the lease-wide sequence Rust validates.
///
/// The page observer is recreated for every track and therefore starts at one again. Rust's
/// sequence, however, belongs to the playback lease and must continue across same-owner track
/// replacements. Keeping this small ledger at the native boundary avoids asking a WebView to own
/// protocol state it cannot see.
struct OfficialSampleSequence {
    private(set) var generation: UInt64?
    private(set) var lastIssued: UInt64 = 0

    mutating func begin(generation: UInt64) {
        if self.generation != generation {
            lastIssued = 0
        }
        self.generation = generation
    }

    mutating func issue(for generation: UInt64) -> UInt64? {
        guard self.generation == generation else { return nil }
        guard lastIssued < UInt64.max else { return nil }
        lastIssued += 1
        return lastIssued
    }

    mutating func reset() {
        generation = nil
        lastIssued = 0
    }
}
