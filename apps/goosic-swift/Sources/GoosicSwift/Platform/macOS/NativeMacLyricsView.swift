#if os(macOS)
import AppKit
import SwiftUI

/// Both listening presentations keep lyric navigation, readability and following in step.
struct NativeMacLyricsView: View {
    @ObservedObject var store: NativeMacModelStore
    let immersive: Bool
    @Environment(\.goosicReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast
    @State private var followsPlayback = true
    @State private var positionSample = Date()
    @State private var hoveredLine: String?
    @FocusState private var focusedLine: String?

    private var model: GoosicAppModel { store.model }
    private var canSeek: Bool {
        model.isSeekable && model.serviceConnected && !model.accountOperationInProgress
            && model.playbackTransition == .idle
    }

    var body: some View {
        if let lyrics = model.lyrics {
            ScrollViewReader { reader in
                VStack(alignment: .leading, spacing: 12) {
                    ScrollView(showsIndicators: false) {
                        LazyVStack(alignment: .leading, spacing: immersive ? 26 : 22) {
                            ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                                lyricLine(line, index: index, active: index == model.activeLyricIndex, synced: lyrics.synced)
                                    .id(index)
                            }
                        }
                        .padding(.horizontal, immersive ? 0 : 18)
                        .padding(.vertical, immersive ? 160 : 120)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
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
                    .onAppear { scrollToCurrent(reader, animated: false) }
                    .onChange(of: model.activeLyricIndex) { _, _ in
                        if followsPlayback { scrollToCurrent(reader, animated: true) }
                    }
                    .onChange(of: followsPlayback) { _, follow in
                        if follow { scrollToCurrent(reader, animated: true) }
                    }
                }
                .onChange(of: model.displayedPosition) { _, _ in positionSample = Date() }
                .onChange(of: model.isPaused) { _, _ in positionSample = Date() }
                .onChange(of: model.currentTrack?.id) { _, _ in
                    followsPlayback = true
                    positionSample = Date()
                    scrollToCurrent(reader, animated: false)
                }
            }
        } else {
            ContentUnavailableView(
                "Lyrics", systemImage: "quote.bubble", description: Text(model.lyricsStatus)
            )
        }
    }

    private func scrollToCurrent(_ reader: ScrollViewProxy, animated: Bool) {
        guard let active = model.activeLyricIndex else { return }
        withAnimation(animated && !reduceMotion ? .easeInOut(duration: immersive ? 0.72 : 0.65) : nil) {
            reader.scrollTo(active, anchor: UnitPoint(x: 0, y: 0.36))
        }
    }

    @ViewBuilder
    private func lyricLine(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool) -> some View {
        if synced && canSeek {
            Button {
                followsPlayback = true
                model.seek(to: Double(line.atMs) / 1_000)
            } label: { lineLabel(line, index: index, active: active, synced: synced) }
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

    @ViewBuilder
    private func lineLabel(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool) -> some View {
        if active && synced && !(line.words ?? []).isEmpty && followsPlayback && !reduceMotion && contrast != .increased {
            // Only the active line redraws at display cadence. Never advance the playback model.
            TimelineView(.animation(minimumInterval: 1.0 / 30, paused: model.isPaused || model.isAdvertisement || !model.serviceConnected || model.playbackTransition != .idle)) { context in
                let elapsed = model.isPaused || model.isAdvertisement || !model.serviceConnected || model.playbackTransition != .idle ? 0
                    : min(1, max(0, context.date.timeIntervalSince(positionSample)))
                lineText(line, index: index, active: active, synced: synced,
                         position: (model.displayedPosition + elapsed) * 1_000)
            }
        } else {
            lineText(line, index: index, active: active, synced: synced,
                     position: model.displayedPosition * 1_000)
        }
    }

    private func lineText(_ line: GoosicLyricsLine, index: Int, active: Bool, synced: Bool, position: Double) -> some View {
        // The listening treatment matches Music; exploration and accessibility keep text sharp.
        let softened = synced && !active && followsPlayback && contrast != .increased
            && focusedLine != line.id && hoveredLine != line.id
        let distance = abs(index - (model.activeLyricIndex ?? index))
        return Text(highlightedText(line, active: active, synced: synced, position: position))
            .font(.system(size: immersive ? 30 : 25, weight: .bold,
                          design: immersive ? .default : .rounded))
            .foregroundStyle(immersive ? Color.primary : (active ? Color.goosicPink : Color.primary))
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .opacity(softened ? max(0.16, (immersive ? 0.36 : 0.34) - Double(distance) * 0.04) : (synced ? 1 : 0.85))
            .blur(radius: softened ? min(3, (immersive ? 1.1 : 0.8) + Double(distance) * 0.35) : 0)
            .shadow(color: active && contrast != .increased ? Color.white.opacity(0.2) : .clear,
                    radius: immersive ? 8 : 5)
            .offset(y: active && !reduceMotion ? -2 : 0)
            .scaleEffect(active || !synced || !followsPlayback || contrast == .increased ? 1 : 0.97,
                         anchor: .leading)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.4), value: model.activeLyricIndex)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
    }
    private func highlightedText(_ line: GoosicLyricsLine, active: Bool, synced: Bool, position: Double) -> AttributedString {
        guard active, synced, followsPlayback, !reduceMotion, contrast != .increased,
              focusedLine != line.id, hoveredLine != line.id else {
            return AttributedString(line.text.isEmpty ? "♪" : line.text)
        }
        let segments = LyricsTiming.segments(line)
        guard !segments.isEmpty else { return AttributedString(line.text.isEmpty ? "♪" : line.text) }
        let color: Color = immersive ? .white : .goosicPink
        var result = AttributedString()
        for word in segments {
            let progress = LyricsTiming.progress(word, positionMs: position)
            let characters = Array(word.text)
            for (index, character) in characters.enumerated() {
                // A soft leading edge follows real segment timestamps when supplied.
                let fill = min(1, max(0, progress * Double(characters.count) - Double(index)))
                var glyph = AttributedString(String(character))
                glyph.foregroundColor = color.opacity(0.55 + fill * 0.45)
                result.append(glyph)
            }
        }
        return result
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
