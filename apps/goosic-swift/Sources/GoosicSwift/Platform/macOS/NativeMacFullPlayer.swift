#if os(macOS)
/*
 * NativeMacFullPlayer.swift — the immersive full-screen player for the native macOS shell.
 *
 * Layout and behaviour ported from the previous Goosic application's full-screen player
 * (src/components/layout/player-bar.tsx, variant "fullscreen", and lyrics-view.tsx).
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
import Accelerate
import AppKit
import ImageIO
import SwiftUI

/// Bakes one small texture off the main actor. Core Animation only sees the finished image.
enum NowPlayingArtworkTexture {
    private static let side = 128
    private static let rowBytes = side * 4
    private static let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
        | CGBitmapInfo.byteOrder32Big.rawValue
    private static let colourSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    @MainActor static var last: (file: URL, image: CGImage)?

    static func make(from file: URL) -> CGImage? {
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let cover = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: side,
              ] as CFDictionary),
              let context = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8,
                                      bytesPerRow: rowBytes, space: colourSpace, bitmapInfo: bitmapInfo),
              let bitmap = context.data else { return nil }

        let extent = CGRect(x: 0, y: 0, width: side, height: side)
        let scale = CGFloat(side) / CGFloat(min(cover.width, cover.height))
        let width = CGFloat(cover.width) * scale
        let height = CGFloat(cover.height) * scale
        context.setFillColor(gray: 0, alpha: 1)
        context.fill(extent)
        context.draw(cover, in: CGRect(x: (extent.width - width) / 2, y: (extent.height - height) / 2,
                                       width: width, height: height))

        let input = bitmap.assumingMemoryBound(to: UInt32.self)
        var warped = withExtendedLifetime(context) { textureMap.map { input[Int($0)] } }
        var blurred = Data(count: side * rowBytes)
        let result = warped.withUnsafeMutableBufferPointer { pixels in
            blurred.withUnsafeMutableBytes { output in
                var source = vImage_Buffer(data: pixels.baseAddress, height: UInt(side),
                                           width: UInt(side), rowBytes: rowBytes)
                var destination = vImage_Buffer(data: output.baseAddress, height: UInt(side),
                                                width: UInt(side), rowBytes: rowBytes)
                // A separable tent filter gives a wide, soft blur without a live GPU effect.
                let error = vImageTentConvolve_ARGB8888(&source, &destination, nil, 0, 0, 37, 37, nil,
                                                       vImage_Flags(kvImageEdgeExtend | kvImageDoNotTile))
                guard error == kvImageNoError else { return error }
                let rgba = output.bindMemory(to: UInt8.self)
                for index in stride(from: 0, to: rgba.count, by: 4) {
                    let red = Float(rgba[index])
                    let green = Float(rgba[index + 1])
                    let blue = Float(rgba[index + 2])
                    let luma = red * 0.2126 + green * 0.7152 + blue * 0.0722
                    rgba[index] = saturated(red, luma: luma)
                    rgba[index + 1] = saturated(green, luma: luma)
                    rgba[index + 2] = saturated(blue, luma: luma)
                }
                return error
            }
        }
        guard result == kvImageNoError,
              let provider = CGDataProvider(data: blurred as CFData) else { return nil }
        return CGImage(width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: rowBytes,
                       space: colourSpace, bitmapInfo: CGBitmapInfo(rawValue: bitmapInfo), provider: provider,
                       decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    private static func saturated(_ channel: Float, luma: Float) -> UInt8 {
        UInt8(clamping: Int((luma + (channel - luma) * 1.35).rounded()))
    }

    /// Inverse texture coordinates for a local twist. The blur softens the rounded samples.
    private static let textureMap: [UInt16] = (0..<(side * side)).map { index in
        let centre = Double(side - 1) / 2
        let x = Double(index % side) - centre
        let y = Double(index / side) - centre
        let weight = max(0, 1 - hypot(x, y) / (Double(side) * 0.66))
        let angle = 2.9 * weight * weight
        let cosine = cos(angle)
        let sine = sin(angle)
        let column = min(side - 1, max(0, Int((centre + x * cosine - y * sine).rounded())))
        let row = min(side - 1, max(0, Int((centre + x * sine + y * cosine).rounded())))
        return UInt16(row * side + column)
    }
}

private struct NowPlayingArtworkBackground: NSViewRepresentable {
    let file: URL?
    let animate: Bool

    func makeNSView(context: Context) -> ArtworkLayers { ArtworkLayers() }

    func updateNSView(_ view: ArtworkLayers, context: Context) {
        view.update(file: file, animate: animate)
    }

    @MainActor
    final class ArtworkLayers: NSView {
        private let base = CALayer()
        private let overlay = CALayer()
        private var animate = false
        private var file: URL?
        private var renderedFile: URL?
        private var renderTask: Task<Void, Never>?

        init() {
            super.init(frame: .zero)
            wantsLayer = true
            layer?.masksToBounds = true
            for texture in [base, overlay] {
                texture.contentsGravity = .resizeAspectFill
                texture.masksToBounds = true
                layer?.addSublayer(texture)
            }
            overlay.opacity = 0.68
            // An off-centre pivot makes colours travel across the field as the layer turns.
            overlay.anchorPoint = CGPoint(x: 0.28, y: 0.69)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

        func update(file: URL?, animate: Bool) {
            if self.file != file {
                self.file = file
                cancelRender()
                renderedFile = nil
                setImage(nil)
            }
            self.animate = animate
            updateVisibility()
        }

        private func setImage(_ image: CGImage?) {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            base.contents = image
            overlay.contents = image
            CATransaction.commit()
        }

        private func cancelRender() {
            renderTask?.cancel()
            renderTask = nil
        }

        private func prepareArtwork() {
            guard let file, renderedFile != file, renderTask == nil else { return }
            if let last = NowPlayingArtworkTexture.last, last.file == file {
                renderedFile = file
                setImage(last.image)
                return
            }

            renderTask = Task { [weak self] in
                let render = Task.detached(priority: .utility) {
                    Task.isCancelled ? nil : NowPlayingArtworkTexture.make(from: file)
                }
                let image = await withTaskCancellationHandler {
                    await render.value
                } onCancel: {
                    render.cancel()
                }
                guard !Task.isCancelled, let self else { return }
                renderTask = nil
                renderedFile = file
                if let image { NowPlayingArtworkTexture.last = (file, image) }
                setImage(image)
                updateMotion()
            }
        }

        override func layout() {
            super.layout()
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            base.frame = bounds
            overlay.frame = bounds.insetBy(dx: -bounds.width * 0.23, dy: -bounds.height * 0.23)
            CATransaction.commit()
        }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            NotificationCenter.default.removeObserver(self, name: NSWindow.didChangeOcclusionStateNotification,
                                                      object: window)
            if newWindow == nil { cancelRender() }
            super.viewWillMove(toWindow: newWindow)
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window {
                NotificationCenter.default.addObserver(self, selector: #selector(updateVisibility),
                                                       name: NSWindow.didChangeOcclusionStateNotification, object: window)
            }
            updateVisibility()
        }

        private var visible: Bool { window?.occlusionState.contains(.visible) == true }

        @objc private func updateVisibility() {
            if visible {
                prepareArtwork()
            } else {
                cancelRender()
            }
            updateMotion()
        }

        private func updateMotion() {
            guard animate, visible, overlay.contents != nil else {
                overlay.removeAnimation(forKey: "drift")
                return
            }
            guard overlay.animation(forKey: "drift") == nil else { return }
            let drift = CAKeyframeAnimation(keyPath: "transform.rotation.z")
            drift.values = [0, 0.29, 0, -0.29, 0]
            drift.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .easeInEaseOut), count: 4)
            drift.duration = 22
            drift.repeatCount = .infinity
            overlay.add(drift, forKey: "drift")
        }
    }
}

/// The cover at full size: a larger render when YouTube has one, the list thumbnail meanwhile
/// and whenever the larger one fails.
private struct NativeMacFullPlayerCover: View {
    let thumbnail: String?
    /// Width over height, from the picture itself.
    var aspect: Double = 1
    /// The cover steps back while playback is paused and comes forward again when it resumes.
    var paused = false
    @Environment(\.goosicReduceMotion) private var reduceMotion

    var body: some View {
        NativeMacCachedImage(url: NowPlayingMesh.highResolutionVariant(of: thumbnail ?? ""), maxPixels: 1200) { larger in
            if let larger {
                larger.resizable().scaledToFill()
            } else {
                NativeMacCachedImage(url: thumbnail, maxPixels: 1200) { image in
                    if let image {
                        image.resizable().scaledToFill()
                    } else {
                        Color.white.opacity(0.08)
                            .overlay(Image(systemName: "music.note").font(.system(size: 48)))
                    }
                }
            }
        }
        .aspectRatio(aspect, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .shadow(color: .black.opacity(paused ? 0.22 : 0.35), radius: paused ? 12 : 24, y: paused ? 6 : 12)
        .scaleEffect(paused ? 0.74 : 1)
        .animation(reduceMotion ? nil : .snappy(duration: 0.5, extraBounce: 0.1), value: paused)
    }
}

/// Cover, title, progress, and transport on the left; lyrics on the right; over the artwork backdrop.
/// Everything stays mounted beneath it, including the official player surface, so opening and
/// closing this never interrupts playback.
struct NativeMacFullPlayer: View {
    @ObservedObject var store: NativeMacModelStore
    /// The timeline's position, observed here because the model no longer publishes it.
    @ObservedObject var clock: PlaybackClock
    /// The mini player: the same layout in a small always-on-top window. Lyrics and the
    /// fill-the-screen controls step aside while it is small and come back as they were.
    var compact = false
    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow
    @Environment(\.goosicReduceMotion) private var reduceMotion
    @SwiftUI.State private var surfaceSettled = false
    /// What the right-hand column shows: lyrics, the queue, or nothing, switched by the glass
    /// capsule at the bottom right as in Music.
    @SwiftUI.State private var rightPane: RightPane = .lyrics
    @SwiftUI.State private var lyricsMessage: String?

    enum RightPane { case lyrics, queue, none }
    @SwiftUI.State private var scrubPosition: Double = 0
    @SwiftUI.State private var isScrubbing = false
    @FocusState private var progressFocused: Bool

    private var model: GoosicAppModel { store.model }
    private var busy: Bool { model.accountOperationInProgress || model.playbackTransition != .idle }
    private var canControl: Bool { model.currentTrack != nil && model.serviceConnected && !busy }
    private var backdropFile: URL? { model.artworkFile(for: model.currentTrack?.thumbnail) }
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
                ? max(min(proxy.size.width - 44, proxy.size.height - 280), 60)
                : min(proxy.size.height * 0.48, 576)
            HStack(alignment: .center, spacing: gutter) {
                playerColumn(cover: cover, width: compact ? proxy.size.width - 44 : cover)
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
            .padding(.top, compact ? 44 : 32)
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
        .overlay(alignment: .topLeading) {
            if compact {
                Button("Back to Goosic", systemImage: "arrow.up.left.and.arrow.down.right") {
                    openWindow(id: "main")
                    model.setFullPlayerOpen(true)
                    dismissWindow(id: NativeMacMiniPlayer.windowID)
                }
                .buttonStyle(.bordered)
                .padding(.leading, 22)
                .padding(.top, 32)
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
        .task {
            try? await Task.sleep(for: .seconds(0.45))
            if !Task.isCancelled { surfaceSettled = true }
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
                .animation(reduceMotion ? nil : .smooth(duration: 0.22), value: active)
                .contentShape(Circle())
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .help(active ? "Hide \(title)" : title)
        .accessibilityLabel(title)
        .accessibilityAddTraits(active ? .isSelected : [])
    }

    // MARK: - Background

    private var background: some View {
        ZStack {
            Color.black
            NowPlayingArtworkBackground(file: backdropFile,
                                        animate: !reduceMotion && surfaceSettled && (compact || model.fullPlayerOpen))
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            Color.black.opacity(0.56)
        }
        .ignoresSafeArea()
    }

    // MARK: - Player column

    private func playerColumn(cover: CGFloat, width: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: compact ? 8 : 14) {
            // The cover keeps its picture's shape inside the square it used to fill: a song
            // stays square, a video is as wide and shorter, and the controls move up to meet it.
            // Clicking the cover goes back, as it did in the previous Goosic.
            NativeMacCoverAspectReader(thumbnail: model.currentTrack?.thumbnail) { aspect in
                let fitted = CoverShape.fit(aspect: aspect, maxWidth: cover, maxHeight: cover)
                let artwork = NativeMacFullPlayerCover(
                    thumbnail: model.currentTrack?.thumbnail, aspect: aspect, paused: model.isPaused
                )
                .frame(width: fitted.width, height: fitted.height)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                Group {
                    if compact {
                        artwork
                            .accessibilityLabel("Artwork")
                    } else {
                        Button(action: close) { artwork }
                            .buttonStyle(NativeMacPlayerButtonStyle())
                            .help("Exit full-screen player")
                            .accessibilityLabel("Exit full-screen player")
                    }
                }
                .frame(maxWidth: .infinity)
                .animation(reduceMotion ? nil : .smooth(duration: 0.35), value: aspect)
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
                                .modifier(NativeMacSymbolTransition(symbol: model.isCurrentTrackLiked ? "heart.fill" : "heart"))
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(model.isCurrentTrackLiked ? AnyShapeStyle(.tint) : AnyShapeStyle(Color.white))
                                .frame(width: 30, height: 30)
                                .contentShape(Circle())
                        }
                        .buttonStyle(NativeMacPlayerButtonStyle())
                        .modifier(NativeMacGlassCircle())
                        .disabled(!model.canRateCurrentTrack || model.libraryOperationInProgress)
                        .help(model.isCurrentTrackLiked ? "Remove from Liked Music" : "Like")
                        .accessibilityLabel(model.isCurrentTrackLiked ? "Remove from Liked Music" : "Like")

                        if let track = model.currentTrack {
                            Menu {
                                Button("Import Timed Lyrics…", systemImage: "text.badge.plus") {
                                    NativeMacLyricsImport.open(model: model)
                                }
                                NativeMacTrackMenuItems(
                                    track: track, model: model, includePlay: false,
                                    onNavigate: compact ? {
                                        openWindow(id: "main")
                                        dismissWindow(id: NativeMacMiniPlayer.windowID)
                                    } : nil
                                )
                                if compact {
                                    Divider()
                                    Button(model.shuffle ? "Turn shuffle off" : "Turn shuffle on", action: model.toggleShuffle)
                                    Button(model.repeatMode.label, action: model.cycleRepeatMode)
                                }
                            } label: {
                                Image(systemName: "ellipsis")
                                    .font(.system(size: 13, weight: .bold))
                                    .foregroundStyle(Color.white)
                                    .frame(width: 30, height: 30)
                                    .contentShape(Circle())
                            }
                            .menuStyle(.button)
                            .buttonStyle(NativeMacPlayerButtonStyle())
                            .menuIndicator(.hidden)
                            .fixedSize()
                            .modifier(NativeMacGlassCircle())
                            .help("More")
                            .accessibilityLabel("More playback actions")
                        }
                    }
                }
            }
            .padding(.top, compact ? 0 : 18)

            progress
            if compact {
                HStack(spacing: 18) {
                    control("Previous track", "backward.fill", size: 20, action: model.previous)
                        .disabled(!canControl || model.isAdvertisement)
                    playPause
                    control("Next track", "forward.fill", size: 20, action: model.next)
                        .disabled(!canControl || model.isAdvertisement)
                }
                .frame(maxWidth: .infinity)
                NativeMacFullPlayerVolume(store: store)
                    .frame(maxWidth: .infinity)
            } else {
                transport
            }
        }
        .frame(width: width)
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
                            progressFocused = false
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
        .focused($progressFocused)
        .focusEffectDisabled()
        .overlay {
            if progressFocused {
                RoundedRectangle(cornerRadius: 4).stroke(.white.opacity(0.45), lineWidth: 1)
                    .allowsHitTesting(false)
            }
        }
        .onExitCommand { progressFocused = false }
        .onChange(of: model.currentTrack?.id) { _, _ in progressFocused = false }
        .onMoveCommand { direction in
            guard model.isSeekable, !busy else { return }
            if direction == .left { model.seek(to: max(position - 10, 0)) }
            if direction == .right { model.seek(to: min(position + 10, total)) }
        }
    }

    private var transport: some View {
        HStack {
            control(model.shuffle ? "Turn shuffle off" : "Turn shuffle on", "shuffle", size: 17, active: model.shuffle, action: model.toggleShuffle)
            Spacer()
            control("Previous track", "backward.fill", size: 24, action: model.previous)
                .disabled(!canControl || model.isAdvertisement)
            Spacer()
            playPause
            Spacer()
            control("Next track", "forward.fill", size: 24, action: model.next)
                .disabled(!canControl || model.isAdvertisement)
            Spacer()
            control(model.repeatMode.label, model.repeatMode == .one ? "repeat.1" : "repeat", size: 17,
                    active: model.repeatMode != .off, action: model.cycleRepeatMode)
        }
    }

    private var playPause: some View {
        Button(action: model.togglePause) {
            ZStack {
                Image(systemName: model.isPaused ? "play.fill" : "pause.fill")
                    .font(.system(size: 34))
                    .modifier(NativeMacSymbolTransition(symbol: model.isPaused ? "play.fill" : "pause.fill"))
                    .opacity(busy ? 0 : 1)
                if busy { ProgressView().controlSize(.regular) }
            }
            .frame(width: 52, height: 52)
            .contentShape(Rectangle())
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .accessibilityLabel(model.isPaused ? "Play" : "Pause")

        .disabled(!canControl)
    }

    private func control(_ title: String, _ symbol: String, size: CGFloat, active: Bool = false,
                         action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size))
                .modifier(NativeMacSymbolTransition(symbol: symbol))
                // `.tint` is the app's pink, set once at the root.
                .foregroundStyle(active ? AnyShapeStyle(.tint) : AnyShapeStyle(.primary))
                .frame(width: 40, height: 40)
                .contentShape(Rectangle())
                .background(active ? Color.white.opacity(0.16) : .clear, in: Circle())
                .animation(reduceMotion ? nil : .easeOut(duration: 0.18), value: active)
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .accessibilityLabel(title)
        .accessibilityAddTraits(active ? .isSelected : [])
        .help(title)
    }

    // MARK: - Lyrics

    private var lyricsColumn: some View {
        NativeMacLyricsView(store: store, clock: store.model.lyricsClock, immersive: true)
    }

}

/// The full-screen player's volume control, placed in the window toolbar while that player is
/// open. As a toolbar item it owns its drag; the root applies the same glass and appearance
/// as the bottom Lyrics/Queue capsule.
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
                    .modifier(NativeMacSymbolTransition(symbol: model.isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill"))
                    .frame(width: 20)
            }
            .buttonStyle(NativeMacPlayerButtonStyle())
            .help(model.isMuted ? "Unmute" : "Mute")
            .accessibilityLabel(model.isMuted ? "Unmute" : "Mute")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .disabled(busy)
    }

    private var shownVolume: Double { model.isMuted ? 0 : model.volume }

    private var slider: some View {
        NativeMacVolumeSlider(value: shownVolume, isEnabled: !busy) {
            model.setVolume($0)
        }
    }
}

/// AppKit's own `NSSlider`, which on macOS 26 draws the system Liquid Glass track and knob.
///
/// `value` and `onChange` are gains; the knob works in positions on the shared audio taper
/// (`VolumeTaper`), so half way up is quiet rather than nearly full volume.
struct NativeMacVolumeSlider: NSViewRepresentable {
    let value: Double
    let isEnabled: Bool
    let onChange: (Double) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onChange: onChange) }

    func makeNSView(context: Context) -> NSSlider {
        let slider = ScrollableVolumeSlider(
            value: VolumeTaper.position(forGain: value),
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
        // While the knob is held it belongs to the hand: the model's echo of an earlier step, or
        // the player's report of its own volume, would otherwise pull it back mid-drag.
        guard !context.coordinator.isHeld else { return }
        let position = VolumeTaper.position(forGain: value)
        if abs(slider.doubleValue - position) > 0.001 {
            slider.doubleValue = position
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var onChange: (Double) -> Void
        private(set) var isDragging = false
        private var lastSent = Date.distantPast
        private var pending: Double?
        /// How often a drag reaches the player. Every step used to publish the model, run
        /// JavaScript in the web player and schedule a save, which is what made the knob lag.
        private static let interval: TimeInterval = 1.0 / 30.0

        init(onChange: @escaping (Double) -> Void) {
            self.onChange = onChange
        }

        @objc func changed(_ sender: NSSlider) {
            let gain = VolumeTaper.gain(atPosition: sender.doubleValue)
            let released = NSApp.currentEvent?.type == .leftMouseUp
            isDragging = NSApp.currentEvent?.type == .leftMouseDragged
                || NSApp.currentEvent?.type == .leftMouseDown
            // A click, a key or the wheel lands at once, and so does the end of a drag.
            guard isDragging, !released else {
                isDragging = false
                pending = nil
                send(gain)
                return
            }
            let wait = Self.interval - Date().timeIntervalSince(lastSent)
            if wait <= 0 {
                send(gain)
                return
            }
            let scheduled = pending != nil
            pending = gain
            guard !scheduled else { return }
            DispatchQueue.main.asyncAfter(deadline: .now() + wait) { [weak self] in
                guard let self, let pending = self.pending else { return }
                self.send(pending)
            }
        }

        /// Whether the knob is still held; also true of nothing once the button is up, in case
        /// the slider ends a drag without a final action.
        var isHeld: Bool { isDragging && NSEvent.pressedMouseButtons & 1 != 0 }

        private func send(_ gain: Double) {
            pending = nil
            lastSent = Date()
            onChange(gain)
        }
    }
}

/// Uses the same model action for dragging, keyboard input and wheel adjustments.
private final class ScrollableVolumeSlider: NSSlider {
    override func scrollWheel(with event: NSEvent) {
        guard isEnabled else { return }
        let delta = event.scrollingDeltaY != 0 ? event.scrollingDeltaY : event.scrollingDeltaX
        guard delta != 0 else { return }
        let step = event.hasPreciseScrollingDeltas ? delta * 0.002 : (delta > 0 ? 0.05 : -0.05)
        doubleValue = min(max(doubleValue + step, minValue), maxValue)
        sendAction(action, to: target)
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
