//! Anonymous GitHub Releases update checker for the Linux shell.
//!
//! Uses `soup3` without a cookie jar or credentials (never touching account profiles or
//! `goosic-service`). Only HTTPS `github.com` release and `.flatpak` download URLs are accepted;
//! opening a download delegates to the desktop's `gtk::UriLauncher` / XDG OpenURI portal.

use gtk::glib;
use serde_json::Value;
use soup::prelude::*;

/// The version of this Linux build, matching the alpha release tag without its leading `v`.
pub const CURRENT_VERSION: &str = "0.4.3-linux-alpha.1";

const RELEASES_URL: &str =
    "https://api.github.com/repos/AnalogicGoose/GoosicReborn/releases?per_page=15";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const TIMEOUT_SECONDS: u32 = 12;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        download_url: String,
        release_url: String,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ParsedVersion {
    major: u32,
    minor: u32,
    patch: u32,
    /// `None` is a stable release, which sorts higher than `Some(n)` (`-linux-alpha.n`).
    is_stable: bool,
    alpha: u32,
}

impl ParsedVersion {
    fn parse(tag: &str) -> Option<Self> {
        let trimmed = tag.trim().strip_prefix('v').unwrap_or(tag.trim());
        if trimmed.contains("-macos-") {
            return None;
        }
        let (base, suffix) = match trimmed.split_once('-') {
            Some((b, s)) => (b, Some(s)),
            None => (trimmed, None),
        };
        let mut parts = base.split('.');
        let major = parts.next()?.parse::<u32>().ok()?;
        let minor = parts.next()?.parse::<u32>().ok()?;
        let patch = parts.next()?.parse::<u32>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let (is_stable, alpha) = match suffix {
            None => (true, 0),
            Some(s) => {
                let num = s.strip_prefix("linux-alpha.")?.parse::<u32>().ok()?;
                (false, num)
            }
        };
        Some(Self {
            major,
            minor,
            patch,
            is_stable,
            alpha,
        })
    }
}

fn is_allowed_https_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && matches!(
                parsed.host_str(),
                Some("github.com" | "objects.githubusercontent.com")
            )
    })
}

/// Evaluates the GitHub Releases JSON array against `current_version`.
pub fn evaluate_releases(body: &[u8], current_version: &str) -> Result<UpdateStatus, String> {
    if body.is_empty() || body.len() > MAX_RESPONSE_BYTES {
        return Err("Update feed response was empty or too large.".to_owned());
    }
    let current = ParsedVersion::parse(current_version)
        .ok_or_else(|| "Current version is not recognizable.".to_owned())?;
    let releases: Vec<Value> = serde_json::from_slice(body)
        .map_err(|_| "Could not parse the GitHub Releases feed.".to_owned())?;

    let mut best: Option<(ParsedVersion, String, String, String)> = None;
    for release in releases {
        if release
            .get("draft")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(tag) = release.get("tag_name").and_then(Value::as_str) else {
            continue;
        };
        let Some(parsed) = ParsedVersion::parse(tag) else {
            continue;
        };
        if parsed <= current {
            continue;
        }
        let Some(html_url) = release
            .get("html_url")
            .and_then(Value::as_str)
            .filter(|url| is_allowed_https_url(url))
        else {
            continue;
        };
        // Prefer a direct `.flatpak` asset on x86_64/current arch if attached; otherwise open the release page.
        let flatpak_url = release
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    let name = asset.get("name").and_then(Value::as_str)?;
                    let url = asset.get("browser_download_url").and_then(Value::as_str)?;
                    (name.ends_with(".flatpak") && is_allowed_https_url(url)).then_some(url)
                })
            });
        // For stable releases without a Linux Flatpak asset yet, do not offer a non-Linux build.
        if parsed.is_stable && flatpak_url.is_none() {
            continue;
        }
        let download_url = flatpak_url.unwrap_or(html_url).to_owned();
        if best
            .as_ref()
            .is_none_or(|(best_ver, _, _, _)| parsed > *best_ver)
        {
            best = Some((
                parsed,
                tag.trim_start_matches('v').to_owned(),
                download_url,
                html_url.to_owned(),
            ));
        }
    }

    Ok(match best {
        Some((_, version, download_url, release_url)) => UpdateStatus::Available {
            version,
            download_url,
            release_url,
        },
        None => UpdateStatus::UpToDate,
    })
}

/// Checks GitHub Releases asynchronously on the GTK main loop and passes the result to `on_done`.
pub fn check(on_done: impl FnOnce(UpdateStatus) + 'static) {
    let session = soup::Session::new();
    session.set_timeout(TIMEOUT_SECONDS);
    let Ok(message) = soup::Message::new("GET", RELEASES_URL) else {
        on_done(UpdateStatus::Failed(
            "Invalid update feed address.".to_owned(),
        ));
        return;
    };
    if let Some(headers) = message.request_headers() {
        headers.append("User-Agent", &format!("Goosic-Linux/{CURRENT_VERSION}"));
        headers.append("Accept", "application/vnd.github+json");
    }

    let fetched = session.send_and_read_future(&message, glib::Priority::LOW);
    glib::spawn_future_local(async move {
        let status = match fetched.await {
            Ok(bytes) if message.status() == soup::Status::Ok => {
                match evaluate_releases(&bytes, CURRENT_VERSION) {
                    Ok(status) => status,
                    Err(reason) => UpdateStatus::Failed(reason),
                }
            }
            Ok(_) => UpdateStatus::Failed(format!(
                "GitHub returned status {:?}.",
                message.status()
            )),
            Err(_) => UpdateStatus::Failed("Could not reach GitHub Releases.".to_owned()),
        };
        on_done(status);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_alpha_and_stable_ordering() {
        let a1 = ParsedVersion::parse("v0.4.3-linux-alpha.1").unwrap();
        let a2 = ParsedVersion::parse("v0.4.3-linux-alpha.2").unwrap();
        let stable = ParsedVersion::parse("v0.4.3").unwrap();
        let next = ParsedVersion::parse("v0.4.4-linux-alpha.1").unwrap();
        assert!(a1 < a2);
        assert!(a2 < stable);
        assert!(stable < next);
        assert!(ParsedVersion::parse("v0.4.3-macos-alpha.1").is_none());
    }

    #[test]
    fn selects_newest_linux_flatpak_and_ignores_drafts_and_macos() {
        let feed = br#"[
            {
                "tag_name": "v0.4.3-linux-alpha.3",
                "draft": true,
                "html_url": "https://github.com/AnalogicGoose/GoosicReborn/releases/tag/v0.4.3-linux-alpha.3",
                "assets": []
            },
            {
                "tag_name": "v0.4.4",
                "draft": false,
                "html_url": "https://github.com/AnalogicGoose/GoosicReborn/releases/tag/v0.4.4",
                "assets": [
                    {"name": "Goosic-0.4.4-windows-x64.zip", "browser_download_url": "https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.4.4/win.zip"}
                ]
            },
            {
                "tag_name": "v0.4.3-linux-alpha.2",
                "draft": false,
                "html_url": "https://github.com/AnalogicGoose/GoosicReborn/releases/tag/v0.4.3-linux-alpha.2",
                "assets": [
                    {"name": "Goosic-0.4.3-linux-alpha.2-linux-x86_64.flatpak", "browser_download_url": "https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.4.3-linux-alpha.2/Goosic.flatpak"}
                ]
            }
        ]"#;
        let result = evaluate_releases(feed, "0.4.3-linux-alpha.1").unwrap();
        assert_eq!(
            result,
            UpdateStatus::Available {
                version: "0.4.3-linux-alpha.2".into(),
                download_url: "https://github.com/AnalogicGoose/GoosicReborn/releases/download/v0.4.3-linux-alpha.2/Goosic.flatpak".into(),
                release_url: "https://github.com/AnalogicGoose/GoosicReborn/releases/tag/v0.4.3-linux-alpha.2".into(),
            }
        );
    }

    #[test]
    fn refuses_non_https_download_urls() {
        let feed = br#"[
            {
                "tag_name": "v0.4.3-linux-alpha.2",
                "draft": false,
                "html_url": "http://evil.example/release",
                "assets": [
                    {"name": "Goosic.flatpak", "browser_download_url": "http://evil.example/Goosic.flatpak"}
                ]
            }
        ]"#;
        let result = evaluate_releases(feed, "0.4.3-linux-alpha.1").unwrap();
        assert_eq!(result, UpdateStatus::UpToDate);
    }
}
