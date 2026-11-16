#if os(macOS)
import AppKit
import SwiftUI

/// Both listening presentations keep lyric navigation, readability and following in step.
struct NativeMacLyricsView: View {
    @ObservedObject var store: NativeMacModelStore
    /// The active line follows the position, which the model no longer publishes.
    @ObservedObject var clock: PlaybackClock
    let immersive: Bool
    @Environment(\.goosicReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast
    @State private var followsPlayback = true
    @State private var positionSample = Date()
    /// Where the presentation's clock stood at `positionSample`, and how fast it runs from
    /// there. It follows the confirmed position without stepping to it; see `reanchor`.
    @State private var positionBase: Double = 0
    @State private var positionRate: Double = 1
    /// The line being sung. It follows the interpolated position, not only the confirmed one,
    /// which arrives about once a second and would make every line change late.
    @State private var activeIndex: Int?
    /// The current line has reached its supplied end and the next has not begun. Nothing is
    /// lit then, as in Music, and a long enough rest shows the interlude dots.
    @State private var resting = false
    /// How far every row is drawn below its place, in points. A line change scrolls the
    /// column at once and sets this to the distance scrolled, so nothing has moved yet; each
    /// row then springs back to zero in its own time. That is how Music moves its lyrics: no
    /// two lines travel together.
    @State private var shift: CGFloat = 0
    /// The line the rows fall in behind: rows at or above it move first.
    @State private var leader = 0
    @State private var viewport: CGFloat = 0
    @State private var rows = RowBox()
    @State private var hoveredLine: String?
    @FocusState private var focusedLine: String?

    private var model: GoosicAppModel { store.model }
    private var canSeek: Bool {
        model.isSeekable && model.serviceConnected && !model.accountOperationInProgress
            && model.playbackTransition == .idle
    }
    /// Nothing may be interpolated while the position is not actually advancing.
    private var clockFrozen: Bool {
        model.isPaused || model.isAdvertisement || !model.serviceConnected
            || model.playbackTransition != .idle
    }

    var body: some View {
        if let lyrics = model.lyrics {
            ScrollViewReader { reader in
                VStack(alignment: .leading, spacing: 12) {
                    ScrollView(showsIndicators: false) {
                        LazyVStack(alignment: .leading, spacing: immersive ? 26 : 22) {
                            if let intro = LyricsTiming.interlude(
                                lines: lyrics.lines, synced: lyrics.synced, index: nil) {
                                placed(
                                    interludeDots(intro, active: activeIndex == nil)
                                        .frame(maxWidth: .infinity, alignment: .leading),
                                    id: Self.introID, order: -1)
                            }
                            ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                                placed(
                                    lyricLine(line, index: index, active: index == activeIndex && !resting,
                                              synced: lyrics.synced),
                                    id: index, order: index)
                                if let gap = LyricsTiming.interlude(
                                    lines: lyrics.lines, synced: lyrics.synced, after: index) {
                                    placed(
                                        interludeDots(gap, active: resting && index == activeIndex)
                                            .frame(maxWidth: .infinity, alignment: .leading),
                                        id: Self.gapID(after: index), order: index + 1)
                                }
                            }
                        }
                        .coordinateSpace(name: Self.rowSpace)
                        .padding(.horizontal, immersive ? Self.bleed : 18)
                        // Room for any row to reach the anchor, the first and the last
                        // included. The move below relies on the column scrolling by exactly
                        // the distance between two rows, which a clamped scroll would not.
                        .padding(.top, max(immersive ? 160 : 120, viewport * Self.anchor))
                        .padding(.bottom, max(immersive ? 160 : 120, viewport * (1 - Self.anchor)))
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { viewport = $0 }
                    .mask {
                        if contrast == .increased {
                            Rectangle()
                        } else {
                            LinearGradient(stops: [
                                .init(color: .clear, location: 0),
                                .init(color: .black, location: immersive ? 0.14 : 0.1),
                                .init(color: .black, location: immersive ? 0.84 : 0.86),
                                .init(color: .clear, location: 1)
                            ], startPoint: .top, endPoint: .bottom)
                        }
                    }
                    // The scroll view clips at its edges and the immersive column has no side
                    // padding of its own, so the swelling dots, a held letter's glow and the
                    // pointer highlight were cut off on the left. This widens the scroll view
                    // and leaves the text where it was. It comes after the mask on purpose: a
                    // mask applied to the narrowed view trimmed the sides straight off again.
                    .padding(.horizontal, immersive ? -Self.bleed : 0)
                    .contextMenu {
                        Button("Import Timed Lyrics…") { NativeMacLyricsImport.open(model: model) }
                    }
                    .overlay(alignment: .bottomTrailing) {
                        if lyrics.synced && !followsPlayback {
                            Button("Resume lyrics", systemImage: "arrow.down.to.line") {
                                followsPlayback = true
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                            .padding(12)
                        }
                    }
                    .background(NativeMacScrollInteractionObserver { followsPlayback = false })
                    .simultaneousGesture(DragGesture(minimumDistance: 4).onChanged { _ in followsPlayback = false })
                    .onAppear {
                        rows.followed = nil
                        let current = model.activeLyricIndex
                        activeIndex = current
                        resting = LyricsTiming.rests(
                            lines: lyrics.lines, index: current,
                            positionMs: Int64(model.displayedPosition * 1_000))
                        scroll(reader, to: anchor(lyrics, index: current, resting: resting), animated: false)
                    }
                    .onChange(of: viewport) { _, _ in
                        // The padding just changed, so the anchor row is no longer at the anchor.
                        if followsPlayback {
                            scroll(reader, to: anchor(lyrics, index: activeIndex, resting: resting), animated: false)
                        }
                    }
                    .onChange(of: activeIndex) { _, current in
                        if followsPlayback {
                            scroll(reader, to: anchor(lyrics, index: current, resting: resting), animated: true)
                        }
                    }
                    .onChange(of: resting) { _, rests in
                        if followsPlayback {
                            scroll(reader, to: anchor(lyrics, index: activeIndex, resting: rests), animated: true)
                        }
                    }
                    .onChange(of: followsPlayback) { _, follow in
                        if follow {
                            scroll(reader, to: anchor(lyrics, index: activeIndex, resting: resting), animated: true)
                        }
                    }
                }
                .task(id: FollowSample(
                    position: model.displayedPosition, frozen: clockFrozen,
                    track: model.currentTrack?.id, lyrics: lyrics
                )) { await follow(lyrics) }
                .onChange(of: model.currentTrack?.id) { _, _ in
                    followsPlayback = true
                    activeIndex = nil
                    resting = false
                    rows.reset()
                    shift = 0
                }
            }
        } else {
            ContentUnavailableView(
                "Lyrics", systemImage: "quote.bubble", description: Text(model.lyricsStatus)
            )
        }
    }

    /// What restarts the following: a newly confirmed position, a pause or resume, another
    /// track, or different lyrics for the same one.
    private struct FollowSample: Equatable {
        let position: Double
        let frozen: Bool
        let track: String?
        let lyrics: GoosicLyrics
    }

    /// Takes the confirmed position, then changes line on time for every line that begins
    /// before the next confirmation is due. It only ever draws; it never moves playback.
    private func follow(_ lyrics: GoosicLyrics) async {
        let sample = Date()
        let anchor = LyricsTiming.reanchor(
            predicted: LyricsTiming.displayPosition(
                confirmed: positionBase, elapsed: sample.timeIntervalSince(positionSample),
                frozen: false, rate: positionRate),
            confirmed: model.displayedPosition, frozen: clockFrozen)
        let confirmed = anchor.base
        positionBase = anchor.base
        positionRate = anchor.rate
        positionSample = sample
        let positionMs = Int64(confirmed * 1_000)
        let lines = lyrics.lines
        let current = GoosicAppModel.activeLyricIndex(
            lines: lines, synced: lyrics.synced, positionMs: positionMs)
        if LyricsTiming.keepsPresented(activeIndex, confirmed: current, lines: lines, positionMs: positionMs) {
            // The line already shown is about to start; it is neither stepped back nor at rest.
            resting = false
        } else {
            var rests = LyricsTiming.rests(lines: lines, index: current, positionMs: positionMs)
            // A rest already shown survives a confirmation just short of the line's end.
            if !rests, resting, current == activeIndex, let current, let end = lines[current].endMs,
               (1...250).contains(end - positionMs) {
                rests = true
            }
            activeIndex = current
            resting = rests
        }
        guard !clockFrozen else { return }
        for moment in LyricsTiming.upcomingChanges(
            lines: lines, synced: lyrics.synced, afterMs: positionMs) {
            let wait = (Double(moment) / 1_000 - confirmed) / anchor.rate - Date().timeIntervalSince(sample)
            if wait > 0 {
                try? await Task.sleep(for: .seconds(wait), tolerance: .milliseconds(8))
            }
            if Task.isCancelled { return }
            let index = GoosicAppModel.activeLyricIndex(
                lines: lines, synced: lyrics.synced, positionMs: moment)
            activeIndex = index
            resting = LyricsTiming.rests(lines: lines, index: index, positionMs: moment)
        }
    }

    /// The row the column follows: the dots while an interlude plays, otherwise the line.
    private func anchor(_ lyrics: GoosicLyrics, index: Int?, resting: Bool) -> Int? {
        guard let index, resting,
              LyricsTiming.interlude(lines: lyrics.lines, synced: lyrics.synced, after: index) != nil
        else { return index }
        return Self.gapID(after: index)
    }

    /// The row of dots drawn after a line whose rest is long enough to show.
    private static func gapID(after index: Int) -> Int { -2 - index }

    /// The row drawn before the first line while the introduction plays.
    private static let introID = -1
    /// Room beside the immersive text for what is drawn outside its bounds.
    private static let bleed: CGFloat = 16
    private var fontSize: CGFloat { immersive ? 30 : 25 }

    private static let rowSpace = "lyricRows"
    /// Where the row being followed sits in the viewport, as a fraction of its height.
    private static let anchor: CGFloat = 0.36
    /// One row's motion, measured from a recording of Music on November 16, 2026: a critically
    /// damped spring that is halfway in about a tenth of a second and settled in a third.
    private static let rowSpring = Animation.spring(response: 0.4, dampingFraction: 1)
    /// How long each row below the leader waits after the one above, from the same recording.
    private static let rowDelay = 0.05

    /// Where the rows are laid out and which one the column follows. It is not view state:
    /// every row reports its place, and none of those reports should redraw anything.
    @MainActor
    private final class RowBox {
        var tops: [Int: CGFloat] = [:]
        var followed: Int?
        func reset() {
            tops = [:]
            followed = nil
        }
    }

    /// A row in the column: it reports where it is laid out, and is drawn `shift` below that.
    private func placed<Row: View>(_ row: Row, id: Int, order: Int) -> some View {
        let delay = Double(min(max(order - leader, 0), 10)) * Self.rowDelay
        return row
            .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.rowSpace)).minY } action: {
                rows.tops[id] = $0
            }
            .offset(y: shift)
            // Taking up the shift is instant; only giving it back is a movement.
            .animation(shift == 0 && !reduceMotion ? Self.rowSpring.delay(delay) : nil, value: shift)
            .id(id)
    }

    private func scroll(_ reader: ScrollViewProxy, to index: Int?, animated: Bool) {
        // Before the first line the introduction's row is the current one, when there is one.
        let id = index ?? Self.introID
        let point = UnitPoint(x: 0, y: Self.anchor)
        let from = rows.followed
        rows.followed = id
        guard animated, !reduceMotion else {
            reader.scrollTo(id, anchor: point)
            return
        }
        guard let from, from != id, let old = rows.tops[from], let new = rows.tops[id] else {
            // Returning to the row already followed, or a row not laid out yet: the column
            // moves as one.
            withAnimation(Self.rowSpring) { reader.scrollTo(id, anchor: point) }
            return
        }
        var still = Transaction()
        still.disablesAnimations = true
        withTransaction(still) {
            reader.scrollTo(id, anchor: point)
            leader = activeIndex ?? -1
            shift = new - old
        }
        // On the next turn, so the shift is taken up before it is given back; set in the same
        // turn the two would cancel and the rows would simply jump.
        DispatchQueue.main.async { shift = 0 }
    }

    @ViewBuilder
    private func lyricLine(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool) -> some View {
        if synced && canSeek {
            Button {
                followsPlayback = true
                model.seek(to: Double(line.atMs) / 1_000)
            } label: {
                lineLabel(line, index: index, active: active, synced: synced)
                    // The line under the pointer is marked the way Music marks it.
                    .background {
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(Color.primary.opacity(hoveredLine == line.id ? 0.08 : 0))
                            .padding(.horizontal, -10)
                            .padding(.vertical, -6)
                            .animation(.easeOut(duration: 0.15), value: hoveredLine)
                    }
            }
            .buttonStyle(.plain)
            .focused($focusedLine, equals: line.id)
            .onHover { hoveredLine = $0 ? line.id : nil }
            .accessibilityLabel("Play from: \(line.text.isEmpty ? "Instrumental" : line.text)")
            .accessibilityValue(active ? "Current line" : "")
        } else {
            lineLabel(line, index: index, active: active, synced: synced)
                .accessibilityValue(active ? "Current line" : "")
        }
    }

    /// Redraws its content at display cadence while `live`, handing it the position to draw
    /// at in milliseconds. It only ever draws; it never advances the playback model.
    private func ticking<Content: View>(
        _ live: Bool, @ViewBuilder content: @escaping (Double) -> Content
    ) -> some View {
        // At 30 frames a second the sweep visibly stepped. Only the one line being sung, or one
        // row of dots, is ever live, so drawing it at 60 costs little.
        TimelineView(.animation(minimumInterval: 1.0 / 60, paused: !live || clockFrozen)) { context in
            content(LyricsTiming.displayPosition(
                confirmed: live && !clockFrozen ? positionBase : model.displayedPosition,
                elapsed: context.date.timeIntervalSince(positionSample),
                frozen: !live || clockFrozen, rate: positionRate) * 1_000)
        }
    }

    /// Three dots that fill across a stretch with nothing sung, and fade out outside it. The
    /// row keeps its height: one that folded away moved every line below it in the same
    /// instant the column was scrolling to the next line, and the two fought.
    private func interludeDots(_ gap: LyricsTiming.Interlude, active: Bool) -> some View {
        let size = fontSize * 0.3
        return ticking(active && !reduceMotion) { position in
            HStack(spacing: size * 0.6) {
                ForEach(0..<3, id: \.self) { dot in
                    Circle()
                        .frame(width: size, height: size)
                        .opacity(gap.dotOpacity(dot, positionMs: position))
                }
            }
            .scaleEffect(gap.scale(positionMs: position, animated: !reduceMotion))
        }
        .foregroundStyle(immersive ? Color.primary : Color.goosicPink)
        .frame(height: fontSize, alignment: .leading)
        .opacity(active ? 1 : 0)
        .animation(reduceMotion ? nil : .smooth(duration: 0.45), value: active)
        .accessibilityHidden(true)
    }

    /// Whether this line is being swept word by word right now.
    private func sweeps(_ line: GoosicLyricsLine, active: Bool, synced: Bool) -> Bool {
        active && synced && !(line.words ?? []).isEmpty && followsPlayback
            && !reduceMotion && contrast != .increased && focusedLine != line.id
    }

    private func lineLabel(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool) -> some View {
        let gap = line.text.isEmpty
            ? LyricsTiming.interlude(lines: model.lyrics?.lines ?? [], synced: synced, index: index) : nil
        // Every line keeps this one structure, so a line becoming current animates into its
        // emphasis instead of being replaced by a different view. Only the line being swept
        // redraws at display cadence.
        return Group {
            if let gap {
                interludeDots(gap, active: active)
            } else {
                ticking(sweeps(line, active: active, synced: synced)) { position in
                    lineText(line, active: active, synced: synced, position: position)
                }
            }
        }
        .modifier(emphasis(line, index: index, active: active, synced: synced))
    }

    private func lineText(_ line: GoosicLyricsLine, active: Bool, synced: Bool, position: Double) -> some View {
        let swept = sweeps(line, active: active, synced: synced)
        // The words must spell the line exactly, or the line is drawn whole.
        let segments = LyricsTiming.segments(line)
        let words = synced && segments.map(\.text).joined() == line.text ? segments : []
        return lineContent(line, words: words, swept: swept, position: position)
            .font(.system(size: fontSize, weight: .bold, design: immersive ? .default : .rounded))
            .foregroundStyle(immersive ? Color.primary : (active ? Color.goosicPink : Color.primary))
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .contentTransition(.identity)
            .modifier(NativeMacLyricSweep(
                sweeps: words.isEmpty ? [] : LyricsTiming.sweeps(line),
                characters: words.map { $0.text.trimmingCharacters(in: .whitespaces).count },
                level: swept && !words.isEmpty ? 1 : 0,
                positionMs: position, fontSize: fontSize))
    }

    /// The line's text. From macOS 15 each word carries its index, so the renderer can sweep,
    /// lift and light it without changing how the line is laid out; the text is built the
    /// same way whether or not the line is current, so becoming current never swaps it or
    /// wraps it differently. Earlier systems brighten the letters in place while it is sung.
    private func lineContent(
        _ line: GoosicLyricsLine, words: [GoosicLyricsWord], swept: Bool, position: Double
    ) -> Text {
        let plain = Text(line.text.isEmpty ? "♪" : line.text)
        guard !words.isEmpty else { return plain }
        if #available(macOS 15, *) {
            return words.enumerated().reduce(Text("")) { text, word in
                Text("\(text)\(Text(word.element.text).customAttribute(NativeMacLyricWord(index: word.offset)))")
            }
        }
        guard swept else { return plain }
        var result = AttributedString()
        let color: Color = immersive ? .white : .goosicPink
        for (word, sweep) in zip(words, LyricsTiming.sweeps(line)) {
            let characters = Array(word.text)
            let progress = sweep.progress(position)
            for (index, character) in characters.enumerated() {
                let fill = min(1, max(0, progress * Double(characters.count) - Double(index)))
                var glyph = AttributedString(String(character))
                glyph.foregroundColor = color.opacity(
                    LyricsTiming.unsungOpacity + fill * (1 - LyricsTiming.unsungOpacity))
                result.append(glyph)
            }
        }
        return Text(result)
    }

    /// The listening treatment shared by a line of text and a row of dots: context is dimmed,
    /// softened and set back; exploration and accessibility keep it sharp.
    private func emphasis(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool) -> NativeMacLyricEmphasis {
        // Before the first line every line is still to come.
        let offset = index - (activeIndex ?? -1)
        let distance = Double(abs(offset))
        let softened = synced && !active && followsPlayback && contrast != .increased
            && focusedLine != line.id && hoveredLine != line.id
        // Nothing is being sung: the introduction, or a line that has reached its end.
        let idle = resting || activeIndex == nil
        let upcoming = offset > 0 || (resting && offset == 0)
        let opacity: Double
        let blur: Double
        if !softened {
            (opacity, blur) = (synced ? 1 : 0.85, 0)
        } else if upcoming {
            // What is about to be sung stays readable, and sharp while nothing is lit.
            opacity = idle ? 0.62 : max(0.3, 0.6 - (distance - 1) * 0.07)
            blur = idle ? 0 : min(3, distance * 0.5)
        } else {
            // What has been sung falls back further.
            opacity = max(0.16, 0.34 - (distance - 1) * 0.06)
            blur = min(3, 0.8 + distance * 0.6)
        }
        return NativeMacLyricEmphasis(
            opacity: opacity,
            blur: blur,
            glow: active && contrast != .increased ? (immersive ? 8 : 5) : 0,
            // Lines below the current one follow it a beat apart, so a line change travels
            // down the column instead of every line moving at once.
            stagger: reduceMotion ? nil : Double(min(max(offset, 0), 6)) * 0.05,
            trigger: (activeIndex ?? -1) * 2 + (resting ? 1 : 0),
            revealed: focusedLine == line.id || hoveredLine == line.id)
    }
}

private struct NativeMacLyricEmphasis: ViewModifier {
    let opacity: Double
    let blur: Double
    let glow: Double
    /// `nil` when motion is reduced.
    let stagger: Double?
    let trigger: Int
    /// Under the pointer or keyboard focus. Lines scroll under a resting pointer on every
    /// line change, so the reveal eases; unanimated, each one popped sharp and soft again.
    let revealed: Bool

    func body(content: Content) -> some View {
        content
            .opacity(opacity)
            .blur(radius: blur)
            .shadow(color: glow > 0 ? Color.white.opacity(0.2) : .clear, radius: glow)
            // Opacity and blur must not overshoot, so they settle without a bounce.
            .animation(stagger.map { .smooth(duration: 0.55).delay($0) }, value: trigger)
            .animation(stagger == nil ? nil : .easeOut(duration: 0.2), value: revealed)
            // A line keeps its size and place when it becomes current, as in Music. It used to
            // grow from 97% and lift two points, which read as a zoom on every line change.
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
    }
}

/// Hands the line to the word renderer where the system has one.
private struct NativeMacLyricSweep: ViewModifier {
    let sweeps: [LyricsTiming.Sweep]
    let characters: [Int]
    /// 1 while the line is being swept, 0 otherwise.
    let level: Double
    let positionMs: Double
    let fontSize: CGFloat

    func body(content: Content) -> some View {
        if #available(macOS 15, *) {
            content.textRenderer(NativeMacLyricSweepRenderer(
                sweeps: sweeps, characters: characters, level: level, positionMs: positionMs,
                fontSize: fontSize))
        } else {
            content
        }
    }
}

@available(macOS 15, *)
private struct NativeMacLyricWord: TextAttribute {
    let index: Int
}

/// Draws the line being sung: each word is swept with a soft edge, rises as it is sung, and
/// a held word's letters swell and glow in turn. It draws over the laid-out glyphs, so the
/// line never wraps differently or moves its neighbours while it animates.
@available(macOS 15, *)
private struct NativeMacLyricSweepRenderer: TextRenderer {
    let sweeps: [LyricsTiming.Sweep]
    let characters: [Int]
    /// How far the line is into its swept treatment, from 0 to 1. It animates with the rest
    /// of the line's emphasis: a line that became current used to drop straight to its unsung
    /// brightness while its own opacity was still rising, which showed as a dark flash.
    var level: Double
    let positionMs: Double
    let fontSize: CGFloat

    var animatableData: Double {
        get { level }
        set { level = newValue }
    }

    /// The brightness of what has not been sung yet: full when the line is not swept.
    private var unsung: Double { 1 - (1 - LyricsTiming.unsungOpacity) * min(1, max(0, level)) }

    /// Room for the lift, the swell and the glow outside the text's own bounds.
    var displayPadding: EdgeInsets {
        let room = fontSize * 0.4
        return EdgeInsets(top: room, leading: room, bottom: room, trailing: room)
    }

    func draw(layout: Text.Layout, in context: inout GraphicsContext) {
        guard !sweeps.isEmpty, level > 0.001 else {
            for line in layout { context.draw(line) }
            return
        }
        var words: [Int: [Text.Layout.Run]] = [:]
        for line in layout {
            for run in line {
                if let word = run[NativeMacLyricWord.self], sweeps.indices.contains(word.index) {
                    words[word.index, default: []].append(run)
                } else {
                    context.draw(run)
                }
            }
        }
        for (index, runs) in words {
            draw(runs, sweep: sweeps[index],
                 characters: characters.indices.contains(index) ? characters[index] : 0, in: context)
        }
    }

    private func draw(
        _ runs: [Text.Layout.Run], sweep: LyricsTiming.Sweep, characters: Int, in context: GraphicsContext
    ) {
        let progress = sweep.progress(positionMs)
        let feather = fontSize * LyricsTiming.featherEm
        let width = runs.reduce(0) { $0 + $1.typographicBounds.rect.width }
        let glyphs = runs.reduce(0) { $0 + $1.count }
        let held = LyricsTiming.held(sweep, characters: characters)
        var word = context
        word.translateBy(x: 0, y: -fontSize * LyricsTiming.liftEm * sweep.lift(positionMs) * level)
        var consumed: CGFloat = 0
        var glyph = 0
        for run in runs {
            let bounds = run.typographicBounds.rect
            // Where the sweep's leading edge sits in this run. It starts a feather before the
            // word and ends a feather after it, so the word is fully lit exactly at its end.
            let edge = bounds.minX - consumed + progress * (width + feather)
            let leftToRight = run.layoutDirection == .leftToRight
            if let held, leftToRight {
                for slice in run {
                    let pose = held.glyph(glyph, of: glyphs, sweep: sweep, positionMs: positionMs)
                    let box = slice.typographicBounds.rect
                    var letter = word
                    let scale = 1 + (pose.scale - 1) * level
                    letter.translateBy(x: box.midX, y: box.midY - fontSize * pose.riseEm * level)
                    letter.scaleBy(x: scale, y: scale)
                    letter.translateBy(x: -box.midX, y: -box.midY)
                    if pose.glow * level > 0.01 {
                        letter.addFilter(.shadow(
                            color: .white.opacity(pose.glow * level),
                            radius: fontSize * min(0.3, held.glow * 0.3 + 0.08)))
                    }
                    fill(in: letter, bounds: bounds, edge: edge, feather: feather, progress: progress,
                         masked: true) { $0.draw(slice, options: .disablesSubpixelQuantization) }
                    glyph += 1
                }
            } else {
                fill(in: word, bounds: bounds, edge: edge, feather: feather, progress: progress,
                     masked: leftToRight) { $0.draw(run, options: .disablesSubpixelQuantization) }
                glyph += run.count
            }
            consumed += bounds.width
        }
    }

    /// Draws the glyphs dim, then lit up to the sweep's edge. Right-to-left text brightens as
    /// a whole instead, because a left-to-right edge would light its last letters first.
    private func fill(
        in context: GraphicsContext, bounds: CGRect, edge: CGFloat, feather: CGFloat,
        progress: Double, masked: Bool, glyphs: (inout GraphicsContext) -> Void
    ) {
        var dim = context
        dim.opacity = masked ? unsung : unsung + (1 - unsung) * progress
        glyphs(&dim)
        guard masked, progress > 0, unsung < 0.999 else { return }
        let area = bounds.insetBy(dx: -fontSize, dy: -fontSize)
        let solid = edge - feather
        context.drawLayer { layer in
            layer.clipToLayer { mask in
                if solid > area.minX {
                    mask.fill(
                        Path(CGRect(x: area.minX, y: area.minY, width: solid - area.minX, height: area.height)),
                        with: .color(.white))
                }
                mask.fill(
                    Path(CGRect(x: solid, y: area.minY, width: feather, height: area.height)),
                    with: .linearGradient(
                        Gradient(colors: [.white, .white.opacity(0)]),
                        startPoint: CGPoint(x: solid, y: 0), endPoint: CGPoint(x: edge, y: 0)))
            }
            glyphs(&layer)
        }
    }
}

/// Observe wheel/trackpad input without intercepting it, including on macOS 14.
/// The window and bounds check confines the monitor to this lyric viewport.
private struct NativeMacScrollInteractionObserver: NSViewRepresentable {
    let onScroll: () -> Void

    func makeNSView(context: Context) -> ScrollObserverView {
        let view = ScrollObserverView()
        view.onScroll = onScroll
        return view
    }

    func updateNSView(_ view: ScrollObserverView, context: Context) { view.onScroll = onScroll }
    static func dismantleNSView(_ view: ScrollObserverView, coordinator: ()) { view.stopObserving() }

    final class ScrollObserverView: NSView {
        var onScroll: (() -> Void)?
        private var monitor: Any?

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopObserving()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
                MainActor.assumeIsolated {
                    if let self, event.window === self.window,
                       self.bounds.contains(self.convert(event.locationInWindow, from: nil)),
                       event.scrollingDeltaX != 0 || event.scrollingDeltaY != 0 {
                        self.onScroll?()
                    }
                }
                return event
            }
        }

        func stopObserving() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
}
#endif
