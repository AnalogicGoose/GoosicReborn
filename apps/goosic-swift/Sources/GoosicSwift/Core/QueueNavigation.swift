/// Swift copy of `index_before` in goosic-shell-support.
enum QueueNavigation {
    static func indexBefore(_ index: Int, count: Int, repeatMode: RepeatMode) -> Int? {
        guard count > 0, (0..<count).contains(index) else { return nil }
        if index > 0 { return index - 1 }
        return repeatMode == .all ? count - 1 : 0
    }
}
