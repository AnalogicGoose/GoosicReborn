#if os(macOS)
import CoreImage
import Foundation

/// Turns a cached artwork file into the blurred backdrop the native macOS shell draws behind
/// its content.
///
/// The blur is baked into a bitmap once per track instead of being applied live, so scrolling
/// and resizing the window cost nothing beyond drawing one image.
@MainActor
enum ArtworkBackdropRenderer {
    /// How strongly the window colour is laid over the blurred art. Enough that body text stays
    /// readable in both light and dark appearances, little enough that the colour still shows.
    static let tintOpacity: Double = 0.55

    private static let context = CIContext(options: [.cacheIntermediates: false])
    /// Only the playing track is ever drawn, so remembering the last one is the whole cache.
    private static var last: (file: URL, image: CGImage)?

    static func backdrop(for file: URL) -> CGImage? {
        if let last, last.file == file { return last.image }
        guard let source = CIImage(contentsOf: file) else { return nil }
        let extent = source.extent
        guard extent.width > 0, extent.height > 0 else { return nil }
        // Thumbnails arrive at anything from 60 to over 500 pixels, so the radius follows the
        // image rather than being fixed; a fixed one would be a smear on one and a sharp picture
        // on the other. Clamping first keeps the edges from fading to transparent.
        let sigma = max(extent.width, extent.height) * 0.08
        let blurred = source.clampedToExtent()
            .applyingGaussianBlur(sigma: sigma)
            .cropped(to: extent)
        guard let image = context.createCGImage(blurred, from: extent) else { return nil }
        last = (file, image)
        return image
    }
}
#endif
