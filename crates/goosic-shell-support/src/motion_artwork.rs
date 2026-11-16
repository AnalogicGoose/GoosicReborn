//! Narrow transport limits for the opt-in motion artwork experiment.
//! Separate from thumbnail policy: no credentials, arbitrary hosts or audio playback.
use url::Url;

pub const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DURATION_SECONDS: f64 = 60.0;

pub fn is_allowed(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some("itunes.apple.com" | "music.apple.com" | "mvod.itunes.apple.com")
        )
}

pub fn duration_matches(requested: f64, candidate: f64) -> bool {
    requested.is_finite()
        && candidate.is_finite()
        && requested > 0.0
        && candidate > 0.0
        && (requested - candidate).abs() <= 5.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_credential_free_apple_hosts_are_allowed() {
        for raw in [
            "https://itunes.apple.com/search",
            "https://music.apple.com/us/album/123",
            "https://mvod.itunes.apple.com/cover.mp4",
        ] {
            assert!(is_allowed(raw));
        }
        for raw in [
            "http://music.apple.com/x",
            "https://music.apple.com.evil/x",
            "https://user@music.apple.com/x",
            "https://music.apple.com:444/x",
            "file:///tmp/x",
        ] {
            assert!(!is_allowed(raw));
        }
    }
    #[test]
    fn a_duration_is_required_and_versions_must_be_close() {
        assert!(duration_matches(200.0, 205.0));
        for seconds in [0.0, 206.0, f64::NAN, f64::INFINITY] {
            assert!(!duration_matches(200.0, seconds));
        }
    }
}
