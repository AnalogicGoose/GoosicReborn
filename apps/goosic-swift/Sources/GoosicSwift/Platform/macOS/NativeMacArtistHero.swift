#if os(macOS)
import ImageIO
import SwiftUI

/// How far an artist page has scrolled past its hero, from 0 at rest to 1 once the hero is gone.
///
/// A reference held in plain `@State` and observed only by the backdrop, so a scroll redraws the
/// picture and not the page's songs and shelves.
@MainActor
final class NativeMacArtistScroll: ObservableObject {
    @Published private(set) var progress: CGFloat = 0

    func update(_ value: CGFloat) {
        // Hundredths are finer than a blur radius can show, and spare a redraw per point scrolled.
        let next = (min(max(value, 0), 1) * 100).rounded() / 100
        if next != progress { progress = next }
    }
}

/// The top of an artist page, laid out as Music lays it out: the name centred near the foot of
/// the artist's banner, and About, Play and Subscribe under it.
///
/// The banner itself is `NativeMacArtistBackdrop`, which stays where it is behind the page while
/// this scrolls over it. Everything shown comes from the page's `artist` header, and
/// `ArtistHero` decides the height and the colour, the same way on every platform.
struct NativeMacArtistHero: SwiftUI.View {
    let header: GoosicArtistHeader
    let name: String
    let channelID: String
    /// The top songs as shown, which Play queues in order.
    let tracks: [GoosicTrack]
    let source: QueueSource
    let key: CatalogKey
    let width: CGFloat
    let scroll: NativeMacArtistScroll
    let model: GoosicAppModel

    @SwiftUI.Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @SwiftUI.State private var showsAbout = false

    private var height: CGFloat { CGFloat(ArtistHero.height(forWidth: Double(width))) }
    private var playingHere: Bool { model.isPlayingFrom(key) }
    private var showsPause: Bool { playingHere && !model.isPaused }

    var body: some SwiftUI.View {
        SwiftUI.VStack(spacing: 10) {
            SwiftUI.Text(name)
                .font(.system(size: 52, weight: .bold))
                .lineLimit(2)
                .minimumScaleFactor(0.6)
                .multilineTextAlignment(.center)
                .shadow(color: .black.opacity(0.25), radius: 12, y: 2)
            if let audience = header.audience {
                SwiftUI.Text(audience)
                    .font(.callout.weight(.medium))
                    .foregroundStyle(.white.opacity(0.8))
            }
            actions.padding(.top, 14)
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 32)
        .padding(.bottom, 30)
        .frame(width: width, height: height, alignment: .bottom)
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .scrollView).minY } action: { top in
            scroll.update(-top / height)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(name)
    }

    private var actions: some SwiftUI.View {
        SwiftUI.HStack(spacing: 26) {
            roundButton("About", "info", size: 46) { showsAbout = true }
                .disabled(header.description == nil && header.subscribers == nil)
                .popover(isPresented: $showsAbout, arrowEdge: .bottom) { about }

            SwiftUI.Button {
                if playingHere {
                    model.togglePause()
                } else if let first = tracks.first {
                    model.launch(.ordered(first, tracks), source: source)
                }
            } label: {
                SwiftUI.Image(systemName: showsPause ? "pause.fill" : "play.fill")
                    .font(.system(size: 28, weight: .semibold))
                    .modifier(NativeMacSymbolTransition(symbol: showsPause ? "pause.fill" : "play.fill"))
                    .foregroundStyle(.black)
                    .frame(width: 72, height: 72)
                    .background(SwiftUI.Circle().fill(.white))
                    .contentShape(SwiftUI.Circle())
            }
            .buttonStyle(NativeMacPlayerButtonStyle())
            .help(showsPause ? "Pause" : "Play")
            .accessibilityLabel(showsPause ? "Pause" : "Play \(name)")
            .disabled(tracks.isEmpty && !playingHere)

            if model.activeAccount != nil {
                let subscribed = model.isSubscribed(to: channelID)
                roundButton(
                    subscribed ? "Unsubscribe" : "Subscribe",
                    subscribed ? "star.fill" : "star",
                    size: 46
                ) {
                    model.setSubscribed(!subscribed, to: channelID, name: name)
                }
                .disabled(model.libraryOperationInProgress)
            } else {
                // Keeps Play centred under the name when there is no account to subscribe with.
                SwiftUI.Color.clear.frame(width: 46, height: 46).accessibilityHidden(true)
            }
        }
    }

    private func roundButton(
        _ label: String, _ symbol: String, size: CGFloat, action: @escaping () -> Void
    ) -> some SwiftUI.View {
        SwiftUI.Button(action: action) {
            SwiftUI.Image(systemName: symbol)
                .font(.system(size: 17, weight: .semibold))
                .frame(width: size, height: size)
                .background(SwiftUI.Circle().fill(.white.opacity(reduceTransparency ? 0.28 : 0.16)))
                .overlay(SwiftUI.Circle().strokeBorder(.white.opacity(0.18)))
                .contentShape(SwiftUI.Circle())
        }
        .buttonStyle(NativeMacPlayerButtonStyle())
        .help(label)
        .accessibilityLabel(label)
    }

    private var about: some SwiftUI.View {
        SwiftUI.ScrollView {
            SwiftUI.VStack(alignment: .leading, spacing: 10) {
                SwiftUI.Text(name).font(.title2.weight(.bold))
                let facts = [header.audience, header.subscribers.map { "\($0) subscribers" }]
                    .compactMap { $0 }
                if !facts.isEmpty {
                    SwiftUI.Text(facts.joined(separator: " · "))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                if let description = header.description {
                    SwiftUI.Text(description)
                        .font(.body)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(width: 420)
        .frame(maxHeight: 460)
    }
}

/// The artist's picture behind the hero, fixed in place while the page scrolls over it.
///
/// As the hero scrolls away the picture blurs and then dissolves into the page colour, so by the
/// time the hero has gone the page is the artist's colour alone, as in Music. Without a banner
/// the header's portrait is a circle near the top, as the previous Goosic showed it, and with
/// neither it is the page colour alone; a song's cover is never used in their place.
struct NativeMacArtistBackdrop: SwiftUI.View {
    let header: GoosicArtistHeader
    let tint: SwiftUI.Color
    let width: CGFloat
    @SwiftUI.ObservedObject var scroll: NativeMacArtistScroll

    @SwiftUI.Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @SwiftUI.Environment(\.colorSchemeContrast) private var contrast

    private var height: CGFloat { CGFloat(ArtistHero.height(forWidth: Double(width))) }
    /// Full blur by the time half the hero has scrolled away; reduced transparency skips the blur
    /// and only fades.
    private var blur: CGFloat { reduceTransparency ? 0 : min(scroll.progress / 0.5, 1) * 30 }
    /// The picture starts dissolving at a third of the hero and is gone with the hero's foot.
    private var visibility: CGFloat { 1 - min(max((scroll.progress - 0.3) / 0.6, 0), 1) }

    var body: some SwiftUI.View {
        SwiftUI.ZStack(alignment: .bottom) {
            tint
            picture
                .blur(radius: blur, opaque: true)
                .opacity(visibility)
            scrim
        }
        .frame(width: width, height: height)
        .clipped()
        .accessibilityHidden(true)
    }

    @SwiftUI.ViewBuilder
    private var picture: some SwiftUI.View {
        if let banner = header.banner {
            NativeMacCachedImage(url: banner, maxPixels: 2880) { image in
                // The clear placeholder matters: a view that draws nothing never appears, and the
                // decode is started by the image view appearing.
                SwiftUI.ZStack {
                    tint
                    if let image {
                        // Faces sit in a banner's upper part, so a taller hero crops from the foot.
                        image.resizable().scaledToFill()
                            .frame(width: width, height: height, alignment: .top)
                            .clipped()
                            .transition(.opacity)
                    }
                }
                .frame(width: width, height: height)
            }
        } else if let portrait = header.portrait {
            NativeMacCachedImage(url: portrait, maxPixels: 640) { image in
                SwiftUI.ZStack {
                    SwiftUI.Circle().fill(.white.opacity(0.08))
                    if let image {
                        image.resizable().scaledToFill()
                    } else {
                        SwiftUI.Image(systemName: "music.mic")
                            .font(.system(size: 64, weight: .light))
                            .foregroundStyle(.white.opacity(0.5))
                    }
                }
                .frame(width: portraitSide, height: portraitSide)
                .clipShape(SwiftUI.Circle())
                .shadow(color: .black.opacity(0.3), radius: 24, y: 12)
            }
            .padding(.top, 52)
            .frame(width: width, height: height, alignment: .top)
        }
    }

    /// A portrait leaves room under it for the name and buttons, whatever the hero's height: they
    /// take about 230 points from the hero's foot, and the portrait starts 52 from its top.
    private var portraitSide: CGFloat { max(min(height - 290, 240), 80) }

    /// Darkens the picture's foot into the page colour, so the picture ends in the page instead of
    /// at an edge, and white text over it stays readable over any picture.
    @SwiftUI.ViewBuilder
    private var scrim: some SwiftUI.View {
        if reduceTransparency {
            SwiftUI.VStack(spacing: 0) {
                SwiftUI.Spacer()
                tint.frame(height: min(height * 0.5, 240))
            }
        } else {
            let strong = contrast == .increased
            SwiftUI.LinearGradient(
                stops: [
                    .init(color: .black.opacity(strong ? 0.35 : 0.2), location: 0),
                    .init(color: .clear, location: 0.22),
                    .init(color: tint.opacity(strong ? 0.55 : 0.25), location: 0.5),
                    .init(color: tint.opacity(strong ? 0.95 : 0.85), location: 0.78),
                    .init(color: tint, location: 1),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
        }
    }
}

/// Samples the colour an artist page takes from its banner, or its portrait when it has no
/// banner, once the picture is in the artwork cache.
struct NativeMacArtistTintReader: SwiftUI.View {
    @SwiftUI.ObservedObject var cache: ArtworkCache
    let url: String?
    let onTint: (SwiftUI.Color) -> Void

    var body: some SwiftUI.View {
        let file = cache.localFile(for: url)
        SwiftUI.Color.clear
            .onChange(of: url, initial: true) { _, value in cache.prioritize(value) }
            .task(id: file) {
                guard let file else { return }
                let rgb = await Task.detached(priority: .userInitiated) {
                    Self.tint(at: file)
                }.value
                guard !Task.isCancelled, let rgb else { return }
                onTint(SwiftUI.Color(
                    .sRGB,
                    red: Double(rgb[0]) / 255,
                    green: Double(rgb[1]) / 255,
                    blue: Double(rgb[2]) / 255
                ))
            }
    }

    /// Draws the picture into a small bitmap, keeping its proportions, for `ArtistHero.tint`.
    nonisolated static func tint(at file: URL) -> [UInt8]? {
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let width = 48
        let height = max(1, Int((Double(width) * Double(image.height) / Double(max(image.width, 1))).rounded()))
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.interpolationQuality = .medium
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        // A bitmap context's first row is the picture's top, which is the order the rule expects.
        return ArtistHero.tint(rgba: pixels, width: width, height: height)
    }
}
#endif
