#if os(macOS)
/// Tracks the latest native transport intent while WebKit attaches its media element.
/// This is command delivery only; confirmed playback state still comes from validated samples.
struct MacMediaTransportRequest {
    private(set) var paused: Bool?
    private(set) var revision: UInt64 = 0

    mutating func request(paused: Bool) {
        revision &+= 1
        self.paused = paused
    }

    mutating func complete(revision: UInt64) {
        guard revision == self.revision else { return }
        paused = nil
    }

    mutating func reset() {
        revision &+= 1
        paused = nil
    }
}
#endif
