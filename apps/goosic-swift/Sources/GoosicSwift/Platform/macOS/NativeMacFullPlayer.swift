#if os(macOS) && !GOOSIC_PORTABLE
/*
 * NativeMacFullPlayer.swift — the immersive full-screen player for the native macOS shell.
 *
 * Layout and behaviour ported from the previous Goosic application's full-screen player
 * (src/components/layout/player-bar.tsx, variant "fullscreen", and lyrics-view.tsx), and its
 * procedural background from src/components/layout/now-playing-background.tsx; the rules those
 * rely on live in Core/NowPlayingMesh.swift.
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
import AppKit
import ImageIO
import SwiftUI

/// Reads a cached cover into the small bitmap `NowPlayingMesh.palette` samples.
enum NowPlayingPaletteLoader {
    static func palette(at file: URL) -> [MeshSample]? {
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let side = NowPlayingMesh.sampleSide
        var pixels = [UInt8](repeating: 0, count: side * side * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress,
                width: side,
                height: side,
                bitsPerComponent: 8,
                bytesPerRow: side * 4,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.interpolationQuality = .medium
            context.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
            return true
        }
        guard drawn else { return nil }
        return NowPlayingMesh.palette(rgba: pixels, width: side, height: side)
    }
}

/// The animated field of soft colour blobs sampled from the cover.
///
/// Each cell is a radial blob rather than a filled square: overlapping blobs with a transparent
/// falloff fuse into one fluid field once blurred, where squares would read as a mosaic. The
/// field is oversized by 30% on every side so the drift never reveals an edge.
struct NowPlayingMeshBackground: View {
    let palette: [MeshSample]
    @Environment(\.goosicReduceMotion) private var reduceMotion

    var body: some View {
        let cells = NowPlayingMesh.cells(for: palette)
        GeometryReader { proxy in
            TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: reduceMotion)) { timeline in
                field(cells: cells, size: proxy.size, time: timeline.date.timeIntervalSinceReferenceDate)
            }
        }
        .background(palette.first.map(Self.color) ?? .black)
        // The frost: in the dark appearance this player always uses, a slight darkening.
        .overlay(Color.black.opacity(0.18))
        .clipped()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func field(cells: [MeshCell], size: CGSize, time: Double) -> some View {
        let side = NowPlayingMesh.gridSide
        let fieldSize = CGSize(width: size.width * 1.6, height: size.height * 1.6)
        let slot = CGSize(width: fieldSize.width / CGFloat(side), height: fieldSize.height / CGFloat(side))
        let drift = reduceMotion ? NowPlayingMesh.restingDrift : NowPlayingMesh.drift(at: time)
        let blur = min(max(size.width * 0.09, 110), 160)

        return ZStack(alignment: .topLeading) {
            ForEach(Array(cells.enumerated()), id: \.offset) { index, cell in
                let pose = reduceMotion ? NowPlayingMesh.restingCell : NowPlayingMesh.breathe(index: index, at: time)
                blob(cell, slot: slot)
                    .scaleEffect(cell.scale * pose.scale)
                    .offset(x: pose.dx * slot.width, y: pose.dy * slot.height)
                    .opacity(pose.opacity)
                    .offset(
                        x: CGFloat(index % side) * slot.width,
                        y: CGFloat(index / side) * slot.height
                    )
            }
        }
        .frame(width: fieldSize.width, height: fieldSize.height, alignment: .topLeading)
        .drawingGroup()
        .blur(radius: blur)
        .saturation(1.08)
        .scaleEffect(drift.scale)
        .rotationEffect(.degrees(drift.degrees))
        .offset(x: drift.dx * fieldSize.width, y: drift.dy * fieldSize.height)
        .frame(width: size.width, height: size.height)
    }

    private func blob(_ cell: MeshCell, slot: CGSize) -> some View {
        let color = Self.color(cell.color)
        // CSS `circle at x y` sizes the gradient to the farthest corner of its box.
        let reach = hypot(max(cell.x, 1 - cell.x) * slot.width, max(cell.y, 1 - cell.y) * slot.height)
        return RadialGradient(
            stops: [
                .init(color: color, location: 0),
                .init(color: color, location: 0.38),
                .init(color: color.opacity(0), location: 0.74),
            ],
            center: UnitPoint(x: cell.x, y: cell.y),
            startRadius: 0,
            endRadius: reach
        )
        .frame(width: slot.width, height: slot.height)
    }

    static func color(_ sample: MeshSample) -> Color {
        Color(.sRGB, red: sample.red / 255, green: sample.green / 255, blue: sample.blue / 255)
    }
}

/// The cover at full size: a larger render when YouTube has one, the list thumbnail meanwhile
/// and whenever the larger one fails.
private struct NativeMacFullPlayerCover: View {
    let thumbnail: String?

    var body: some View {
        let original = thumbnail.flatMap(URL.init(string:))
        let larger = thumbnail.flatMap { NowPlayingMesh.highResolutionVariant(of: $0) }.flatMap(URL.init(string:))
        AsyncImage(url: larger) { phase in
            if case .success(let image) = phase {
                image.resizable().scaledToFill()
            } else {
                AsyncImage(url: original) { fallback in
                    if case .success(let image) = fallback {
                        image.resizable().scaledToFill()
                    } else {
                        Color.white.opacity(0.08)
                            .overlay(Image(systemName: "music.note").font(.system(size: 48)))
                    }
                }
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .shadow(color: .black.opacity(0.35), radius: 24, y: 12)
    }
}

/// Cover, title, progress, and transport on the left; lyrics on the right; over the mesh.
/// Everything stays mounted beneath it, including the official player surface, so opening and
/// closing this never interrupts playback.
struct NativeMacFullPlayer: View {
    @ObservedObject var store: NativeMacModelStore
    /// The mini player: the same layout in a small always-on-top window. Lyrics and the
    /// fill-the-screen controls step aside while it is small and come back as they were.
    var compact = false
    @Environment(\.openWindow) private var openWindow
    @Environment(\.goosicReduceMotion) private var reduceMotion
    @State private var palette: [MeshSample]?
    /// What the right-hand column shows: lyrics, the queue, or nothing, switched by the glass
    /// capsule at the bottom right as in Music.
    @State private var rightPane: RightPane = .lyrics
    @State private var lyricsMessage: String?

    enum RightPane { case lyrics, queue, none }
    @State private var scrubPosition: Double = 0
    @State private var isScrubbing = false

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }
    private var canControl: Bool { model.currentTrack != nil && model.serviceConnected && !busy }
    private var paletteFile: URL? { model.artworkFile(for: model.currentTrack?.thumbnail) }
    private var lyricsUnavailable: Bool {
        model.lyrics == nil && (model.lyricsStatus == "No lyrics were found for this track."
            || model.lyricsStatus.hasPrefix("Could not load lyrics:"))
    }
    private var visibleRightPane: RightPane {
        lyricsUnavailable && rightPane == .lyrics ? .none : rightPane
    }

    var body: some View {
        GeometryReader { proxy in
            let gutter = min(max(proxy.size.width * 0.07, 32), 128)
            let inset = compact ? 22 : min(max(proxy.size.width * 0.09, 40), 176)
            let cover = compact
                ? max(min(proxy.size.width - 44, proxy.size.height - 200), 80)
                : min(proxy.size.height * 0.48, 576)
            HStack(alignment: .center, spacing: gutter) {
                playerColumn(cover: cover)
                    .frame(maxWidth: 608)
                if !compact && visibleRightPane == .lyrics {
                    lyricsColumn
                        .frame(maxWidth: 768, maxHeight: min(proxy.size.height * 0.7, 832))
                        .transition(.opacity)
                } else if !compact && visibleRightPane == .queue {
                    NativeMacQueuePanel(store: store)
                        .frame(maxWidth: 520, maxHeight: min(proxy.size.height * 0.75, 860))
                        .transition(.opacity)
                }
            }
            .padding(.horizontal, inset)
            .padding(.top, compact ? 30 : 32)
            .padding(.bottom, compact ? 16 : 80)
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
        .background(background)
        // Volume and Exit are window toolbar items (see `NativeMacRootView`), not overlays here:
        // this view runs under the transparent title bar, and a slider drawn in that strip loses
        // its drag to the title bar, which moves the window instead.
        // Lyrics and Queue share one Liquid Glass capsule at the bottom right, as in Music; the
        // one showing is drawn filled.
        .overlay(alignment: .bottomTrailing) {
            if !compact {
                NativeMacGlassGroup {
                    HStack(spacing: 2) {
                        paneButton(.lyrics, "quote.bubble", "Lyrics")
                        paneButton(.queue, "list.bullet", "Up Next")
                    }
                    .padding(4)
                    .modifier(NativeMacGlassCapsule())
                }
                .padding(18)
            }
        }
        .overlay(alignment: .top) {
            if let lyricsMessage, !compact {
                Text(lyricsMessage)
                    .font(.subheadline.weight(.medium))
                    .padding(.horizontal, 18)
                    .padding(.vertical, 10)
                    .background(.regularMaterial, in: Capsule())
                    .padding(.top, 72)
                    .onAppear {
                        NSAccessibility.post(
                            element: NSApplication.shared,
                            notification: .announcementRequested,
                            userInfo: [
                                .announcement: lyricsMessage,
                                .priority: NSAccessibilityPriorityLevel.medium.rawValue,
                            ]
                        )
                    }
            }
        }
        .ignoresSafeArea()
        .environment(\.colorScheme, .dark)
        .task(id: paletteFile) {
            guard let file = paletteFile else { return }
            let sampled = await Task.detached(priority: .userInitiated) {
                NowPlayingPaletteLoader.palette(at: file)
            }.value
            guard !Task.isCancelled else { return }
            palette = sampled
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.25), value: rightPane)
        .onChange(of: model.currentTrack?.id) { _, _ in isScrubbing = false }
        .onChange(of: model.lyricsStatus) { _, _ in
            if lyricsUnavailable && rightPane == .lyrics && !compact {
                lyricsMessage = "Lyrics are not available for this song."
            }
        }
        .onAppear {
            if lyricsUnavailable && rightPane == .lyrics && !compact {
                lyricsMessage = "Lyrics are not available for this song."
            }
        }
        .task(id: lyricsMessage) {
            guard lyricsMessage != nil else { return }
            try? await Task.sleep(for: .seconds(3))
            if !Task.isCancelled { lyricsMessage = nil }
        }
    }

    private func close() { model.setFullPlayerOpen(false) }

    private func paneButton(_ pane: RightPane, _ symbol: String, _ title: String) -> some View {
        let active = visibleRightPane == pane
        return Button {
            if pane == .lyrics && lyricsUnavailable {
                lyricsMessage = "Lyrics are not available for this song."
                return
            }
            rightPane = active ? .none : pane
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 15, weight: .semibold))
                // The showing pane is a filled light circle with a dark glyph, as Music draws it.
                .foregroundStyle(active ? Color.black.opacity(0.85) : Color.white)
                .frame(width: 34, height: 34)
                .background(active ? Color.white.opacity(0.9) : Color.clear, in: Circle())
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(active ? "Hide \(title)" : title)
        .accessibilityLabel(title)
        .accessibilityAddTraits(active ? .isSelected : [])
    }

    // MARK: - Background

    private var background: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)
            if let palette {
                NowPlayingMeshBackground(palette: palette)
                    .id(palette)
                    .transition(.opacity)
            }
            // The old player's `bg-background/35`: enough to keep white text readable over a
            // pale cover without flattening the colour.
            Color(nsColor: .windowBackgroundColor).opacity(0.35)
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 1.15), value: palette)
        .ignoresSafeArea()
    }

    // MARK: - Player column

    private func playerColumn(cover: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            // Clicking the cover goes back, as it did in the previous Goosic.
            if compact {
                NativeMacFullPlayerCover(thumbnail: model.currentTrack?.thumbnail)
                    .frame(width: cover, height: cover)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .frame(maxWidth: .infinity)
                    .accessibilityLabel("Artwork")
            } else {
                Button(action: close) {
                    NativeMacFullPlayerCover(thumbnail: model.currentTrack?.thumbnail)
                        .frame(width: cover, height: cover)
                        // Video art is 16:9. Crop at the square so it stays in the column.
                        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(.plain)
                .frame(maxWidth: .infinity)
                .help("Exit full-screen player")
                .accessibilityLabel("Exit full-screen player")
            }

            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.isAdvertisement ? "Advertisement" : (model.currentTrack?.title ?? "Nothing playing"))
                        .font(compact ? .headline : .title3.weight(.semibold))
                        .lineLimit(1)
                    Text(busy ? "Preparing playback…" : fullSubtitle)
                        .font(compact ? .caption : .callout)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                // Like and More, each a small Liquid Glass circle beside the title, as in Music.
                NativeMacGlassGroup {
                    HStack(spacing: 8) {
                        Button(action: model.toggleLikeCurrentTrack) {
                            Image(systemName: model.isCurrentTrackLiked ? "heart.fill" : "heart")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(model.isCurrentTrackLiked ? AnyShapeStyle(.tint) : AnyShapeStyle(Color.white))
                                .frame(width: 30, height: 30)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)
                        .modifier(NativeMacGlassCircle())
                        .disabled(!model.canRateCurrentTrack || model.libraryOperationInProgress)
                        .help(model.isCurrentTrackLiked ? "Remove from Liked Music" : "Like")
                        .accessibilityLabel(model.isCurrentTrackLiked ? "Remove from Liked Music" : "Like")

                        if let track = model.currentTrack {
                            Menu {
                                NativeMacTrackMenuItems(track: track, model: model, includePlay: false)
                            } label: {
                                Image(systemName: "ellipsis")
                                    .font(.system(size: 13, weight: .bold))
                                    .foregroundStyle(Color.white)
                                    .frame(width: 30, height: 30)
                                    .contentShape(Circle())
                            }
                            .menuStyle(.button)
                            .buttonStyle(.plain)
                            .menuIndicator(.hidden)
                            .fixedSize()
                            .modifier(NativeMacGlassCircle())
                            .help("More")
                        }
                    }
                }
            }
            .padding(.top, compact ? 8 : 18)

            progress
            transport
        }
        .frame(width: cover)
    }

    /// "Artist — Album", as Music writes it under the title.
    private var fullSubtitle: String {
        guard let track = model.currentTrack else { return model.nowPlayingSubtitle }
        let parts = [track.artist, track.album].filter { !$0.isEmpty }
        return parts.isEmpty ? model.nowPlayingSubtitle : parts.joined(separator: " — ")
    }

    private var progress: some View {
        let total = max(model.duration, 1)
        let position = isScrubbing ? scrubPosition : min(model.displayedPosition, total)
        return VStack(spacing: 6) {
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(.white.opacity(0.22))
                    Capsule().fill(.white.opacity(0.85))
                        .frame(width: max(proxy.size.width * position / total, 0))
                }
                .frame(height: isScrubbing ? 7 : 5)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            guard model.isSeekable, !busy else { return }
                            isScrubbing = true
                            scrubPosition = min(max(value.location.x / proxy.size.width, 0), 1) * total
                        }
                        .onEnded { _ in
                            guard isScrubbing else { return }
                            model.seek(to: scrubPosition)
                            isScrubbing = false
                        }
                )
            }
            .frame(height: 14)
            HStack {
                Text(GoosicAppModel.timeText(position))
                Spacer()
                Text("−" + GoosicAppModel.timeText(max(total - position, 0)))
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .opacity(model.isSeekable ? 1 : 0.5)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: isScrubbing)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Playback position")
        .accessibilityValue("\(model.elapsedText) of \(model.durationText)")
        .accessibilityAdjustableAction { direction in
            guard model.isSeekable, !busy else { return }
            let step = direction == .increment ? 10.0 : -10.0
            model.seek(to: min(max(position + step, 0), total))
        }
        .focusable(model.isSeekable && !busy)
        .onMoveCommand { direction in
            guard model.isSeekable, !busy else { return }
            if direction == .left { model.seek(to: max(position - 10, 0)) }
            if direction == .right { model.seek(to: min(position + 10, total)) }
        }
    }

    private var transport: some View {
        HStack {
            control("Shuffle", "shuffle", size: 17, active: model.shuffle, action: model.toggleShuffle)
            Spacer()
            control("Previous track", "backward.fill", size: 24, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            Spacer()
            Button(action: model.togglePause) {
                ZStack {
                    if busy {
                        ProgressView().controlSize(.regular)
                    } else {
                        Image(systemName: model.isPaused ? "play.fill" : "pause.fill")
                            .font(.system(size: 34))
                    }
                }
                .frame(width: 52, height: 52)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(model.isPaused ? "Play" : "Pause")
            .keyboardShortcut(.space, modifiers: [])
            .disabled(!canControl)
            Spacer()
            control("Next track", "forward.fill", size: 24, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
            Spacer()
            control(model.repeatMode.label, model.repeatMode == .one ? "repeat.1" : "repeat", size: 17,
                    active: model.repeatMode != .off, action: model.cycleRepeatMode)
        }
    }

    private func control(_ title: String, _ symbol: String, size: CGFloat, active: Bool = false,
                         action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size))
                // `.tint` is the app's pink, set once at the root.
                .foregroundStyle(active ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
                .frame(width: 40, height: 40)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .help(title)
    }

    // MARK: - Lyrics

    /// Where the active line settles, as a fraction of the column's height. Lower than centre, so
    /// more of what is coming is visible than of what has passed.
    private static let activeLineAnchor = UnitPoint(x: 0, y: 0.36)

    @ViewBuilder
    private var lyricsColumn: some View {
        if let lyrics = model.lyrics {
            let active = model.activeLyricIndex
            ScrollViewReader { reader in
                ScrollView(showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 26) {
                        ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                            lyricLine(line, isActive: index == active, synced: lyrics.synced)
                                .id(index)
                        }
                    }
                    .padding(.vertical, 160)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .animation(reduceMotion ? nil : .easeInOut(duration: 0.4), value: active)
                }
                .onAppear {
                    if let active { reader.scrollTo(active, anchor: Self.activeLineAnchor) }
                }
                .onChange(of: active) { _, next in
                    guard let next else { return }
                    withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.72)) {
                        reader.scrollTo(next, anchor: Self.activeLineAnchor)
                    }
                }
            }
            .mask(
                LinearGradient(
                    stops: [
                        .init(color: .clear, location: 0),
                        .init(color: .black, location: 0.14),
                        .init(color: .black, location: 0.84),
                        .init(color: .clear, location: 1),
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
        } else {
            VStack(alignment: .leading, spacing: 10) {
                Image(systemName: "quote.bubble")
                    .font(.system(size: 30))
                Text(model.lyricsStatus)
                    .font(.title3.weight(.semibold))
            }
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }

    private func lyricLine(_ line: GoosicLyricsLine, isActive: Bool, synced: Bool) -> some View {
        // Unsynced lyrics are shown plainly: guessing which line is current would be worse.
        let dimmed = synced && !isActive
        return Text(line.text.isEmpty ? "♪" : line.text)
            .font(.system(size: 30, weight: .bold))
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .opacity(dimmed ? 0.3 : (synced ? 1 : 0.85))
            .blur(radius: dimmed ? 1.5 : 0)
            .scaleEffect(isActive ? 1 : 0.97, anchor: .leading)
            .contentShape(Rectangle())
            .onTapGesture {
                guard synced, model.isSeekable, !busy else { return }
                model.seek(to: Double(line.atMs) / 1_000)
            }
    }
}

/// The full-screen player's volume control, placed in the window toolbar while that player is
/// open. As a toolbar item it owns its drag, and the system draws it on Liquid Glass.
struct NativeMacFullPlayerVolume: View {
    @ObservedObject var store: NativeMacModelStore

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }

    // Music's order: the slider, then the speaker, which mutes.
    var body: some View {
        HStack(spacing: 10) {
            slider
                .frame(width: 130)
            Button(action: model.toggleMuted) {
                Image(systemName: model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                    .frame(width: 20)
            }
            .buttonStyle(.plain)
            .help(model.isMuted ? "Unmute" : "Mute")
            .accessibilityLabel(model.isMuted ? "Unmute" : "Mute")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .disabled(busy || model.isAdvertisement)
    }

    private var shownVolume: Double { model.isMuted ? 0 : model.volume }

    @ViewBuilder
    private var slider: some View {
        if #available(macOS 26.0, *) {
            NativeMacVolumeSlider(value: shownVolume, isEnabled: !busy && !model.isAdvertisement) {
                model.setVolume($0)
            }
        } else {
            Slider(value: Binding(get: { shownVolume }, set: { model.setVolume($0) }), in: 0...1)
                .controlSize(.small)
                .accessibilityLabel("Volume")
        }
    }
}

/// AppKit's own `NSSlider`, which on macOS 26 draws the system Liquid Glass track and knob.
@available(macOS 26.0, *)
private struct NativeMacVolumeSlider: NSViewRepresentable {
    let value: Double
    let isEnabled: Bool
    let onChange: (Double) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onChange: onChange) }

    func makeNSView(context: Context) -> NSSlider {
        let slider = NSSlider(
            value: value,
            minValue: 0,
            maxValue: 1,
            target: context.coordinator,
            action: #selector(Coordinator.changed(_:))
        )
        slider.isContinuous = true
        slider.controlSize = .small
        slider.isEnabled = isEnabled
        slider.setAccessibilityLabel("Volume")
        return slider
    }

    func updateNSView(_ slider: NSSlider, context: Context) {
        context.coordinator.onChange = onChange
        slider.isEnabled = isEnabled
        // The model echoes each step of a drag back here; only move the knob for a change that
        // came from somewhere else, such as mute or the player bar.
        if abs(slider.doubleValue - value) > 0.001 {
            slider.doubleValue = value
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var onChange: (Double) -> Void

        init(onChange: @escaping (Double) -> Void) {
            self.onChange = onChange
        }

        @objc func changed(_ sender: NSSlider) {
            onChange(sender.doubleValue)
        }
    }
}
/// Groups Liquid Glass shapes so they are drawn and morph together, as system controls are.
struct NativeMacGlassGroup<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        if #available(macOS 26.0, *) {
            GlassEffectContainer { content }
        } else {
            content
        }
    }
}

/// A transparent, interactive Liquid Glass circle; a thin material before macOS 26.
struct NativeMacGlassCircle: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .circle)
        } else {
            content.background(.ultraThinMaterial, in: Circle())
        }
    }
}

/// A transparent, interactive Liquid Glass capsule; a thin material before macOS 26.
struct NativeMacGlassCapsule: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 26.0, *) {
            content.glassEffect(.regular.interactive(), in: .capsule)
        } else {
            content.background(.ultraThinMaterial, in: Capsule())
        }
    }
}
#endif
