import Foundation

/// The shape the playing cover is drawn at, taken from the picture itself.
///
/// A song's art is square and a music video's is wide, and cropping both to one fixed square cut
/// the sides off every video. The player bar and the full player instead take the shape of the
/// picture they have. The ratio is held between 9:16 and 16:9 so an odd banner cannot turn the
/// cover into a sliver, and a picture with no size yet is square, the shape most covers are.
///
/// This is the same rule as `cover_aspect` and `fit_cover` in `goosic-shell-support` and the
/// Windows `CoverShape`, held to the same test cases.
enum CoverShape {
    static let minAspect = 9.0 / 16.0
    static let maxAspect = 16.0 / 9.0

    /// The width-to-height ratio for a picture of the given pixel size.
    static func aspect(pixelWidth: Double, pixelHeight: Double) -> Double {
        guard pixelWidth.isFinite, pixelHeight.isFinite, pixelWidth > 0, pixelHeight > 0 else { return 1 }
        return min(max(pixelWidth / pixelHeight, minAspect), maxAspect)
    }

    /// The size a cover of `aspect` takes inside a slot: as large as fits without cropping,
    /// touching the slot on its longer side.
    static func fit(aspect: Double, maxWidth: Double, maxHeight: Double) -> (width: Double, height: Double) {
        let aspect = self.aspect(pixelWidth: aspect, pixelHeight: 1)
        let width = max(maxWidth, 0)
        let height = max(maxHeight, 0)
        if height == 0 || aspect >= width / height {
            return (width, width / aspect)
        }
        return (height * aspect, height)
    }
}
