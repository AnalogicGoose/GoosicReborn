//! Where catalog artwork may be fetched from, what it is cached as, and the shape the playing
//! cover is drawn at.
//!
//! Fetching and caching are platform work — each shell has its own HTTP stack and its own cache
//! directory. The two decisions that are not are here: which hosts a catalog response may point
//! the shell at, and how a remote URL becomes a file name that can never escape its directory.
//! Artwork is public CDN content, so whatever fetches it must be anonymous: no cookies, no
//! account headers, exactly like a catalog read.

use url::Url;

/// Hosts YouTube Music serves artwork from. Anything else is refused rather than fetched, so a
/// catalog response cannot point the shell at an arbitrary server. `gstatic.com` is Google's
/// static host, where YouTube Music keeps the art of its own playlists such as Liked Music;
/// without it those cards were left blank.
pub const ALLOWED_HOST_SUFFIXES: [&str; 5] = [
    "googleusercontent.com",
    "ggpht.com",
    "ytimg.com",
    "youtube.com",
    "gstatic.com",
];

/// Artwork is small. Anything larger is not a thumbnail and is discarded.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// A bound on how many downloads are in flight, so opening a dense screen cannot start hundreds
/// of connections at once.
pub const MAX_CONCURRENT_FETCHES: usize = 6;

/// Whether a shell is willing to fetch `url` at all. The suffix is matched on a label boundary,
/// so `evilgoogleusercontent.com` is not `googleusercontent.com`.
pub fn is_allowed(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ALLOWED_HOST_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// A stable, collision-resistant file name for a remote URL.
///
/// Two independent FNV-1a passes give 128 bits — far more than enough to keep two thumbnails from
/// sharing a file — without a hashing dependency. The second pass runs over the reversed bytes, so
/// a transposition changes the key. The result is 32 hex digits and nothing else, so a path built
/// from it cannot climb out of the cache directory whatever the URL contained.
pub fn cache_key(remote: &str) -> String {
    fn fnv1a(bytes: impl Iterator<Item = u8>, seed: u64) -> u64 {
        bytes.fold(seed, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        })
    }
    let bytes = remote.as_bytes();
    let low = fnv1a(bytes.iter().copied(), 0xcbf2_9ce4_8422_2325);
    let high = fnv1a(bytes.iter().rev().copied(), 0x9dc5_bb15_8f2c_1e37);
    format!("{low:016x}{high:016x}")
}

/// The narrowest a now-playing cover is drawn: a portrait picture at 9:16.
pub const MIN_ASPECT: f64 = 9.0 / 16.0;

/// The widest a now-playing cover is drawn: a music video's 16:9 frame.
pub const MAX_ASPECT: f64 = 16.0 / 9.0;

/// The width-to-height ratio the playing cover is drawn at, taken from the picture itself.
///
/// A song's art is square and a music video's is wide, and cropping both to one fixed square cut
/// the sides off every video. The player instead takes the shape of the picture it has. The ratio
/// is held between 9:16 and 16:9 so an odd banner cannot turn the cover into a sliver, and a
/// picture with no size yet is square, the shape most covers are.
pub fn cover_aspect(pixel_width: f64, pixel_height: f64) -> f64 {
    if !(pixel_width.is_finite() && pixel_height.is_finite())
        || pixel_width <= 0.0
        || pixel_height <= 0.0
    {
        return 1.0;
    }
    (pixel_width / pixel_height).clamp(MIN_ASPECT, MAX_ASPECT)
}

/// The size a cover of `aspect` takes inside a `max_width` by `max_height` slot: as large as fits
/// without cropping, touching the slot on its longer side.
pub fn fit_cover(aspect: f64, max_width: f64, max_height: f64) -> (f64, f64) {
    let aspect = cover_aspect(aspect, 1.0);
    let (max_width, max_height) = (max_width.max(0.0), max_height.max(0.0));
    if max_height == 0.0 || aspect >= max_width / max_height {
        (max_width, max_width / aspect)
    } else {
        (max_height * aspect, max_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_aspect_follows_the_picture_within_bounds() {
        assert_eq!(cover_aspect(544.0, 544.0), 1.0);
        assert!((cover_aspect(1280.0, 720.0) - 16.0 / 9.0).abs() < 1e-9);
        assert!((cover_aspect(480.0, 360.0) - 4.0 / 3.0).abs() < 1e-9);
        assert_eq!(cover_aspect(3000.0, 300.0), MAX_ASPECT);
        assert_eq!(cover_aspect(100.0, 1000.0), MIN_ASPECT);
        assert_eq!(cover_aspect(0.0, 720.0), 1.0);
        assert_eq!(cover_aspect(f64::NAN, 720.0), 1.0);
    }

    #[test]
    fn a_cover_fits_its_slot_on_the_longer_side() {
        assert_eq!(fit_cover(1.0, 520.0, 520.0), (520.0, 520.0));
        let (w, h) = fit_cover(16.0 / 9.0, 520.0, 520.0);
        assert_eq!(w, 520.0);
        assert!((h - 292.5).abs() < 1e-9);
        let (w, h) = fit_cover(9.0 / 16.0, 520.0, 520.0);
        assert!((w - 292.5).abs() < 1e-9);
        assert_eq!(h, 520.0);
        assert_eq!(fit_cover(1.0, 0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn artwork_hosts_youtube_music_actually_serves_are_allowed() {
        assert!(is_allowed("https://yt3.googleusercontent.com/abc"));
        assert!(is_allowed("https://lh3.googleusercontent.com/abc"));
        assert!(is_allowed("https://i.ytimg.com/vi/abc/hq.jpg"));
        assert!(is_allowed("https://yt3.ggpht.com/abc"));
        assert!(
            is_allowed("https://googleusercontent.com/art.jpg"),
            "the bare domain itself"
        );
        assert!(is_allowed(
            "https://www.gstatic.com/youtube/media/ytm/images/pbg/liked-music-@576.png"
        ));
    }

    #[test]
    fn plaintext_is_refused_so_artwork_is_never_fetched_in_the_clear() {
        assert!(!is_allowed("http://yt3.googleusercontent.com/abc"));
    }

    #[test]
    fn an_unrelated_host_is_refused_rather_than_fetched() {
        assert!(!is_allowed("https://example.test/art.jpg"));
        assert!(!is_allowed("https://localhost/art.jpg"));
        assert!(!is_allowed("https://169.254.169.254/latest/meta-data"));
        assert!(!is_allowed("file:///etc/passwd"));
        assert!(!is_allowed(""));
    }

    #[test]
    fn suffix_lookalikes_do_not_slip_through() {
        assert!(!is_allowed("https://evilgoogleusercontent.com/art.jpg"));
        assert!(!is_allowed("https://notytimg.com/art.jpg"));
        assert!(!is_allowed(
            "https://googleusercontent.com.evil.test/art.jpg"
        ));
    }

    #[test]
    fn the_key_is_stable_and_distinct() {
        let url = "https://yt3.googleusercontent.com/abc";
        assert_eq!(cache_key(url), cache_key(url));
        assert_ne!(
            cache_key("https://yt3.googleusercontent.com/a"),
            cache_key("https://yt3.googleusercontent.com/b")
        );
        // Two passes over the bytes, one reversed, so a transposition changes the key.
        assert_ne!(
            cache_key("https://yt3.googleusercontent.com/ab"),
            cache_key("https://yt3.googleusercontent.com/ba")
        );
    }

    /// The low half is plain FNV-1a-64, so the published test vector pins it: a port that got the
    /// constants or the byte order wrong would still be "stable and distinct" and fail here.
    #[test]
    fn the_low_half_matches_the_published_fnv1a_vector() {
        assert!(cache_key("a").starts_with("af63dc4c8601ec8c"));
        assert!(cache_key("").starts_with("cbf29ce484222325"));
    }

    #[test]
    fn the_key_is_a_fixed_length_filename_safe_string() {
        let key = cache_key("https://yt3.googleusercontent.com/a?x=1&y=/../..");
        assert_eq!(key.len(), 32);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
