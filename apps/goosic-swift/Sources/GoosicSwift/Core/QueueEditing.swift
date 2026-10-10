import Foundation

/// Clearing Up Next keeps history and the playing entry. Undo belongs to one queue revision.
/// The Rust shell-support copy is held to the same boundary and expiration cases.
enum QueueEditing {
    static let undoWindow: TimeInterval = 10

    static func retainedCount(count: Int, currentIndex: Int) -> Int? {
        guard currentIndex >= 0, currentIndex < count else { return nil }
        return currentIndex + 1
    }

    static func canUndo(expectedRevision: UInt64, revision: UInt64,
                        deadline: TimeInterval, now: TimeInterval) -> Bool {
        expectedRevision == revision && now.isFinite && deadline.isFinite && now < deadline
    }
}
