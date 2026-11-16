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

    /// How far the presentation may run ahead of the last confirmed playback position.
    static let interpolationLimit: TimeInterval = 1

    // MARK: Word motion

    /// How lit the part of the current line not yet sung is drawn.
    static let unsungOpacity: Double = 0.4
    /// The soft leading edge of the sweep, in multiples of the font size.
    static let featherEm: Double = 0.5
    /// How far a word rises as it is sung, in multiples of the font size.
    static let liftEm: Double = 0.05
    /// The sweep of a word whose start equals the next word's.
    static let settleMs: Double = 220
    /// The sweep of a line's last word when the provider gave it no end. Nothing later in the
    /// line bounds it, so it takes a fixed, short time rather than a guess at its length.
    static let lastWordSweepMs: Double = 450
    /// The longest a word without a supplied end is swept. Past this the gap before the next
    /// word is more likely a rest than a held note.
    static let longestInferredSweepMs: Double = 2_500

    /// The span of the recording a word is drawn across.
    struct Sweep: Equatable {
        var startMs: Double
        var endMs: Double
        /// Whether the provider supplied the end. An inferred end only ever shapes drawing.
        var supplied: Bool

        var durationMs: Double { endMs - startMs }

        func progress(_ positionMs: Double) -> Double {
            guard positionMs.isFinite, durationMs > 0 else { return 0 }
            return min(1, max(0, (positionMs - startMs) / durationMs))
        }

        /// The word's rise, eased out over at least a second, and kept once reached.
        func lift(_ positionMs: Double) -> Double {
            guard positionMs.isFinite else { return 0 }
            let x = min(1, max(0, (positionMs - startMs) / max(durationMs, 1_000)))
            return 1 - (1 - x) * (1 - x)
        }
    }

    /// The sweep of every word in a line, in order.
    ///
    /// A supplied start/end pair is used as it is. A word with only a start is swept until the
    /// next word in the same line begins, which is a boundary the provider did supply: the
    /// word cannot still be the one being sung after that. This shapes the drawing of a
    /// word-timed line only. It never creates timing for a line without words, and `progress`
    /// still reports a start-only word the way the shared rule defines it.
    static func sweeps(_ line: GoosicLyricsLine) -> [Sweep] {
        let words = line.words ?? []
        return words.enumerated().map { index, word in
            let start = Double(word.atMs)
            if let end = word.endMs, end > word.atMs {
                return Sweep(startMs: start, endMs: Double(end), supplied: true)
            }
            guard index + 1 < words.count else {
                // The line's supplied end bounds its last word the way the next word bounds
                // the others.
                if let end = line.endMs, Double(end) > start {
                    return Sweep(
                        startMs: start, endMs: min(Double(end), start + longestInferredSweepMs),
                        supplied: false)
                }
                return Sweep(startMs: start, endMs: start + lastWordSweepMs, supplied: false)
            }
            let next = Double(words[index + 1].atMs)
            let end = next > start ? min(next, start + longestInferredSweepMs) : start + settleMs
            return Sweep(startMs: start, endMs: end, supplied: false)
        }
    }

    /// The extra emphasis a held word gets: its letters swell and glow one after another.
    struct Held: Equatable {
        /// How strongly the letters swell, from 0 to 1.2.
        var amount: Double
        /// The peak opacity of the glow, from 0 to 0.8.
        var glow: Double

        /// One letter's pose. `scale` multiplies its size, `riseEm` lifts it in multiples of
        /// the font size, and `glow` is the opacity of its glow.
        func glyph(
            _ index: Int, of count: Int, sweep: Sweep, positionMs: Double
        ) -> (scale: Double, riseEm: Double, glow: Double) {
            guard positionMs.isFinite, sweep.durationMs > 0 else { return (1, 0, 0) }
            let delay = sweep.durationMs / 2.5 / Double(max(1, count)) * Double(index)
            let x = min(1, max(0, (positionMs - sweep.startMs - delay) / sweep.durationMs))
            let bell = sin(x * .pi)
            return (1 + 0.1 * amount * bell, liftEm * bell * min(1, amount * 2), glow * bell)
        }
    }

    /// Whether a word is held long enough to be emphasised, and how strongly. A word shorter
    /// than a second, or one that is a single letter or a long phrase, is only swept.
    static func held(_ sweep: Sweep, characters: Int) -> Held? {
        guard sweep.durationMs >= 1_000, (2...7).contains(characters) else { return nil }
        func shaped(_ value: Double) -> Double { value <= 1 ? value * value * value : value.squareRoot() }
        return Held(
            amount: min(1.2, shaped(sweep.durationMs / 2_000) * 0.6),
            glow: min(0.8, shaped(sweep.durationMs / 3_000) * 0.5))
    }

    // MARK: Rests

    /// The shortest wait before the next line that is drawn as a rest. A line that ends a
    /// fraction of a second before the next begins stays lit until then: dimming it for that
    /// moment showed as the highlight dropping back and jumping forward on a line change.
    static let restMinMs: Int64 = 1_000

    /// Where a line comes to rest: its supplied end, when the next line is far enough off, or
    /// when it is the last line. Without a supplied end the line stays current until the next
    /// one starts.
    static func restStart(lines: [GoosicLyricsLine], index: Int) -> Int64? {
        guard lines.indices.contains(index), let end = lines[index].endMs,
              end > lines[index].atMs else { return nil }
        if index + 1 < lines.count, lines[index + 1].atMs - end < restMinMs { return nil }
        return end
    }

    /// Whether the current line has finished and the next has not begun.
    static func rests(lines: [GoosicLyricsLine], index: Int?, positionMs: Int64) -> Bool {
        guard let index, let end = restStart(lines: lines, index: index) else { return false }
        return positionMs >= end
    }

    /// The moments inside the interpolation limit after `positionMs` at which the presentation
    /// changes: a line starting, or a line reaching its supplied end. In order, without repeats.
    static func upcomingChanges(
        lines: [GoosicLyricsLine], synced: Bool, afterMs positionMs: Int64
    ) -> [Int64] {
        guard synced else { return [] }
        let limit = positionMs + Int64(interpolationLimit * 1_000)
        var moments = Set<Int64>()
        for (index, line) in lines.enumerated() {
            if line.atMs > positionMs, line.atMs <= limit { moments.insert(line.atMs) }
            if let end = restStart(lines: lines, index: index), end > positionMs, end <= limit {
                moments.insert(end)
            }
        }
        return moments.sorted()
    }

    // MARK: Interludes

    /// The shortest stretch without singing that is drawn as an interlude.
    static let interludeMinMs: Int64 = 4_000

    /// A stretch the provider marked as having nothing sung.
    struct Interlude: Equatable {
        var startMs: Int64
        var endMs: Int64

        func progress(_ positionMs: Double) -> Double {
            guard positionMs.isFinite, endMs > startMs else { return 0 }
            return min(1, max(0, (positionMs - Double(startMs)) / Double(endMs - startMs)))
        }

        /// How lit one of the three dots is: they fill one after another across the interlude.
        func dotOpacity(_ dot: Int, positionMs: Double) -> Double {
            0.25 + 0.75 * min(1, max(0, progress(positionMs) * 3 - Double(dot)))
        }

        /// The size of the dots: a slow breath, then a swell and a collapse just before the
        /// next line. Still when motion is reduced.
        func scale(positionMs: Double, animated: Bool) -> Double {
            guard animated, positionMs.isFinite else { return 1 }
            let breath = 1 + 0.08 * sin(positionMs / 1_800 * 2 * .pi)
            let remaining = Double(endMs) - positionMs
            guard remaining < 600 else { return breath }
            let x = min(1, max(0, 1 - remaining / 600))
            return breath * (x < 0.35 ? 1 + 0.25 * x / 0.35 : 1.25 * (1 - (x - 0.35) / 0.65))
        }
    }

    /// The interlude between a line's supplied end and the next line's start, when it is long
    /// enough to draw. A line without a supplied end has no interlude after it.
    static func interlude(lines: [GoosicLyricsLine], synced: Bool, after index: Int) -> Interlude? {
        guard synced, lines.indices.contains(index), index + 1 < lines.count,
              let end = lines[index].endMs, end > lines[index].atMs,
              !lines[index].text.trimmingCharacters(in: .whitespaces).isEmpty,
              lines[index + 1].atMs - end >= interludeMinMs else { return nil }
        return Interlude(startMs: end, endMs: lines[index + 1].atMs)
    }

    /// The interlude a row stands for: the introduction when `index` is `nil`, otherwise a
    /// line the provider left empty. Both are stretches the lyrics themselves mark as unsung;
    /// a rest inside or after a sung line is never guessed at.
    static func interlude(lines: [GoosicLyricsLine], synced: Bool, index: Int?) -> Interlude? {
        guard synced, let first = lines.first else { return nil }
        guard let index else {
            return first.atMs >= interludeMinMs ? Interlude(startMs: 0, endMs: first.atMs) : nil
        }
        guard lines.indices.contains(index), index + 1 < lines.count,
              lines[index].text.trimmingCharacters(in: .whitespaces).isEmpty,
              lines[index + 1].atMs - lines[index].atMs >= interludeMinMs else { return nil }
        return Interlude(startMs: lines[index].atMs, endMs: lines[index + 1].atMs)
    }

    /// The position to draw at: the confirmed one, carried forward by the time since it was
    /// confirmed, and never by more than `interpolationLimit`. Frozen while nothing is playing.
    static func displayPosition(
        confirmed: Double, elapsed: TimeInterval, frozen: Bool, rate: Double = 1
    ) -> Double {
        guard !frozen, elapsed.isFinite else { return confirmed }
        return confirmed + min(interpolationLimit, max(0, elapsed)) * rate
    }

    /// Where the presentation's clock continues from when a new position is confirmed.
    ///
    /// The confirmation never lands exactly where the clock had got to, and stepping to it
    /// made every sweep hitch about once a second. When the two are close the clock keeps the
    /// position it was already showing and runs slightly fast or slow instead, closing 60% of
    /// the gap over the next second. A real jump, such as a seek or a stall, is taken at once,
    /// and so is anything while the position is not advancing.
    static func reanchor(predicted: Double, confirmed: Double, frozen: Bool) -> (base: Double, rate: Double) {
        let error = confirmed - predicted
        guard !frozen, predicted.isFinite, abs(error) < 0.3 else { return (confirmed, 1) }
        return (predicted, 1 + error * 0.6)
    }

    /// The synced lines that begin after `positionMs` and inside the interpolation limit, in
    /// order. These are the line changes the presentation can make on time instead of waiting
    /// for the next confirmed position.
    static func upcomingStarts(
        lines: [GoosicLyricsLine], synced: Bool, afterMs positionMs: Int64
    ) -> [(index: Int, atMs: Int64)] {
        guard synced else { return [] }
        let limit = positionMs + Int64(interpolationLimit * 1_000)
        return lines.enumerated().compactMap { index, line in
            line.atMs > positionMs && line.atMs <= limit ? (index, line.atMs) : nil
        }
    }

    /// Whether a line the presentation already moved to should stay current when a confirmed
    /// position lands just short of its start. Without this, a clock that ran a few
    /// milliseconds ahead would flick back to the previous line and forward again.
    static func keepsPresented(
        _ presented: Int?, confirmed: Int?, lines: [GoosicLyricsLine], positionMs: Int64
    ) -> Bool {
        guard let presented, lines.indices.contains(presented), presented == (confirmed ?? -1) + 1
        else { return false }
        let lead = lines[presented].atMs - positionMs
        return lead > 0 && lead <= 250
    }
}
