//! Where a sign-in window may go, and when a sign-in counts as finished.
//!
//! A window that follows an arbitrary redirect is a window collecting a password for somebody
//! else, so the navigation rule stays narrow — and it lives here, shared, because a rule about
//! where credentials may be typed must not have one version per platform that can drift. What
//! crosses from the page is a small metadata object and nothing else: no cookies, no headers, no
//! URL query values.

use std::time::{Duration, Instant};

use goosic_protocol::{AccountSummary, AccountsSnapshot};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::navigation::PlaybackTransition;
use crate::text::{is_control_or_format, trim_whitespace};

/// How long a staged sign-in may stay open before it is abandoned.
///
/// The macOS branch has since raised this to three minutes and restarts it on every navigation,
/// because a second-factor prompt can take a minute of attention. This is the `development` value
/// and moves with that branch when it lands.
pub const COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);

/// The YouTube hosts a sign-in passes through. These have no country variants, so they stay an
/// exact list.
pub const ALLOWED_LOGIN_HOSTS: [&str; 3] = ["accounts.youtube.com", "www.youtube.com", "music.youtube.com"];

/// The Google services a sign-in is allowed to visit, as the leftmost label.
pub const GOOGLE_SIGN_IN_LABELS: [&str; 5] = ["accounts", "consent", "myaccount", "gds", "ogs"];

pub const MAX_METADATA_BYTES: usize = 16 * 1024;
pub const MAX_DISPLAY_NAME_BYTES: usize = 128;
pub const MAX_EMAIL_BYTES: usize = 320;
pub const MAX_CHANNEL_BYTES: usize = 128;
pub const MAX_AVATAR_URL_BYTES: usize = 2_048;

/// The display name used when the page offers none worth keeping.
pub const FALLBACK_DISPLAY_NAME: &str = "YouTube Music account";

/// What the page reports about the account that signed in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginSummary {
    pub display_name: String,
    pub email: Option<String>,
    pub channel: Option<String>,
    pub avatar_url: Option<String>,
}

/// A finished sign-in, ready to be stored as account metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginResult {
    pub account_id: Uuid,
    pub profile_id: Uuid,
    pub summary: LoginSummary,
}

/// Whether the page's report is enough to call the sign-in complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionDecision {
    Wait,
    Accept(LoginSummary),
}

impl CompletionDecision {
    /// A display name is easy to obtain from a signed-out or default shell, so a second,
    /// account-specific field is required before the page-side marker is believed.
    pub fn from(summary: Option<&LoginSummary>) -> CompletionDecision {
        let Some(summary) = summary else {
            return CompletionDecision::Wait;
        };
        let has_email_or_channel = [&summary.email, &summary.channel]
            .into_iter()
            .flatten()
            .any(|value| !trim_whitespace(value).is_empty());
        let has_valid_avatar = summary
            .avatar_url
            .as_deref()
            .is_some_and(|avatar| is_https_without_credentials(trim_whitespace(avatar)));
        if has_email_or_channel || has_valid_avatar {
            CompletionDecision::Accept(summary.clone())
        } else {
            CompletionDecision::Wait
        }
    }
}

/// What a polling tick should do with a staged sign-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingDecision {
    Wait,
    Accept,
    Cancel,
}

impl PollingDecision {
    /// A tick from a sign-in that has been replaced, or one past its deadline, cancels; a page on
    /// any origin but the exact completion origin waits; otherwise the page's report decides.
    pub fn decide(
        token: u64,
        active_token: u64,
        now: Instant,
        deadline: Instant,
        exact_origin: bool,
        summary: Option<&LoginSummary>,
    ) -> PollingDecision {
        if token != active_token || now >= deadline {
            return PollingDecision::Cancel;
        }
        if !exact_origin {
            return PollingDecision::Wait;
        }
        match CompletionDecision::from(summary) {
            CompletionDecision::Wait => PollingDecision::Wait,
            CompletionDecision::Accept(_) => PollingDecision::Accept,
        }
    }
}

/// Whether the login surface may navigate its main frame to `url`.
pub fn is_allowed_login_url(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    // `port()` is `None` for an explicit `:443` too, because the parser drops the default.
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ALLOWED_LOGIN_HOSTS.contains(&host.as_str()) || is_google_sign_in_host(&host)
}

/// Whether `host` is one of Google's own sign-in services on one of its country domains.
///
/// This is the one place the rule stopped being a plain list, and it was forced: Google localises
/// sign-in onto country domains, and a Nicaraguan attempt died at `accounts.google.com.ni`. There
/// are roughly two hundred of those and they are not a set anybody can keep correct by hand, so the
/// shape is checked instead of the spelling.
///
/// What the shape still refuses is the thing the list existed to refuse. The service label must be
/// one Goosic knows, the second label must be exactly `google`, and what follows may only be one or
/// two labels of at most three letters — a public suffix and nothing else.
/// `accounts.google.com.evil.example` has too many labels, `accounts.google.evil.com` has one that
/// is too long, and `evil.google.com` is not a service on the list. What it does concede is a
/// country registry that Google does not own: someone holding `google.zz` could serve
/// `accounts.google.zz`. That is a narrower hole than a wildcard and a wider one than an
/// enumeration, and it is the trade this makes knowingly.
pub fn is_google_sign_in_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if !(labels.len() == 3 || labels.len() == 4) {
        return false;
    }
    if labels[1] != "google" || !GOOGLE_SIGN_IN_LABELS.contains(&labels[0]) {
        return false;
    }
    labels[2..]
        .iter()
        .all(|label| !label.is_empty() && label.len() <= 3 && label.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// Whether a page is on the one origin a completed sign-in lands on.
pub fn is_exact_completion_origin(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some(ALLOWED_HOST_FOR_COMPLETION)
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

const ALLOWED_HOST_FOR_COMPLETION: &str = "music.youtube.com";

/// Bounds and cleans what the page reported, or refuses it.
pub fn sanitize_metadata(data: &[u8]) -> Option<LoginSummary> {
    if data.len() > MAX_METADATA_BYTES {
        return None;
    }
    let raw: LoginSummary = serde_json::from_slice(data).ok()?;
    Some(LoginSummary {
        display_name: clean(Some(&raw.display_name), MAX_DISPLAY_NAME_BYTES)
            .unwrap_or_else(|| FALLBACK_DISPLAY_NAME.to_owned()),
        email: clean(raw.email.as_deref(), MAX_EMAIL_BYTES),
        channel: clean(raw.channel.as_deref(), MAX_CHANNEL_BYTES),
        avatar_url: clean(raw.avatar_url.as_deref(), MAX_AVATAR_URL_BYTES)
            .filter(|avatar| is_https_without_credentials(avatar)),
    })
}

/// The finished sign-in, or `None` unless every condition holds: two distinct profile identities,
/// the exact completion origin, metadata that survives cleaning, and a report worth believing.
pub fn make_result(
    account_id: Uuid,
    profile_id: Uuid,
    metadata: &[u8],
    page_url: &str,
) -> Option<LoginResult> {
    if account_id == profile_id || !is_exact_completion_origin(page_url) {
        return None;
    }
    let summary = sanitize_metadata(metadata)?;
    match CompletionDecision::from(Some(&summary)) {
        CompletionDecision::Accept(summary) => Some(LoginResult { account_id, profile_id, summary }),
        CompletionDecision::Wait => None,
    }
}

/// Whether `value` is a UUID in its canonical hyphenated spelling. Braced, URN and unhyphenated
/// forms are refused, because a profile identity stored two ways is two profiles.
pub fn is_valid_stable_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.hyphenated().to_string() == value.to_lowercase())
}

/// Whether an account and its web profile have distinct, canonical identities.
pub fn are_distinct_profiles(account_id: &str, profile_id: &str) -> bool {
    is_valid_stable_uuid(account_id)
        && is_valid_stable_uuid(profile_id)
        && account_id.to_lowercase() != profile_id.to_lowercase()
}

/// The active account in a snapshot, if the snapshot names one it contains.
pub fn active_account(snapshot: &AccountsSnapshot) -> Option<&AccountSummary> {
    let id = snapshot.active_account_id.as_deref()?;
    snapshot.accounts.iter().find(|account| account.id == id)
}

/// Whether a snapshot is new enough to replace the one on screen. An older epoch is a late answer
/// and is ignored, except for the first state persisted before this process started.
pub fn accepts_snapshot_epoch(epoch: u64, current_epoch: u64, initial: bool) -> bool {
    initial || epoch >= current_epoch
}

/// Whether an account change may start. Never during an advertisement, and never while another
/// playback transition is in flight.
pub fn can_start_account_transition(advertisement: bool, transition: PlaybackTransition) -> bool {
    !advertisement && transition == PlaybackTransition::Idle
}

/// Whether playback controls may act while an account operation is or is not in progress.
pub fn can_interact(account_operation_in_progress: bool) -> bool {
    !account_operation_in_progress
}

/// The command that moves to `target`: the compatibility command for guest, which accepts no
/// account, and `accounts.activate` for a real one.
pub fn activation_command(target: Option<&str>) -> &'static str {
    if target.is_none() {
        "account.change"
    } else {
        "accounts.activate"
    }
}

/// Whether a staged sign-in may be kept. All three steps must have succeeded; anything less is
/// rolled back rather than left half-committed.
pub fn can_commit_staging(upsert: bool, activation: bool, rebind: bool) -> bool {
    upsert && activation && rebind
}

pub fn should_discard_staging(upsert: bool, activation: bool, rebind: bool) -> bool {
    !can_commit_staging(upsert, activation, rebind)
}

/// Runs in the page only after exact-origin navigation. The marker requires an account-menu
/// affordance in the authenticated shell; arriving at music.youtube.com alone is not enough.
///
/// This lives beside the rules that judge its output rather than in any host: what counts as a
/// completed sign-in must not depend on which WebKit is running. It is the `development` version,
/// evaluated as an expression; the macOS branch has since rewritten it as an async body that reads
/// the page's own `LOGGED_IN` flag, and this copy moves with that branch when it lands.
pub const COMPLETION_SCRIPT: &str = r#"(() => {
  if (location.origin !== 'https://music.youtube.com') return '';
  const marker = document.querySelector('#avatar-btn, button[aria-label*="Account"], [aria-label*="Google Account"]');
  if (!marker) return '';
  const clip = (value, limit) => (value || '').trim().slice(0, limit);
  const text = (selector, limit) => clip(document.querySelector(selector)?.textContent, limit);
  const attr = (selector, name, limit) => clip(document.querySelector(selector)?.getAttribute(name), limit);
  const visible = (element) => {
    if (!element) return false;
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const signInVisible = Array.from(document.querySelectorAll('a,button,[role="button"]'))
    .some((element) => visible(element) && /sign\s*in|log\s*in/i.test(element.textContent || element.getAttribute('aria-label') || ''));
  if (signInVisible) return '';
  const identity = clip(marker.getAttribute('aria-label') || marker.getAttribute('title') || '', 320);
  const signedOut = /sign\s*in|log\s*in|not\s*signed/i.test(identity);
  const email = text('#account-email', 320);
  const channel = text('ytmusic-account-menu-renderer #channel-title', 128);
  const avatarUrl = attr('#avatar-btn img', 'src', 2048);
  if (signedOut) return '';
  if (!identity && !email && !channel) {
    try { marker.click(); } catch (_) {}
    return '';
  }
  return JSON.stringify({
    displayName: text('#account-name', 128) || clip(identity, 128),
    email, channel, avatarUrl
  });
})();
"#;

fn clean(value: Option<&str>, max_bytes: usize) -> Option<String> {
    let trimmed = trim_whitespace(value?);
    let acceptable =
        !trimmed.is_empty() && trimmed.len() <= max_bytes && !trimmed.chars().any(is_control_or_format);
    acceptable.then(|| trimmed.to_owned())
}

fn is_https_without_credentials(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(display_name: &str, email: Option<&str>, avatar_url: Option<&str>) -> LoginSummary {
        LoginSummary {
            display_name: display_name.into(),
            email: email.map(Into::into),
            channel: None,
            avatar_url: avatar_url.map(Into::into),
        }
    }

    #[test]
    fn profile_uuids_must_be_canonical_and_distinct() {
        assert!(are_distinct_profiles(
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222"
        ));
        assert!(!are_distinct_profiles("not-a-uuid", "22222222-2222-2222-2222-222222222222"));
        assert!(!are_distinct_profiles(
            "11111111-1111-1111-1111-111111111111",
            "11111111-1111-1111-1111-111111111111"
        ));
        // The same identity spelled differently is still one identity, and a non-canonical
        // spelling is refused outright.
        assert!(!are_distinct_profiles(
            "11111111-1111-1111-1111-111111111111",
            "11111111111111111111111111111111"
        ));
        assert!(!is_valid_stable_uuid("{11111111-1111-1111-1111-111111111111}"));
        assert!(is_valid_stable_uuid("8E4CA2CD-373A-46E3-A5B0-9A2A7B3B5084"));
    }

    #[test]
    fn a_login_summary_is_bounded_and_sanitized() {
        let raw = serde_json::to_vec(&summary("  Ada  ", Some("ada@example.test"), Some("https://example.test/avatar.jpg")))
            .unwrap();
        let cleaned = sanitize_metadata(&raw).expect("a clean summary must survive");
        assert_eq!(cleaned.display_name, "Ada");
        assert_eq!(cleaned.avatar_url.as_deref(), Some("https://example.test/avatar.jpg"));
        assert_eq!(sanitize_metadata(&vec![b' '; MAX_METADATA_BYTES + 1]), None);
    }

    #[test]
    fn hostile_metadata_is_cleaned_rather_than_stored() {
        // JSON escapes rather than the characters themselves, so the test shows what it sends:
        // a right-to-left override in the name and a zero-width space in the channel.
        let raw = r#"{"displayName":"Ada\u202egnp.exe","email":"   ","channel":"x\u200by",
                      "avatarUrl":"https://user:pw@example.test/a.jpg"}"#;
        let cleaned = sanitize_metadata(raw.as_bytes()).unwrap();
        assert_eq!(cleaned.display_name, FALLBACK_DISPLAY_NAME, "a bidi override is refused");
        assert_eq!(cleaned.email, None, "blank is absent");
        assert_eq!(cleaned.channel, None, "a zero-width space is refused");
        assert_eq!(cleaned.avatar_url, None, "an avatar carrying credentials is refused");
        let long_name = format!(r#"{{"displayName":"{}"}}"#, "a".repeat(MAX_DISPLAY_NAME_BYTES + 1));
        assert_eq!(sanitize_metadata(long_name.as_bytes()).unwrap().display_name, FALLBACK_DISPLAY_NAME);
    }

    #[test]
    fn completion_requires_the_exact_origin_and_a_second_identity_field() {
        assert!(is_exact_completion_origin("https://music.youtube.com"));
        assert!(!is_exact_completion_origin("https://www.youtube.com"));
        assert!(!is_exact_completion_origin("http://music.youtube.com"));
        assert!(!is_exact_completion_origin("https://music.youtube.com:8443/"));
        let decide = |s: LoginSummary| CompletionDecision::from(Some(&s));
        assert_eq!(decide(summary(FALLBACK_DISPLAY_NAME, None, None)), CompletionDecision::Wait);
        assert_eq!(decide(summary("Ada", None, None)), CompletionDecision::Wait);
        assert_eq!(decide(summary("Ada", None, Some("http://evil.example/avatar"))), CompletionDecision::Wait);
        assert_ne!(decide(summary("Ada", Some("ada@example.test"), None)), CompletionDecision::Wait);
        assert_ne!(decide(summary("Ada", None, Some("https://lh3.example/a"))), CompletionDecision::Wait);
        assert_eq!(CompletionDecision::from(None), CompletionDecision::Wait);
    }

    #[test]
    fn a_result_needs_every_condition_at_once() {
        let account = Uuid::from_u128(1);
        let profile = Uuid::from_u128(2);
        let metadata = br#"{"displayName":"Ada","email":"ada@example.test"}"#;
        let page = "https://music.youtube.com/";
        assert!(make_result(account, profile, metadata, page).is_some());
        assert!(make_result(account, account, metadata, page).is_none(), "one identity for both");
        assert!(make_result(account, profile, metadata, "https://www.youtube.com/").is_none());
        assert!(make_result(account, profile, br#"{"displayName":"Ada"}"#, page).is_none());
    }

    #[test]
    fn polling_rejects_stale_tokens_and_expired_deadlines() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(30);
        let ada = summary("Ada", Some("ada@example.test"), None);
        assert_eq!(PollingDecision::decide(1, 2, now, deadline, true, Some(&ada)), PollingDecision::Cancel);
        assert_eq!(
            PollingDecision::decide(2, 2, now + Duration::from_secs(31), deadline, true, Some(&ada)),
            PollingDecision::Cancel
        );
        assert_eq!(PollingDecision::decide(2, 2, now, deadline, false, Some(&ada)), PollingDecision::Wait);
        assert_eq!(PollingDecision::decide(2, 2, now, deadline, true, Some(&ada)), PollingDecision::Accept);
    }

    #[test]
    fn login_navigation_allows_only_the_https_allowlist() {
        assert!(is_allowed_login_url("https://accounts.google.com/signin"));
        assert!(is_allowed_login_url("https://music.youtube.com/"));
        // Each of these was refused during a real sign-in, which is why they are covered.
        assert!(is_allowed_login_url("https://accounts.youtube.com/accounts/SetSID"));
        assert!(is_allowed_login_url("https://gds.google.com/web/consent"));
        assert!(!is_allowed_login_url("https://evil.example/"));
        assert!(!is_allowed_login_url("http://accounts.google.com/"));
        assert!(!is_allowed_login_url("about:blank"));
        assert!(!is_allowed_login_url("https://user:pass@accounts.google.com/"));
        assert!(!is_allowed_login_url("https://accounts.google.com:8443/"));
        assert!(!is_allowed_login_url(""));
    }

    /// Google localises sign-in onto country domains, so the rule checks the shape of a host
    /// rather than its exact spelling. These are the shapes it must keep refusing.
    #[test]
    fn google_country_domains_are_accepted_without_opening_the_door() {
        for allowed in [
            "https://accounts.google.com.ni/signin", // the one that stalled a real sign-in
            "https://accounts.google.es/",
            "https://accounts.google.co.uk/",
            "https://consent.google.com.mx/",
            "https://myaccount.google.de/",
        ] {
            assert!(is_allowed_login_url(allowed), "{allowed} should be allowed");
        }
        for refused in [
            "https://accounts.google.com.evil.example/", // too many labels after google
            "https://accounts.google.evil.com/",         // a label too long to be a suffix
            "https://evil.google.com/",                  // not a service this knows
            "https://accounts.notgoogle.com/",           // second label is not google
            "https://accounts.google.com.ni.evil.example/",
            "https://google.com/",      // no service label at all
            "https://accounts.google/", // no suffix at all
            "https://accounts.google.com./", // a trailing dot is an empty label
        ] {
            assert!(!is_allowed_login_url(refused), "{refused} should be refused");
        }
    }

    #[test]
    fn account_transitions_are_gated() {
        assert!(can_start_account_transition(false, PlaybackTransition::Idle));
        assert!(!can_start_account_transition(true, PlaybackTransition::Idle));
        assert!(!can_start_account_transition(false, PlaybackTransition::Releasing));
        assert_eq!(activation_command(None), "account.change");
        assert_eq!(activation_command(Some("account")), "accounts.activate");
        assert!(can_commit_staging(true, true, true));
        assert!(should_discard_staging(true, false, true));
        assert!(can_interact(false));
        assert!(!can_interact(true));
    }

    /// The staged transaction has durable metadata after upsert but no activation or rebind, so
    /// production takes the rollback path.
    #[test]
    fn a_release_failure_after_upsert_routes_to_rollback() {
        assert!(should_discard_staging(true, false, false));
        assert!(!can_commit_staging(true, false, false));
    }

    #[test]
    fn a_stale_snapshot_epoch_is_ignored_but_the_initial_persisted_state_is_accepted() {
        assert!(!accepts_snapshot_epoch(2, 3, false));
        assert!(accepts_snapshot_epoch(0, 3, true));
        assert!(accepts_snapshot_epoch(3, 3, false));
    }

    #[test]
    fn the_active_account_is_the_one_the_snapshot_names() {
        let wire = r#"{"accounts":[{"id":"11111111-1111-1111-1111-111111111111",
            "webkitProfileId":"22222222-2222-2222-2222-222222222222","displayName":"Ada",
            "email":"ada@example.test"}],"activeAccountId":"11111111-1111-1111-1111-111111111111",
            "epoch":4}"#;
        let snapshot: AccountsSnapshot = serde_json::from_str(wire).unwrap();
        assert_eq!(active_account(&snapshot).map(|a| a.display_name.as_str()), Some("Ada"));
        let guest = AccountsSnapshot { active_account_id: None, ..snapshot.clone() };
        assert_eq!(active_account(&guest), None);
        let dangling = AccountsSnapshot { active_account_id: Some("gone".into()), ..snapshot };
        assert_eq!(active_account(&dangling), None);
    }
}
