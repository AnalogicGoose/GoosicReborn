import Foundation

/// The two decisions an artist page's hero makes the same way on every platform: how tall it is
/// for a given width, and which colour the page takes from the banner.
///
/// This is the macOS copy of `goosic-shell-support`'s `hero` module, and the two are held to the
/// same test cases. `docs/UI_DESIGN.md` describes the hero they serve.
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
        min(max(width / bannerAspect, minHeight), maxHeight)
    }

    /// The colour the page takes from a banner, as 8-bit sRGB.
    ///
    /// `rgba` is a small bitmap of the banner, four bytes a pixel, rows from the top. The bottom
    /// band is averaged in linear light and then darkened, never lightened, until white text over
    /// it is readable. `nil` for a bitmap too small to hold the pixels it claims.
    static func tint(rgba: [UInt8], width: Int, height: Int) -> [UInt8]? {
        guard width > 0, height > 0, rgba.count >= width * height * 4 else { return nil }
        let band = min(max(Int((Double(height) * tintBand).rounded(.up)), 1), height)
        var sum = [0.0, 0.0, 0.0]
        for row in (height - band)..<height {
            for column in 0..<width {
                let offset = (row * width + column) * 4
                for channel in 0..<3 {
                    sum[channel] += linear(rgba[offset + channel])
                }
            }
        }
        let count = Double(band * width)
        var average = sum.map { $0 / count }
        let luminance = 0.2126 * average[0] + 0.7152 * average[1] + 0.0722 * average[2]
        if luminance > maxTintLuminance {
            let scale = maxTintLuminance / luminance
            average = average.map { $0 * scale }
        }
        return average.map(encode)
    }

    private static func linear(_ channel: UInt8) -> Double {
        let value = Double(channel) / 255
        return value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
    }

    private static func encode(_ value: Double) -> UInt8 {
        let value = min(max(value, 0), 1)
        let srgb = value <= 0.0031308 ? value * 12.92 : 1.055 * pow(value, 1 / 2.4) - 0.055
        return UInt8((srgb * 255).rounded())
    }
}
