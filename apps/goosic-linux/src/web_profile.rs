//! Where a web profile keeps its cookies and storage, and the navigation rule both web surfaces keep.
//!
//! Every account plays in a WebKit profile of its own, so cookies never mix, and a sign-in writes
//! into a staging directory that becomes a profile only once Rust has accepted the account. This is
//! the filesystem half of that, and the guard the official player and the sign-in window share.

use std::path::{Path, PathBuf};

use goosic_shell_support::bridge;
use gtk::glib;
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::{CookiePersistentStorage, LoadEvent, NetworkSession, PolicyDecisionType, WebView};

/// A profile directory is named by its uppercase UUID, as the Swift Linux shell named it, so a
/// profile that shell wrote is found again.
fn directory_name(profile: Uuid) -> String {
    profile.hyphenated().to_string().to_uppercase()
}

fn profile_directory(profile: Uuid) -> PathBuf {
    glib::user_data_dir()
        .join("goosic")
        .join("profiles")
        .join(directory_name(profile))
}

/// The engine's HTTP cache is a cache, so it lives with the other caches.
fn cache_directory(profile: Uuid) -> PathBuf {
    glib::user_cache_dir()
        .join("goosic")
        .join("web")
        .join(directory_name(profile))
}

fn staging_directory(profile: Uuid) -> PathBuf {
    glib::user_data_dir()
        .join("goosic")
        .join("staging")
        .join(directory_name(profile))
}

/// A renderer over the storage of a profile playback uses.
pub fn profile_view(profile: Uuid) -> WebView {
    view(
        &profile_directory(profile).join("data"),
        &cache_directory(profile),
    )
}

/// A renderer over a sign-in's staged storage.
pub fn staging_view(profile: Uuid) -> WebView {
    let staging = staging_directory(profile);
    view(&staging.join("data"), &staging.join("cache"))
}

fn view(data: &Path, cache: &Path) -> WebView {
    let _ = std::fs::create_dir_all(data);
    let session = NetworkSession::new(
        Some(&*data.to_string_lossy()),
        Some(&*cache.to_string_lossy()),
    );
    // Cookies are written into the profile explicitly. A sign-in lives in its cookies, and one kept
    // only in the network process's memory would be gone the next time Goosic starts.
    if let Some(cookies) = session.cookie_manager() {
        cookies.set_persistent_storage(
            &data.join("cookies.sqlite").to_string_lossy(),
            CookiePersistentStorage::Sqlite,
        );
    }
    let view = WebView::builder().network_session(&session).build();
    if let Some(settings) = WebViewExt::settings(&view) {
        // YouTube Music refuses to run its player under a bare engine agent, and Google's sign-in
        // is wary of one, so the agent names a Safari version, as the macOS host does.
        settings.set_user_agent(Some(&format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) {}",
            bridge::SAFARI_USER_AGENT_SUFFIX
        )));
        // The user pressed play in Goosic, and that gesture does not cross into the web view.
        settings.set_media_playback_requires_user_gesture(false);
    }
    view
}

/// Makes a finished sign-in's staged storage the profile's own. Called only once Rust has stored
/// and activated the account; until then the staging directory is all there is, and discarding it
/// leaves nothing half made.
pub fn promote_staging(profile: Uuid) -> std::io::Result<()> {
    let staging = staging_directory(profile);
    let destination = profile_directory(profile);
    std::fs::create_dir_all(&destination)?;
    let data = destination.join("data");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    std::fs::rename(staging.join("data"), &data)?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

/// Deletes a sign-in's staged storage now, and again a moment later: WebKit's network process
/// outlives the view by a little and writes into the directory once more as it winds down.
pub fn discard_staging(profile: Uuid) {
    let directory = staging_directory(profile);
    let _ = std::fs::remove_dir_all(&directory);
    glib::timeout_add_seconds_local_once(3, move || {
        let _ = std::fs::remove_dir_all(&directory);
    });
}

/// Clears every staged sign-in. Called at startup, when no sign-in can be open, so one abandoned by
/// a crash or a late write does not stay on disk.
pub fn clear_abandoned_staging() {
    let _ = std::fs::remove_dir_all(glib::user_data_dir().join("goosic").join("staging"));
}

/// Deletes a removed account's cookies and storage, so removing an account signs it out of this
/// machine rather than only hiding it. The guest profile is never deleted.
pub fn delete_profile(profile: Uuid) {
    if profile == bridge::GUEST_PROFILE_ID {
        return;
    }
    let _ = std::fs::remove_dir_all(profile_directory(profile));
    let _ = std::fs::remove_dir_all(cache_directory(profile));
}

/// Keeps a renderer's main frame on the pages `allowed` accepts.
///
/// WebKitGTK asks for a policy on every frame's navigation without saying which frame is
/// navigating, so the rule is applied where only the main frame reports: when its load starts or is
/// redirected, the view's URI is the one being loaded, and a load that is not allowed is stopped
/// before it commits. Subframes are left alone on purpose. They are how YouTube Music serves
/// advertisements and how sign-in serves reCAPTCHA; refusing them would not make either surface
/// safer, it would stop sign-in completing and quietly turn a player that reports advertisements
/// into one that blocks them. New windows are refused outright: neither surface has a use for one,
/// and for the player it would be a second media owner.
pub fn guard_navigation(view: &WebView, allowed: fn(&str) -> bool) {
    view.connect_decide_policy(|_, decision, kind| {
        if kind == PolicyDecisionType::NewWindowAction {
            decision.ignore();
            return true;
        }
        false
    });
    view.connect_load_changed(move |view, event| {
        if !matches!(event, LoadEvent::Started | LoadEvent::Redirected) {
            return;
        }
        let uri = view.uri();
        if uri.as_deref().is_some_and(allowed) {
            return;
        }
        // A silent refusal looks exactly like a page that never arrives, so the host is named on
        // stderr. The host only: a URL can carry tokens in its query.
        eprintln!(
            "goosic: refused navigation to {}",
            describe_destination(uri.as_deref())
        );
        view.stop_loading();
    });
}

/// A refused destination, reduced to what can be logged: a host, or else a scheme.
fn describe_destination(uri: Option<&str>) -> String {
    match uri {
        None => "an unreadable request".to_owned(),
        Some(uri) => match url::Url::parse(uri) {
            Ok(url) => url
                .host_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("a {} URL", url.scheme())),
            Err(_) => "an unparsable URL".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_destination_is_logged_as_a_host_and_never_a_url() {
        assert_eq!(
            describe_destination(Some("https://accounts.google.com/signin?token=secret")),
            "accounts.google.com"
        );
        assert_eq!(
            describe_destination(Some("data:text/html,hi")),
            "a data URL"
        );
        assert_eq!(describe_destination(None), "an unreadable request");
    }

    #[test]
    fn a_sign_in_is_staged_apart_from_every_profile() {
        let id = Uuid::parse_str("0f3a2b1c-4d5e-4f60-8a7b-9c0d1e2f3a4b").unwrap();
        assert!(
            profile_directory(id).ends_with("goosic/profiles/0F3A2B1C-4D5E-4F60-8A7B-9C0D1E2F3A4B")
        );
        assert!(
            staging_directory(id).ends_with("goosic/staging/0F3A2B1C-4D5E-4F60-8A7B-9C0D1E2F3A4B")
        );
        assert!(!staging_directory(id).starts_with(profile_directory(id)));
    }
}
