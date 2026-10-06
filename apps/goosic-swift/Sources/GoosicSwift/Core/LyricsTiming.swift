import Foundation

/// Imported lyric timing never changes playback authority.
enum LyricsTiming {
    static func valid(_ document: GoosicLyrics) -> Bool {
        guard document.synced, !document.lines.isEmpty, document.lines.count <= 900 else { return false }
        var previous: Int64 = -1
        for line in document.lines {
            guard line.atMs >= 0, line.atMs > previous, line.text.unicodeScalars.count <= 512 else { return false }
            previous = line.atMs
            guard let words = line.words, !words.isEmpty else { continue }
            guard words.count <= 128, words.map(\.text).joined() == line.text else { return false }
            var end = line.atMs
            for word in words {
                guard word.atMs >= end, (word.endMs ?? word.atMs) >= word.atMs, !word.text.isEmpty else { return false }
                end = word.endMs ?? word.atMs
            }
        }
        for (line, next) in zip(document.lines, document.lines.dropFirst()) {
            if let end = line.words?.last?.endMs, end > next.atMs { return false }
        }
        return true
    }

    static func progress(_ word: GoosicLyricsWord, positionMs: Double) -> Double {
        guard positionMs.isFinite else { return 0 }
        guard let end = word.endMs, end > word.atMs else {
            return positionMs >= Double(word.atMs) ? 1 : 0
        }
        return min(1, max(0, (positionMs - Double(word.atMs)) / (Double(end) - Double(word.atMs))))
    }

    static func segments(_ line: GoosicLyricsLine) -> [GoosicLyricsWord] {
        line.words ?? []
    }
}
