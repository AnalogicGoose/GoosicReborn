import Foundation
import GoosicShellSupportFFI

/// The two decisions an artist page's hero makes the same way on every platform: how tall it is
/// for a given width, and which colour the page takes from the banner.
///
/// Both are answered by `goosic-shell-support`'s `hero` module, which Windows asks too. `docs/UI_DESIGN.md` describes the hero they serve.
enum ArtistHero {
    /// A banner's proportions, width over height: YouTube Music serves artist banners at 2880 by 1200.
    static let bannerAspect = 2.4
    /// The shortest hero, so a narrow window still shows the name and its buttons over the picture.
    static let minHeight = 360.0
    /// The tallest hero, so a wide window does not turn the whole first screen into picture.
    static let maxHeight = 620.0
    /// How much of the banner, from its foot, the page colour is sampled from.
    static let tintBand = 0.25
    /// The brightest relative luminance the page colour may have, so white text over it keeps at
    /// least 4.5:1 contrast: (1 + 0.05) / (0.18 + 0.05) ≈ 4.57.
    static let maxTintLuminance = 0.18

    /// The hero's height, in the same units as `width`.
    static func height(forWidth width: Double) -> Double {
        goosic_hero_height(width)
    }

    /// The colour the page takes from a banner, as 8-bit sRGB.
    ///
    /// `rgba` is a small bitmap of the banner, four bytes a pixel, rows from the top. The bottom
    /// band is averaged in linear light and then darkened, never lightened, until white text over
    /// it is readable. `nil` for a bitmap too small to hold the pixels it claims.
    static func tint(rgba: [UInt8], width: Int, height: Int) -> [UInt8]? {
        guard width > 0, height > 0 else { return nil }
        var rgb = [UInt8](repeating: 0, count: 3)
        let found = rgba.withUnsafeBufferPointer { pixels in
            rgb.withUnsafeMutableBufferPointer { out in
                goosic_hero_tint(pixels.baseAddress, pixels.count, width, height, out.baseAddress)
            }
        }
        return found ? rgb : nil
    }
}
