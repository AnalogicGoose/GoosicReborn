import Foundation
import GoosicShellSupportFFI

/// The shape the playing cover is drawn at, taken from the picture itself.
///
/// A song's art is square and a music video's is wide, and cropping both to one fixed square cut
/// the sides off every video. The player bar and the full player instead take the shape of the
/// picture they have. The ratio is held between 9:16 and 16:9 so an odd banner cannot turn the
/// cover into a sliver, and a picture with no size yet is square, the shape most covers are.
///
/// The rule is `cover_aspect` and `fit_cover` in `goosic-shell-support`, which Windows asks too.
enum CoverShape {
    static let minAspect = 9.0 / 16.0
    static let maxAspect = 16.0 / 9.0

    /// The width-to-height ratio for a picture of the given pixel size.
    static func aspect(pixelWidth: Double, pixelHeight: Double) -> Double {
        goosic_cover_aspect(pixelWidth, pixelHeight)
    }

    /// The size a cover of `aspect` takes inside a slot: as large as fits without cropping,
    /// touching the slot on its longer side.
    static func fit(aspect: Double, maxWidth: Double, maxHeight: Double) -> (width: Double, height: Double) {
        var width = 0.0
        var height = 0.0
        goosic_cover_fit(aspect, maxWidth, maxHeight, &width, &height)
        return (width, height)
    }
}
