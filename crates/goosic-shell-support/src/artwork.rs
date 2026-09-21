//! Where catalog artwork may be fetched from, and what it is cached as.
//!
//! Fetching and caching are platform work — each shell has its own HTTP stack and its own cache
//! directory. The two decisions that are not are here: which hosts a catalog response may point
//! the shell at, and how a remote URL becomes a file name that can never escape its directory.
//! Artwork is public CDN content, so whatever fetches it must be anonymous: no cookies, no
//! account headers, exactly like a catalog read.

use url::Url;

/// Hosts YouTube Music serves artwork from. Anything else is refused rather than fetched, so a
/// catalog response cannot point the shell at an arbitrary server.
pub const ALLOWED_HOST_SUFFIXES: [&str; 4] =
    ["googleusercontent.com", "ggpht.com", "ytimg.com", "youtube.com"];

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
        bytes.fold(seed, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3))
    }
    let bytes = remote.as_bytes();
    let low = fnv1a(bytes.iter().copied(), 0xcbf2_9ce4_8422_2325);
    let high = fnv1a(bytes.iter().rev().copied(), 0x9dc5_bb15_8f2c_1e37);
    format!("{low:016x}{high:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artwork_hosts_youtube_music_actually_serves_are_allowed() {
        assert!(is_allowed("https://yt3.googleusercontent.com/abc"));
        assert!(is_allowed("https://lh3.googleusercontent.com/abc"));
        assert!(is_allowed("https://i.ytimg.com/vi/abc/hq.jpg"));
        assert!(is_allowed("https://yt3.ggpht.com/abc"));
        assert!(is_allowed("https://googleusercontent.com/art.jpg"), "the bare domain itself");
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
        assert!(!is_allowed("https://googleusercontent.com.evil.test/art.jpg"));
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
