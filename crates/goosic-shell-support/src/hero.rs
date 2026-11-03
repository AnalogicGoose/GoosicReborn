//! The two decisions an artist page's hero makes the same way on every platform.
//!
//! How tall the hero is for a given width, and which colour the page takes from the banner.
//! Drawing the hero is native work; these are numbers, and three shells computing them three ways
//! would give one artist three different pages. `docs/UI_DESIGN.md` describes the hero they
//! serve, and `Core/ArtistHero.swift` is the macOS copy, held to the same cases.

/// A banner's proportions, width over height: YouTube Music serves artist banners at 2880 by 1200.
pub const BANNER_ASPECT: f64 = 2.4;
/// The shortest hero, so a narrow window still shows the name and its buttons over the picture.
pub const MIN_HEIGHT: f64 = 360.0;
/// The tallest hero, so a wide window does not turn the whole first screen into picture.
pub const MAX_HEIGHT: f64 = 620.0;

/// How much of the banner, from its foot, the page colour is sampled from. The name and the
/// buttons sit there, and that band is what the page continues.
pub const TINT_BAND: f64 = 0.25;
/// The brightest relative luminance the page colour may have. White text over it then keeps at
/// least the 4.5:1 contrast that WCAG asks of body text: (1 + 0.05) / (0.18 + 0.05) ≈ 4.57.
pub const MAX_TINT_LUMINANCE: f64 = 0.18;

/// The hero's height, in the same units as `width`.
pub fn height(width: f64) -> f64 {
    (width / BANNER_ASPECT).clamp(MIN_HEIGHT, MAX_HEIGHT)
}

/// The colour the page takes from a banner, as 8-bit sRGB.
///
/// `rgba` is a small bitmap of the banner, four bytes a pixel, rows from the top; a shell scales
/// the picture down before sampling, since the result is one colour. The bottom band is averaged
/// and then darkened, never lightened, until white text over it is readable. Returns `None` for a
/// bitmap too small to hold the pixels it claims.
pub fn tint(rgba: &[u8], width: usize, height: usize) -> Option<[u8; 3]> {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return None;
    }
    let band = ((height as f64 * TINT_BAND).ceil() as usize).clamp(1, height);
    let mut sum = [0.0; 3];
    for row in height - band..height {
        for column in 0..width {
            let pixel = &rgba[(row * width + column) * 4..][..3];
            for (total, channel) in sum.iter_mut().zip(pixel) {
                *total += linear(*channel);
            }
        }
    }
    let count = (band * width) as f64;
    let mut average = sum.map(|total| total / count);
    let luminance = 0.2126 * average[0] + 0.7152 * average[1] + 0.0722 * average[2];
    if luminance > MAX_TINT_LUMINANCE {
        let scale = MAX_TINT_LUMINANCE / luminance;
        average = average.map(|channel| channel * scale);
    }
    Some(average.map(encode))
}

/// sRGB to linear light, so averaging and darkening happen in the space where light adds up.
fn linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(value: f64) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let srgb = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: usize, height: usize, pixel: impl Fn(usize) -> [u8; 3]) -> Vec<u8> {
        (0..width * height)
            .flat_map(|index| {
                let [r, g, b] = pixel(index / width);
                [r, g, b, 255]
            })
            .collect()
    }

    #[test]
    fn the_hero_follows_the_banner_between_a_floor_and_a_ceiling() {
        assert_eq!(height(1200.0), 500.0);
        assert_eq!(height(600.0), MIN_HEIGHT);
        assert_eq!(height(2400.0), MAX_HEIGHT);
    }

    #[test]
    fn a_dark_banner_keeps_its_own_colour() {
        let navy = bitmap(4, 4, |_| [20, 30, 60]);
        assert_eq!(tint(&navy, 4, 4), Some([20, 30, 60]));
    }

    #[test]
    fn a_bright_banner_is_darkened_until_white_text_reads_over_it() {
        let white = bitmap(4, 4, |_| [255, 255, 255]);
        // Linear 0.18 is sRGB 118: a mid grey, never lighter.
        assert_eq!(tint(&white, 4, 4), Some([118, 118, 118]));
    }

    #[test]
    fn only_the_foot_of_the_banner_is_sampled() {
        // White above, black in the bottom quarter: the page continues the black.
        let banner = bitmap(
            4,
            8,
            |row| if row < 6 { [255, 255, 255] } else { [0, 0, 0] },
        );
        assert_eq!(tint(&banner, 4, 8), Some([0, 0, 0]));
    }

    #[test]
    fn a_bitmap_shorter_than_it_claims_has_no_tint() {
        assert_eq!(tint(&[0; 12], 2, 2), None);
        assert_eq!(tint(&[], 0, 0), None);
    }
}
