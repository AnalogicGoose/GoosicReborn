//! Which release the updater offers, on every platform.
//!
//! Goosic publishes two kinds of release on GitHub. A stable release is an ordinary one, and an
//! experimental release is the same kind of build marked as a pre-release. Versions follow Apple's
//! scheme: a plain `x.y.z` is a release, and `x.y.z-beta.N` is the Nth beta of that release,
//! shown as "0.4.7 beta 2". A beta comes before the release it leads to, so 0.4.7 beta 2 is
//! older than 0.4.7 and newer than 0.4.6. Every build also carries a build number that only ever
//! rises, `x.y.z` × 100 plus the beta number, or 99 for the release itself: 0.4.7 beta 2 is
//! 40702 and 0.4.7 is 40799. The macOS bundle and the Sparkle feed compare that number, and the
//! Windows updater compares the same order through [`ReleaseVersion`].
//!
//! 0.4.7 was published as an experimental release before beta numbers existed, under the plain
//! tag `v0.4.7`. A plain tag marked as a pre-release therefore reads as beta 1 of its version, so
//! a copy running 0.4.7 beta 2 is never offered it as if it were the finished 0.4.7.
//!
//! Switching back from experimental to stable does not downgrade anything; the copy stays where
//! it is until a stable release passes it.
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

/// The beta number a release that is not a beta stands for: above every beta of its version.
pub const RELEASE_BETA_RANK: u32 = 99;

/// One version in Apple's scheme: `x.y.z`, or its Nth beta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReleaseVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `None` for the release itself, `Some(n)` for beta n (1 to 98).
    pub beta: Option<u32>,
}

impl ReleaseVersion {
    /// The order key: the version, then the beta, with the release itself after every beta.
    fn key(&self) -> (u64, u64, u64, u32) {
        (
            self.major,
            self.minor,
            self.patch,
            self.beta.unwrap_or(RELEASE_BETA_RANK),
        )
    }

    /// The build number that rises with every build: the version × 100 plus the beta number, or
    /// [`RELEASE_BETA_RANK`] for the release. `None` when minor or patch reach 100, where the
    /// number could go backwards.
    pub fn build_number(&self) -> Option<u64> {
        (self.minor < 100 && self.patch < 100).then(|| {
            (self.major * 10_000 + self.minor * 100 + self.patch) * 100
                + u64::from(self.beta.unwrap_or(RELEASE_BETA_RANK))
        })
    }

    /// What a person reads: "0.4.7" or "0.4.7 beta 2".
    pub fn label(&self) -> String {
        match self.beta {
            Some(beta) => format!("{}.{}.{} beta {beta}", self.major, self.minor, self.patch),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }

    /// The version as a tag or file name spells it: "0.4.7" or "0.4.7-beta.2".
    pub fn spelled(&self) -> String {
        match self.beta {
            Some(beta) => format!("{}.{}.{}-beta.{beta}", self.major, self.minor, self.patch),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

impl PartialOrd for ReleaseVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReleaseVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

/// Reads `v0.4.6`, `0.4.6` or `v0.4.7-beta.2`. Any other suffix, such as
/// `v0.4.3-linux-alpha.1`, names a platform test build, not a release either updater installs,
/// so it reads as nothing, as does a beta numbered 0 or above 98.
pub fn parse_release_version(tag: &str) -> Option<ReleaseVersion> {
    let core = tag.trim();
    let core = core
        .strip_prefix('v')
        .or_else(|| core.strip_prefix('V'))
        .unwrap_or(core);
    let core = core.split('+').next().unwrap_or("");
    let (core, beta) = match core.split_once('-') {
        None => (core, None),
        Some((core, suffix)) => {
            let number = suffix.strip_prefix("beta.")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let number: u32 = number.parse().ok()?;
            if number == 0 || number >= RELEASE_BETA_RANK {
                return None;
            }
            (core, Some(number))
        }
    };
    let mut parts = core.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    let (major, minor, patch) = (next()?, next()?, next()?);
    parts.next().is_none().then_some(ReleaseVersion {
        major,
        minor,
        patch,
        beta,
    })
}

/// Reads a version as a plain triple, ignoring any beta. Kept for callers that only need the
/// release a build belongs to.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    parse_release_version(tag).map(|version| (version.major, version.minor, version.patch))
}

/// A published release's place in the order: a plain tag marked as a pre-release is beta 1 of
/// its version, which is how 0.4.7 was published before beta numbers existed.
fn published_version(release: &ReleaseInfo) -> Option<ReleaseVersion> {
    let mut version = parse_release_version(&release.tag)?;
    if release.prerelease && version.beta.is_none() {
        version.beta = Some(1);
    }
    Some(version)
}

/// The release to offer, as an index into `releases`, or `None` when nothing is newer than
/// `current`. A development build (`current` unknown) is never offered an update.
pub fn choose_release(
    current: Option<&str>,
    channel: UpdateChannel,
    releases: &[ReleaseInfo],
) -> Option<usize> {
    let current = parse_release_version(current?)?;
    releases
        .iter()
        .enumerate()
        .filter(|(_, release)| !release.draft)
        .filter(|(_, release)| channel == UpdateChannel::Experimental || !release.prerelease)
        .filter_map(|(index, release)| Some((published_version(release)?, index)))
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
        assert_eq!(parse_version("v0.4.7-beta.2"), Some((0, 4, 7)));
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

    fn beta(major: u64, minor: u64, patch: u64, beta: Option<u32>) -> ReleaseVersion {
        ReleaseVersion {
            major,
            minor,
            patch,
            beta,
        }
    }

    #[test]
    fn betas_read_in_apples_scheme() {
        assert_eq!(
            parse_release_version("v0.4.7-beta.2"),
            Some(beta(0, 4, 7, Some(2)))
        );
        assert_eq!(parse_release_version("0.4.7"), Some(beta(0, 4, 7, None)));
        assert_eq!(parse_release_version("v0.4.7-beta.0"), None);
        assert_eq!(parse_release_version("v0.4.7-beta.99"), None);
        assert_eq!(parse_release_version("v0.4.7-beta."), None);
        assert_eq!(parse_release_version("v0.4.7-rc.1"), None);
        assert_eq!(parse_release_version("v0.4.3-linux-alpha.1"), None);
        assert_eq!(beta(0, 4, 7, Some(2)).label(), "0.4.7 beta 2");
        assert_eq!(beta(0, 4, 7, None).label(), "0.4.7");
        assert_eq!(beta(0, 4, 7, Some(2)).spelled(), "0.4.7-beta.2");
    }

    #[test]
    fn a_beta_comes_before_its_release_and_after_the_last() {
        assert!(beta(0, 4, 6, None) < beta(0, 4, 7, Some(1)));
        assert!(beta(0, 4, 7, Some(1)) < beta(0, 4, 7, Some(2)));
        assert!(beta(0, 4, 7, Some(2)) < beta(0, 4, 7, None));
        assert!(beta(0, 4, 7, None) < beta(0, 4, 8, Some(1)));
    }

    #[test]
    fn build_numbers_only_rise() {
        assert_eq!(beta(0, 4, 7, Some(2)).build_number(), Some(40702));
        assert_eq!(beta(0, 4, 7, None).build_number(), Some(40799));
        assert_eq!(beta(1, 0, 0, Some(1)).build_number(), Some(1_000_001));
        assert_eq!(beta(0, 100, 0, None).build_number(), None);
        // Above every number the feed has carried, the last being 407 for 0.4.7.
        assert!(beta(0, 4, 7, Some(2)).build_number().unwrap() > 407);
        let order = [
            beta(0, 4, 7, Some(2)),
            beta(0, 4, 7, Some(3)),
            beta(0, 4, 7, None),
            beta(0, 4, 8, Some(1)),
            beta(0, 5, 0, None),
        ];
        for pair in order.windows(2) {
            assert!(pair[0].build_number() < pair[1].build_number());
        }
    }

    #[test]
    fn the_plain_0_4_7_pre_release_is_beta_one() {
        let releases = [release("v0.4.7", true), release("v0.4.7-beta.2", true)];
        assert_eq!(
            choose_release(Some("0.4.7-beta.2"), UpdateChannel::Experimental, &releases),
            None,
            "beta 2 is never offered the older beta under the plain tag"
        );
        assert_eq!(
            choose_release(Some("0.4.6"), UpdateChannel::Experimental, &releases),
            Some(1)
        );
        assert_eq!(
            choose_release(Some("0.4.6"), UpdateChannel::Stable, &releases),
            None
        );
        let finished = [release("v0.4.7-beta.2", true), release("v0.4.7", false)];
        assert_eq!(
            choose_release(Some("0.4.7-beta.2"), UpdateChannel::Experimental, &finished),
            Some(1),
            "a stable 0.4.7 is newer than its betas"
        );
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
