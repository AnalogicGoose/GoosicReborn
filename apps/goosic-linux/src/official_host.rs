//! The one WebKitGTK renderer that plays the official YouTube Music page.
//!
//! The shell never produces online audio itself. It asks Rust for the `officialWebView` lease, and
//! only then does this host load the official player page, with an observer that reports what the
//! page's media element is really doing. What counts as a trustworthy report is decided by
//! `goosic-shell-support`'s bridge rules, the same ones every shell applies. What is here is only
//! WebKitGTK: making the view, installing the scripts, receiving the messages, running commands.
//!
//! Two properties are load-bearing. The bridge lives in a script world of its own: the page shares
//! the DOM with the observer but not its JavaScript scope, so it cannot see the handler to post to
//! it. That is how Linux gets the origin guarantee macOS gets from a security origin. And a network
//! session belongs to a profile and can only be given to a view as it is built, so a different
//! account means a new renderer inside a container that stays put.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use goosic_shell_support::bridge::{self, BridgeEvent};
use goosic_shell_support::playback::clamp_volume;
use gtk::prelude::*;
use gtk::{gio, glib};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::{
    LoadEvent, PolicyDecisionType, UserContentInjectedFrames, UserScript, UserScriptInjectionTime,
};

/// The bridge's script world. The page cannot reach anything registered in it.
const SCRIPT_WORLD: &str = "goosic";

/// How long a quiesce waits for the page before giving up on it. A hung web process must not hold
/// Rust's lease forever; forgetting the load's identity still stops its late reports.
const QUIESCE_TIMEOUT: Duration = Duration::from_secs(2);

/// Where the host tells the shell what happened.
pub struct Handlers {
    pub on_event: Box<dyn Fn(BridgeEvent)>,
    pub on_status: Box<dyn Fn(String)>,
    /// The official app followed its own "up next" away from the video Goosic asked for. Goosic
    /// owns the queue, so the shell decides what plays instead.
    pub on_page_advanced: Box<dyn Fn(String)>,
    /// What the loaded page contains, for when playback does not start and the page is the reason.
    pub on_diagnostics: Box<dyn Fn(String)>,
}

/// The identity of the current load, which every report must match.
struct Expectation {
    /// Not a credential: it keeps an old document's messages from being believed after a reload.
    token: String,
    generation: u64,
    video_id: String,
}

enum Verdict {
    Accepted,
    Rejected(String),
    MovedOn(String),
}

pub struct OfficialHost {
    container: gtk::Box,
    view: RefCell<Option<webkit6::WebView>>,
    profile: Cell<Uuid>,
    expected: RefCell<Option<Expectation>>,
    last_sequence: Cell<u64>,
    advertisement: Cell<bool>,
    handlers: Handlers,
}

impl OfficialHost {
    pub fn new(handlers: Handlers) -> Rc<OfficialHost> {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // The renderer has to be in the widget tree to be realised and to play. Nobody looks at it,
        // so it is one pixel and ignores the pointer.
        // A size request is only a minimum, and a web view asks to expand, so the container also
        // refuses to grow and clips what it holds.
        container.set_size_request(1, 1);
        container.set_hexpand(false);
        container.set_vexpand(false);
        container.set_halign(gtk::Align::Start);
        container.set_valign(gtk::Align::Start);
        container.set_overflow(gtk::Overflow::Hidden);
        container.set_can_target(false);
        container.set_focusable(false);
        let host = Rc::new(OfficialHost {
            container,
            view: RefCell::new(None),
            profile: Cell::new(bridge::GUEST_PROFILE_ID),
            expected: RefCell::new(None),
            last_sequence: Cell::new(0),
            advertisement: Cell::new(false),
            handlers,
        });
        host.rebuild_renderer();
        host
    }

    /// The mount point, which stays in the window while renderers come and go inside it.
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// The video of the current load, if one is loaded.
    pub fn loaded_video_id(&self) -> Option<String> {
        self.expected
            .borrow()
            .as_ref()
            .map(|expected| expected.video_id.clone())
    }

    pub fn is_advertisement(&self) -> bool {
        self.advertisement.get()
    }

    /// Moves playback onto a profile's own cookies and storage. The shell releases Rust's lease
    /// first, which is what makes tearing the old renderer down safe.
    pub fn bind_profile(self: &Rc<Self>, profile: Uuid) {
        if self.profile.get() == profile {
            return;
        }
        self.profile.set(profile);
        self.rebuild_renderer();
    }

    fn rebuild_renderer(self: &Rc<Self>) {
        // The old document is going away, so nothing it might still say is worth accepting.
        self.invalidate_expectations();
        let old = self.view.borrow_mut().take();
        if let Some(old) = old {
            old.stop_loading();
            if let Some(content) = old.user_content_manager() {
                content.remove_all_scripts();
            }
            self.container.remove(&old);
        }
        let view = build_view(self.profile.get());
        view.set_size_request(1, 1);
        view.set_hexpand(false);
        view.set_vexpand(false);
        self.attach(&view);
        self.container.append(&view);
        *self.view.borrow_mut() = Some(view);
    }

    fn attach(self: &Rc<Self>, view: &webkit6::WebView) {
        view.connect_decide_policy(|_, decision, kind| {
            if kind != PolicyDecisionType::NavigationAction {
                return false;
            }
            let uri = decision
                .downcast_ref::<webkit6::NavigationPolicyDecision>()
                .and_then(|decision| decision.navigation_action())
                .and_then(|action| action.request())
                .and_then(|request| request.uri());
            match uri {
                Some(uri) if is_official_page(&uri) => decision.use_(),
                other => {
                    // A silent refusal looks exactly like a page that never arrives, so the host is
                    // named on stderr. The host only: a URL can carry tokens in its query.
                    eprintln!(
                        "goosic: refused navigation to {}",
                        describe_destination(other.as_deref())
                    );
                    decision.ignore();
                }
            }
            true
        });

        let weak = Rc::downgrade(self);
        view.connect_load_changed(move |_, event| {
            if event != LoadEvent::Finished {
                return;
            }
            let Some(host) = weak.upgrade() else {
                return;
            };
            // The blank document a detach loads finishes loading too; calling that ready would be
            // a lie.
            if host.expected.borrow().is_none() {
                return;
            }
            host.status("Official host is ready; waiting for a validated player event.");
            host.probe_page();
        });

        if let Some(content) = view.user_content_manager() {
            if content.register_script_message_handler(bridge::HANDLER_NAME, Some(SCRIPT_WORLD)) {
                let weak = Rc::downgrade(self);
                content.connect_script_message_received(
                    Some(bridge::HANDLER_NAME),
                    move |_, value| {
                        if let (Some(host), Some(json)) = (weak.upgrade(), value.to_json(0)) {
                            host.handle_message(json.as_bytes());
                        }
                    },
                );
            }
        }
    }

    /// Loads the official page for `video_id`. Only call this while Rust holds the lease for
    /// `generation`: the page starts playing as soon as it can.
    pub fn load(&self, video_id: &str, generation: u64) {
        let view = self.view.borrow().clone();
        let Some(view) = view else {
            self.status("Official host is not attached to the window.");
            return;
        };
        if !bridge::is_valid_video_id(video_id) {
            self.status("That is not a YouTube Music video id.");
            return;
        }
        let token = Uuid::new_v4().to_string();
        *self.expected.borrow_mut() = Some(Expectation {
            token: token.clone(),
            generation,
            video_id: video_id.to_owned(),
        });
        self.last_sequence.set(0);
        self.advertisement.set(false);

        // Each load gets its own observer carrying this load's identity, so a document from an
        // earlier load can never satisfy the checks a report has to pass.
        let allow = format!("https://{}/*", bridge::ALLOWED_HOST);
        if let Some(content) = view.user_content_manager() {
            content.remove_all_scripts();
            // The media-session guard has to run in the page's own world: an isolated world has its
            // own wrappers of the page's objects, and a guard there would leave the page's session
            // untouched.
            content.add_script(&UserScript::new(
                bridge::MEDIA_SESSION_GUARD_SCRIPT,
                UserContentInjectedFrames::TopFrame,
                UserScriptInjectionTime::Start,
                &[allow.as_str()],
                &[],
            ));
            content.add_script(&UserScript::for_world(
                &bridge::observer_script(&token, generation, video_id),
                UserContentInjectedFrames::TopFrame,
                UserScriptInjectionTime::End,
                SCRIPT_WORLD,
                &[allow.as_str()],
                &[],
            ));
        }
        // The id was validated above, so it cannot change the shape of the URL.
        view.load_uri(&format!(
            "https://{}/watch?v={video_id}",
            bridge::ALLOWED_HOST
        ));
        self.status(&format!("Official host loading {video_id}…"));
    }

    pub fn play(self: &Rc<Self>) {
        self.run_on_media("media => { media.play(); return 'play-requested'; }");
    }

    pub fn pause(self: &Rc<Self>) {
        self.run_on_media("media => { media.pause(); return 'pause-requested'; }");
    }

    /// Asks for a position. Like play and pause this is a request: the position has not moved until
    /// the player reports it back.
    pub fn seek(self: &Rc<Self>, seconds: f64) {
        if !seconds.is_finite() || seconds < 0.0 {
            return;
        }
        if self.advertisement.get() {
            self.status("Seeking is unavailable while the official player shows an advertisement.");
            return;
        }
        let script = format!(
            "(() => {{
  const candidates = [
    document.querySelector('#movie_player'),
    document.querySelector('ytmusic-player'),
    document.querySelector('ytmusic-player-bar')
  ];
  const player = candidates.find(c => c && typeof c.seekTo === 'function');
  if (!player) return 'seek-unsupported';
  player.seekTo({seconds:.3}, true);
  return 'seek-requested';
}})();"
        );
        let weak = Rc::downgrade(self);
        self.evaluate(&script, move |result| {
            let Some(host) = weak.upgrade() else {
                return;
            };
            match result {
                Err(failure) => host.status(&format!("The player refused the seek: {failure}.")),
                Ok(Some(json)) if json.contains("seek-unsupported") => {
                    host.status("This official player page does not expose its seek API.")
                }
                Ok(_) => {}
            }
        });
    }

    pub fn set_volume(self: &Rc<Self>, volume: f64) {
        if self.advertisement.get() {
            self.status("Volume is unchanged while the official player shows an advertisement.");
            return;
        }
        let Some(volume) = clamp_volume(volume) else {
            return;
        };
        self.run_on_media(&format!(
            "media => {{ media.muted = false; media.volume = {volume:.3}; return 'volume-requested'; }}"
        ));
    }

    pub fn set_muted(self: &Rc<Self>, muted: bool) {
        if self.advertisement.get() {
            self.status("Mute is unavailable while the official player shows an advertisement.");
            return;
        }
        self.run_on_media(&format!(
            "media => {{ media.muted = {muted}; return 'mute-requested'; }}"
        ));
    }

    /// Pauses every media element on the page, then calls `done` — or calls it after
    /// `QUIESCE_TIMEOUT` if the page never answers.
    pub fn quiesce(self: &Rc<Self>, done: impl FnOnce() + 'static) {
        let slot: OnceSlot = Rc::new(RefCell::new(Some(Box::new(done))));
        let from_page = slot.clone();
        self.evaluate(
            "Array.from(document.querySelectorAll('audio,video')).forEach(media => media.pause()); 'quiesced';",
            move |_| fire(&from_page),
        );
        glib::timeout_add_local_once(QUIESCE_TIMEOUT, move || fire(&slot));
    }

    /// Forgets the current load's identity. Call after media is quiesced and before Rust's lease is
    /// released, so a late report from the old document cannot be forwarded.
    pub fn invalidate_expectations(&self) {
        *self.expected.borrow_mut() = None;
        self.last_sequence.set(0);
        self.advertisement.set(false);
    }

    /// Quiesces and returns the renderer to a document that can play nothing.
    pub fn detach(self: &Rc<Self>, done: impl FnOnce() + 'static) {
        self.invalidate_expectations();
        let weak = Rc::downgrade(self);
        self.quiesce(move || {
            if let Some(host) = weak.upgrade() {
                host.blank();
            }
            done();
        });
    }

    fn blank(&self) {
        let view = self.view.borrow().clone();
        if let Some(view) = view {
            view.stop_loading();
            if let Some(content) = view.user_content_manager() {
                content.remove_all_scripts();
            }
            view.load_html("<!doctype html><title>Goosic</title>", None);
        }
    }

    /// Reports what the loaded page contains. When no report ever arrives the cause is almost
    /// always the page — a consent wall, a redirect, a player that never made a media element — and
    /// that is invisible in a renderer one pixel wide.
    pub fn probe_page(self: &Rc<Self>) {
        let script = "(() => {
  const media = document.querySelectorAll('audio,video');
  return JSON.stringify({
    title: document.title || '',
    media: media.length,
    readyState: media[0] ? media[0].readyState : -1,
    paused: media[0] ? media[0].paused : null
  });
})();";
        let weak = Rc::downgrade(self);
        self.evaluate(script, move |result| {
            let Some(host) = weak.upgrade() else {
                return;
            };
            let report = match result {
                Ok(Some(json)) => json,
                Ok(None) => "The page probe returned nothing.".to_owned(),
                Err(failure) => format!("The page probe failed: {failure}."),
            };
            (host.handlers.on_diagnostics)(report);
        });
    }

    fn run_on_media(self: &Rc<Self>, function: &str) {
        let script = format!(
            "(() => {{ const media = document.querySelector('audio,video'); if (!media) return 'no-media'; return ({function})(media); }})();"
        );
        let weak = Rc::downgrade(self);
        self.evaluate(&script, move |result| {
            if let (Err(failure), Some(host)) = (result, weak.upgrade()) {
                host.status(&format!(
                    "The official player refused the command: {failure}."
                ));
            }
        });
    }

    /// Runs `script` in the bridge's world, which shares the page's DOM but not its scope, so the
    /// player can be driven without the page observing the driver. The result arrives as JSON.
    fn evaluate(&self, script: &str, done: impl FnOnce(Result<Option<String>, String>) + 'static) {
        let view = self.view.borrow().clone();
        let Some(view) = view else {
            done(Err("the renderer is not attached".to_owned()));
            return;
        };
        view.evaluate_javascript(
            script,
            Some(SCRIPT_WORLD),
            None,
            None::<&gio::Cancellable>,
            move |result| {
                done(
                    result
                        .map(|value| value.to_json(0).map(|json| json.to_string()))
                        .map_err(|error| error.message().to_owned()),
                )
            },
        );
    }

    fn handle_message(&self, body: &[u8]) {
        let Some(event) = bridge::parse_event(body) else {
            self.status("Rejected an invalid official-player bridge message.");
            return;
        };
        let verdict = {
            let expected = self.expected.borrow();
            let token = expected.as_ref().map(|e| e.token.as_str());
            let generation = expected.as_ref().map(|e| e.generation);
            let video = expected.as_ref().map(|e| e.video_id.as_str());
            // A well-formed report for a different video means the official app moved on by itself.
            // That is not noise to shrug at: it is the signal that Goosic's own queue takes over.
            let moved_on = event.version == bridge::EVENT_VERSION
                && Some(event.token.as_str()) == token
                && Some(event.generation) == generation
                && !event.is_advertisement
                && !event.video_id.is_empty()
                && video.is_some_and(|video| video != event.video_id);
            if moved_on {
                Verdict::MovedOn(video.unwrap_or_default().to_owned())
            } else {
                match bridge::rejection_reason(
                    &event,
                    token,
                    generation,
                    video,
                    self.last_sequence.get(),
                ) {
                    Some(reason) => Verdict::Rejected(reason),
                    None => Verdict::Accepted,
                }
            }
        };
        match verdict {
            Verdict::MovedOn(requested) => {
                self.status(
                    "The official app moved to its own next video; Goosic's queue decides.",
                );
                (self.handlers.on_page_advanced)(requested);
            }
            Verdict::Rejected(reason) => {
                self.status(&format!("Rejected an official-player report: {reason}."))
            }
            Verdict::Accepted => {
                self.last_sequence.set(event.sequence);
                self.advertisement.set(event.is_advertisement);
                (self.handlers.on_event)(event);
            }
        }
    }

    fn status(&self, message: &str) {
        (self.handlers.on_status)(message.to_owned());
    }
}

/// A callback shared by two paths that race to call it, of which only the first one does.
type OnceSlot = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

fn fire(slot: &OnceSlot) {
    let done = slot.borrow_mut().take();
    if let Some(done) = done {
        done();
    }
}

/// Builds a renderer whose cookies and storage belong to `profile`.
fn build_view(profile: Uuid) -> webkit6::WebView {
    let name = profile.hyphenated().to_string().to_uppercase();
    // The data directory is the one the Swift Linux shell used, so a sign-in made there carries
    // over. The engine's HTTP cache is a cache, so it lives with the other caches.
    let data = glib::user_data_dir()
        .join("goosic")
        .join("profiles")
        .join(&name)
        .join("data");
    let cache = glib::user_cache_dir()
        .join("goosic")
        .join("web")
        .join(&name);
    let session = webkit6::NetworkSession::new(
        Some(&*data.to_string_lossy()),
        Some(&*cache.to_string_lossy()),
    );
    let view = webkit6::WebView::builder()
        .network_session(&session)
        .build();
    if let Some(settings) = WebViewExt::settings(&view) {
        // YouTube Music refuses to run its player under a bare engine agent, so the agent names a
        // Safari version, as the macOS host does.
        settings.set_user_agent(Some(&format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) {}",
            bridge::SAFARI_USER_AGENT_SUFFIX
        )));
        // The user pressed play in Goosic, and that gesture does not cross into the web view.
        settings.set_media_playback_requires_user_gesture(false);
    }
    view.set_hexpand(true);
    view.set_vexpand(true);
    view
}

/// Whether the player may navigate its main frame to `uri`.
///
/// The host must match exactly. The Swift shell compared a string prefix, which also admits
/// `music.youtube.com.evil.example`; a parsed host cannot be fooled that way. `about:blank` is
/// where a fresh view starts and carries no origin to be redirected to.
fn is_official_page(uri: &str) -> bool {
    uri == "about:blank"
        || url::Url::parse(uri).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str() == Some(bridge::ALLOWED_HOST)
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
        })
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
    fn only_the_official_host_itself_is_a_player_page() {
        assert!(is_official_page(
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_official_page("about:blank"));
        // Each of these passes a prefix check and must not pass this one.
        assert!(!is_official_page(
            "https://music.youtube.com.evil.example/watch"
        ));
        assert!(!is_official_page("https://music.youtube.community/"));
        assert!(!is_official_page("https://user@music.youtube.com/"));
        assert!(!is_official_page("http://music.youtube.com/"));
        assert!(!is_official_page("https://music.youtube.com:8443/"));
        assert!(!is_official_page(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
    }

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
}
