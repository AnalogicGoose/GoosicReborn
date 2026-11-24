#if os(macOS)
import ImageIO
import SwiftUI

/// Hands its content the playing cover's shape, read from the largest copy of the picture that
/// is on disk: the full-size render when YouTube has one, the list thumbnail meanwhile. The player
/// bar and the full player read it the same way, so both show the cover at one shape.
struct NativeMacCoverAspectReader<Content: View>: View {
    let thumbnail: String?
    @ViewBuilder let content: (Double) -> Content
    @Environment(\.goosicArtworkModel) private var model

    var body: some View {
        if let model {
            NativeMacObservedCoverAspect(cache: model.artwork, thumbnail: thumbnail, content: content)
        } else {
            content(1)
        }
    }
}

/// Observes the cache directly, so the shape updates when the full-size picture arrives.
private struct NativeMacObservedCoverAspect<Content: View>: View {
    @ObservedObject var cache: ArtworkCache
    let thumbnail: String?
    @ViewBuilder let content: (Double) -> Content

    var body: some View {
        let larger = NowPlayingMesh.highResolutionVariant(of: thumbnail ?? "")
        let file = cache.localFile(for: larger) ?? cache.localFile(for: thumbnail)
        content(NativeMacCoverAspect.aspect(of: file))
    }
}

@MainActor
enum NativeMacCoverAspect {
    /// A cached file never changes, so its shape is read from its header once.
    private static var known: [URL: Double] = [:]

    static func aspect(of file: URL?) -> Double {
        guard let file else { return 1 }
        if let aspect = known[file] { return aspect }
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
              let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue
        else { return 1 }
        // EXIF orientations 5 to 8 turn the picture a quarter, which swaps its sides.
        let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1
        let aspect = orientation >= 5
            ? CoverShape.aspect(pixelWidth: height, pixelHeight: width)
            : CoverShape.aspect(pixelWidth: width, pixelHeight: height)
        if known.count > 256 { known.removeAll() }
        known[file] = aspect
        return aspect
    }
}
#endif
