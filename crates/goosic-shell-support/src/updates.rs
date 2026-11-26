//! Which release the updater offers, on every platform.
//!
//! Goosic publishes two kinds of release on GitHub. A stable release is an ordinary one, and an
//! experimental release is the same kind of build marked as a pre-release. Version numbers stay
//! plain `x.y.z` and keep rising across both, so the macOS build number derived from them, the
//! Windows Setup file name and every installed copy's comparison keep working unchanged: a
//! stable listener simply never sees the numbers spent on experiments. Switching back from
//! experimental to stable does not downgrade anything; the copy stays where it is until a stable
//! release passes it.
//!
//! The Windows updater reads the release list and asks [`choose_release`]; the macOS updater
//! reads the Sparkle feed, where an experimental item carries a `sparkle:channel`, and asks
//! [`sparkle_channels`] which channels it may take. Neither shell decides this on its own.

use serde::{Deserialize, Serialize};

/// The channel a Sparkle feed item carries when it is an experimental release.
pub const EXPERIMENTAL_SPARKLE_CHANNEL: &str = "experimental";

/// Which releases a listener wants to be offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Experimental,
}

impl UpdateChannel {
    /// Reads a stored preference; anything unknown is stable.
    pub fn parse(value: &str) -> UpdateChannel {
        match value {
            "experimental" => UpdateChannel::Experimental,
            _ => UpdateChannel::Stable,
        }
    }
}

/// What the updater knows about one published release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub tag: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

/// Reads `v0.4.6` or `0.4.6`. A suffixed tag such as `v0.4.3-linux-alpha.1` names a platform
/// test build, not a release either updater installs, so it reads as nothing.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let core = tag.trim();
    let core = core
        .strip_prefix('v')
        .or_else(|| core.strip_prefix('V'))
        .unwrap_or(core);
    let core = core.split('+').next().unwrap_or("");
    let mut parts = core.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// The release to offer, as an index into `releases`, or `None` when nothing is newer than
/// `current`. A development build (`current` unknown) is never offered an update.
pub fn choose_release(
    current: Option<&str>,
    channel: UpdateChannel,
    releases: &[ReleaseInfo],
) -> Option<usize> {
    let current = parse_version(current?)?;
    releases
        .iter()
        .enumerate()
        .filter(|(_, release)| !release.draft)
        .filter(|(_, release)| channel == UpdateChannel::Experimental || !release.prerelease)
        .filter_map(|(index, release)| Some((parse_version(&release.tag)?, index)))
        .filter(|(version, _)| *version > current)
        .max_by_key(|(version, _)| *version)
        .map(|(_, index)| index)
}

/// The Sparkle channels, beyond the default one, a copy on `channel` may install from.
pub fn sparkle_channels(channel: UpdateChannel) -> &'static [&'static str] {
    match channel {
        UpdateChannel::Stable => &[],
        UpdateChannel::Experimental => &[EXPERIMENTAL_SPARKLE_CHANNEL],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool) -> ReleaseInfo {
        ReleaseInfo {
            tag: tag.into(),
            draft: false,
            prerelease,
        }
    }

    #[test]
    fn versions_are_plain_triples() {
        assert_eq!(parse_version("v0.4.6"), Some((0, 4, 6)));
        assert_eq!(parse_version("0.10.2+abc"), Some((0, 10, 2)));
        assert_eq!(parse_version("v0.4.3-linux-alpha.1"), None);
        assert_eq!(parse_version("v0.4"), None);
        assert_eq!(parse_version("v0.4.6.1"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn stable_skips_pre_releases_and_experimental_takes_the_newest() {
        let releases = [
            release("v0.4.3-linux-alpha.1", true),
            release("v0.4.6", true),
            release("v0.4.5", false),
        ];
        assert_eq!(
            choose_release(Some("0.4.4"), UpdateChannel::Stable, &releases),
            Some(2)
        );
        assert_eq!(
            choose_release(Some("0.4.4"), UpdateChannel::Experimental, &releases),
            Some(1)
        );
        assert_eq!(
            choose_release(Some("0.4.5"), UpdateChannel::Stable, &releases),
            None
        );
    }

    #[test]
    fn nothing_older_drafted_or_unknown_is_offered() {
        let mut draft = release("v9.0.0", false);
        draft.draft = true;
        let releases = [draft, release("v0.4.5", false)];
        assert_eq!(
            choose_release(Some("0.4.6"), UpdateChannel::Experimental, &releases),
            None,
            "leaving experimental never downgrades"
        );
        assert_eq!(choose_release(None, UpdateChannel::Stable, &releases), None);
    }

    #[test]
    fn only_experimental_reads_the_experimental_feed_channel() {
        assert!(sparkle_channels(UpdateChannel::Stable).is_empty());
        assert_eq!(
            sparkle_channels(UpdateChannel::parse("experimental")),
            ["experimental"]
        );
        assert_eq!(UpdateChannel::parse("nonsense"), UpdateChannel::Stable);
    }
}
