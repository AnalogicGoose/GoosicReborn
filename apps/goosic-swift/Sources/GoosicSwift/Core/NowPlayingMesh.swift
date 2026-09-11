/*
 * NowPlayingMesh.swift — the palette, seeded layout, and motion behind the full-screen player's
 * procedural background, and the rule that asks YouTube's image servers for a larger cover.
 *
 * Ported from the previous Goosic application: src/components/layout/now-playing-background.tsx
 * (palette extraction, the seeded mesh grid) and src/index.css (the drift and breathe keyframes),
 * plus getHighResVariant from src/components/shared/thumbnail.tsx. That implementation was
 * written independently from the technique described by frigopedro/Apple-Music-Background, which
 * carries no license; no code from that repository is used here.
 *
 * Copyright (C) Oscar Mantilla, George Shyshov, and Goosic contributors.
 *
 * This program is free software: you can redistribute it and/or modify it under the terms of
 * the GNU General Public License as published by the Free Software Foundation, version 3.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
 * without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details. A copy is in LICENSE-GPL-3.0 at the
 * repository root.
 */
import Foundation

/// One colour the cover really contains, and how common it is relative to the most common one.
struct MeshSample: Hashable, Sendable {
    /// Channels in 0...255, exactly as averaged from the cover.
    var red: Double
    var green: Double
    var blue: Double
    /// 1 for the most common colour, proportionally less for the rest.
    var weight: Double
}

/// One blob of the mesh: which sampled colour it paints and where it sits in its grid slot.
struct MeshCell: Hashable, Sendable {
    var color: MeshSample
    /// Blob centre within its slot, as fractions of the slot.
    var x: Double
    var y: Double
    /// Blob size relative to its slot, so radii vary across the field.
    var scale: Double
}

/// Where one breathing blob is at a moment in time.
struct MeshCellPose: Equatable, Sendable {
    var opacity: Double
    /// Offset as a fraction of the slot size.
    var dx: Double
    var dy: Double
    /// Multiplier on the cell's own `scale`.
    var scale: Double
}

/// Where the whole drifting field is at a moment in time.
struct MeshDriftPose: Equatable, Sendable {
    /// Offset as a fraction of the field size.
    var dx: Double
    var dy: Double
    var scale: Double
    var degrees: Double
}

enum NowPlayingMesh {
    /// The cover is scaled to this square before sampling. Sampling every second pixel of it is
    /// enough for a stable palette while keeping track changes cheap.
    static let sampleSide = 48
    /// The field is this many slots on each side.
    static let gridSide = 6
    static let paletteSize = 5

    // MARK: - Palette

    /// The cover's dominant colours, most common first, from `width × height` RGBA bytes.
    ///
    /// Frequency matters more than saturation: a mostly white and red cover stays mostly white
    /// and red instead of promoting a small colourful detail. Colours are never invented — a
    /// simple cover with fewer than five distinct clusters repeats its real ones — and a cover
    /// with no opaque pixels has no palette at all.
    static func palette(rgba pixels: [UInt8], width: Int, height: Int) -> [MeshSample]? {
        guard width > 0, height > 0, pixels.count >= width * height * 4 else { return nil }

        struct Bucket {
            var red = 0.0, green = 0.0, blue = 0.0
            var count = 0
        }
        var buckets: [Int: Bucket] = [:]
        // First-seen order, so equally common colours rank the same way on every run.
        var order: [Int] = []

        for y in stride(from: 0, to: height, by: 2) {
            for x in stride(from: 0, to: width, by: 2) {
                let offset = (y * width + x) * 4
                guard pixels[offset + 3] >= 200 else { continue }
                let red = pixels[offset], green = pixels[offset + 1], blue = pixels[offset + 2]
                let key = Int(red >> 4) << 8 | Int(green >> 4) << 4 | Int(blue >> 4)
                if buckets[key] == nil {
                    buckets[key] = Bucket()
                    order.append(key)
                }
                buckets[key]!.red += Double(red)
                buckets[key]!.green += Double(green)
                buckets[key]!.blue += Double(blue)
                buckets[key]!.count += 1
            }
        }

        let candidates = order.enumerated()
            .map { rank, key -> (rank: Int, red: Double, green: Double, blue: Double, count: Int) in
                let bucket = buckets[key]!
                let count = Double(bucket.count)
                return (rank, bucket.red / count, bucket.green / count, bucket.blue / count, bucket.count)
            }
            .sorted { $0.count != $1.count ? $0.count > $1.count : $0.rank < $1.rank }

        var selected: [(red: Double, green: Double, blue: Double, count: Int)] = []
        for candidate in candidates {
            let distinct = selected.allSatisfy { color in
                let dr = candidate.red - color.red
                let dg = candidate.green - color.green
                let db = candidate.blue - color.blue
                return (dr * dr + dg * dg + db * db).squareRoot() > 36
            }
            if distinct { selected.append((candidate.red, candidate.green, candidate.blue, candidate.count)) }
            if selected.count == paletteSize { break }
        }

        guard let largest = selected.first?.count else { return nil }
        var samples = selected.map {
            MeshSample(
                red: $0.red.rounded(),
                green: $0.green.rounded(),
                blue: $0.blue.rounded(),
                weight: Double($0.count) / Double(largest)
            )
        }
        let distinctCount = samples.count
        while samples.count < paletteSize {
            samples.append(samples[samples.count % distinctCount])
        }
        return samples
    }

    // MARK: - Layout

    /// A 32-bit FNV-1a of the palette, so each cover gets its own arrangement and the same cover
    /// always gets the same one.
    static func seed(for palette: [MeshSample]) -> UInt32 {
        var hash: UInt32 = 2_166_136_261
        for sample in palette {
            let text = "rgb(\(Int(sample.red)) \(Int(sample.green)) \(Int(sample.blue))):"
                + String(format: "%.4f", sample.weight)
            for unit in text.utf16 {
                hash = (hash ^ UInt32(unit)) &* 16_777_619
            }
        }
        return hash
    }

    /// The field's blobs, row by row, placed by a generator seeded from the palette.
    ///
    /// Colours are picked in proportion to their weight, and each blob sits off its slot centre;
    /// that offset is what stops the blurred field from resolving into a visible lattice.
    static func cells(for palette: [MeshSample]) -> [MeshCell] {
        guard !palette.isEmpty else { return [] }
        var state = seed(for: palette)
        if state == 0 { state = 1 }
        func random() -> Double {
            state = state &* 1_664_525 &+ 1_013_904_223
            return Double(state) / 4_294_967_296
        }
        let totalWeight = palette.reduce(0) { $0 + $1.weight }
        func pick() -> MeshSample {
            var target = random() * totalWeight
            for sample in palette {
                target -= sample.weight
                if target <= 0 { return sample }
            }
            return palette[0]
        }
        return (0..<(gridSide * gridSide)).map { _ in
            // Evaluated in this order on purpose: colour, then x, y, and scale.
            let color = pick()
            let x = (20 + random() * 60) / 100
            let y = (20 + random() * 60) / 100
            let scale = 1.05 + random() * 0.45
            return MeshCell(color: color, x: x, y: y, scale: scale)
        }
    }

    // MARK: - Motion

    /// Progress through an `alternate` animation at `time`: 0 → 1, then 1 → 0, and so on.
    /// A negative `delay` starts partway through, as in CSS.
    static func alternatingProgress(time: Double, duration: Double, delay: Double) -> Double {
        guard duration > 0 else { return 0 }
        let elapsed = (time - delay) / duration
        let cycle = elapsed.rounded(.down)
        let fraction = elapsed - cycle
        let odd = Int(cycle).quotientAndRemainder(dividingBy: 2).remainder != 0
        return odd ? 1 - fraction : fraction
    }

    /// Symmetric ease-in-out, close to CSS `ease-in-out`.
    static func ease(_ progress: Double) -> Double {
        (1 - cos(Double.pi * min(max(progress, 0), 1))) / 2
    }

    /// The whole field's slow 28-second drift, through three keyframes.
    static func drift(at time: Double) -> MeshDriftPose {
        let frames = [
            MeshDriftPose(dx: -0.03, dy: -0.02, scale: 1.12, degrees: -1.5),
            MeshDriftPose(dx: 0.02, dy: 0.03, scale: 1.17, degrees: 1),
            MeshDriftPose(dx: 0.04, dy: -0.01, scale: 1.13, degrees: 2),
        ]
        let progress = alternatingProgress(time: time, duration: 28, delay: -8)
        let (from, to, local) = progress < 0.5
            ? (frames[0], frames[1], progress / 0.5)
            : (frames[1], frames[2], (progress - 0.5) / 0.5)
        let eased = ease(local)
        func mix(_ a: Double, _ b: Double) -> Double { a + (b - a) * eased }
        return MeshDriftPose(
            dx: mix(from.dx, to.dx),
            dy: mix(from.dy, to.dy),
            scale: mix(from.scale, to.scale),
            degrees: mix(from.degrees, to.degrees)
        )
    }

    /// The field at rest, for Reduce Motion.
    static let restingDrift = MeshDriftPose(dx: -0.02, dy: -0.01, scale: 1.12, degrees: -1)

    /// One blob's breathing. Each cell has its own period and phase, and every third runs in
    /// reverse, so the field never pulses in unison.
    static func breathe(index: Int, at time: Double) -> MeshCellPose {
        let duration = 18 + Double(index % 7) * 2
        let delay = -(Double(index) * 1.37).truncatingRemainder(dividingBy: 19)
        var eased = ease(alternatingProgress(time: time, duration: duration, delay: delay))
        if index % 3 == 1 { eased = 1 - eased }
        return MeshCellPose(
            opacity: 0.82 + 0.18 * eased,
            dx: -0.05 + 0.10 * eased,
            dy: -0.04 + 0.08 * eased,
            scale: 0.94 + 0.14 * eased
        )
    }

    /// A blob at rest, for Reduce Motion.
    static let restingCell = MeshCellPose(opacity: 1, dx: -0.03, dy: -0.02, scale: 1)

    // MARK: - Artwork size

    /// A larger render of a YouTube Music cover, or `nil` when no upgrade applies.
    ///
    /// Google's image servers take the size in the URL: `=w120-h120-l90-rj` and `=s120-c-…` have
    /// their leading size token replaced and keep the trailing modifiers. A video thumbnail is
    /// swapped for `maxresdefault.jpg`, which not every video has, so a caller must fall back to
    /// the original when the larger one fails to load.
    static func highResolutionVariant(of url: String, size: Int = 1080) -> String? {
        if let upgraded = replacingFirst(#"=w\d+-h\d+"#, in: url, with: "=w\(size)-h\(size)") {
            return upgraded
        }
        if let upgraded = replacingFirst(#"=s\d+"#, in: url, with: "=s\(size)") {
            return upgraded
        }
        if let upgraded = replacingFirst(#"(/vi/[^/]+/)[^./]+\.jpg"#, in: url, with: "$1maxresdefault.jpg") {
            return upgraded
        }
        if url.range(of: #"(?:lh3\.googleusercontent\.com|yt3\.ggpht\.com)"#, options: .regularExpression) != nil,
           !url.contains("=") {
            return "\(url)=w\(size)-h\(size)-l90-rj"
        }
        return nil
    }

    private static func replacingFirst(_ pattern: String, in text: String, with template: String) -> String? {
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return nil }
        let whole = NSRange(text.startIndex..., in: text)
        guard let match = expression.firstMatch(in: text, range: whole),
              let range = Range(match.range, in: text) else { return nil }
        let replacement = expression.replacementString(for: match, in: text, offset: 0, template: template)
        return text.replacingCharacters(in: range, with: replacement)
    }
}
