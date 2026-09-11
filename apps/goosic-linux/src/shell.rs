//! The shell's state for the life of the application, and the window it shows.
//!
//! The service connection belongs to the application rather than to a window, because closing the
//! window will not end the conversation once the shell plays in the background. Navigation lives
//! in a `Browser` and playback in a `Player`, both of which decide without GTK; this file carries
//! answers from the service and the renderer into them, and redraws.
//!
//! Every playback change goes through Rust first. The shell asks for the lease, and only once it is
//! granted does a renderer load anything; it releases the lease only after the renderer has been
//! quiesced, so there is no moment when a page plays that Rust does not know about.
//!
//! One discipline runs through the whole file: no host is called and nothing is redrawn while a
//! `RefCell` borrow is open. Hosts report back synchronously, and a report that finds the state
//! already borrowed is a panic rather than a bug report.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use goosic_protocol::{Owner, PreferencesPatch, RequestPayload, ResponseEnvelope};
use goosic_shell_support::bridge::BridgeEvent;
use goosic_shell_support::catalog::{PageView, Track};
use goosic_shell_support::navigation::{
    CatalogKey, EntityReference, PlaybackTransition, Route, SearchFilter,
};
use goosic_shell_support::playback::{
    believes_sample, clamp_seek, clamp_volume, is_seekable, merge_preferences,
    should_advance_after_end, volume_sync, PendingSeek, ReportedSample, VolumeSync,
    PREFERENCE_SAVE_DELAY,
};
use goosic_shell_support::{ServiceClient, TransportError};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::official_host::{self, OfficialHost};
use crate::pages::Browser;
use crate::playback::Player;
use crate::player_bar::{BarActions, PlayerBar};
use crate::{bridge, service, ui};

type Answer = Result<ResponseEnvelope, TransportError>;

const PENDING: &str = "A playback change is still being confirmed; try again in a moment.";
const IN_ADVERTISEMENT: &str =
    "Track changes are unavailable while the official player shows an advertisement.";

pub struct Shell {
    pub window: gtk::ApplicationWindow,
    client: Option<ServiceClient>,
    browser: RefCell<Browser>,
    player: RefCell<Player>,
    official: Rc<OfficialHost>,
    rows: gio::ListStore,
    scroller: gtk::ScrolledWindow,
    search_bar: gtk::Box,
    routes: gtk::ListBox,
    bar: PlayerBar,
    connection: gtk::Label,
    status: gtk::Label,
    diagnostics: RefCell<String>,
    pending_preferences: RefCell<Option<PreferencesPatch>>,
    preference_token: Cell<u64>,
}

fn no_context() -> Rc<[Track]> {
    Rc::from(Vec::new())
}

impl Shell {
    /// Builds the window, starts the service and asks it to say hello.
    pub fn start(app: &gtk::Application) -> Rc<Shell> {
        let launched = service::launch();
        let shell = Rc::new_cyclic(|weak: &Weak<Shell>| {
            let actions = Rc::new(ui::Actions {
                open: Box::new(forward(weak, Shell::open)),
                play: Box::new({
                    let weak = weak.clone();
                    move |track, context| {
                        if let Some(shell) = weak.upgrade() {
                            shell.play(track, context);
                        }
                    }
                }),
                retry: Box::new(forward_unit(weak, Shell::retry)),
                back: Box::new(forward_unit(weak, Shell::back)),
            });
            let (scroller, rows) = ui::page_list(actions);
            let search_bar = ui::search_bar(
                forward(weak, Shell::submit_search),
                forward(weak, Shell::select_filter),
            );
            let sidebar = ui::sidebar(forward(weak, Shell::navigate));
            let official = OfficialHost::new(official_host::Handlers {
                on_event: Box::new(forward(weak, Shell::receive_official)),
                on_status: Box::new(forward(weak, Shell::host_status)),
                on_page_advanced: Box::new(forward(weak, Shell::official_moved_on)),
                on_diagnostics: Box::new(forward(weak, Shell::host_diagnostics)),
            });
            let bar = PlayerBar::new(BarActions {
                toggle_pause: Box::new(forward_unit(weak, Shell::toggle_pause)),
                previous: Box::new(forward_unit(weak, Shell::previous)),
                next: Box::new(forward_unit(weak, Shell::next)),
                seek: Box::new(forward(weak, Shell::seek)),
                volume: Box::new(forward(weak, Shell::set_volume)),
                mute: Box::new(forward_unit(weak, Shell::toggle_muted)),
                shuffle: Box::new(forward_unit(weak, Shell::toggle_shuffle)),
                repeat: Box::new(forward_unit(weak, Shell::cycle_repeat)),
                autoplay: Box::new(forward_unit(weak, Shell::toggle_autoplay)),
                radio: Box::new(forward_unit(weak, Shell::start_radio)),
                stop: Box::new(forward_unit(weak, Shell::release_playback)),
            });

            let client = match launched {
                Ok(client) => Some(client),
                Err(error) => {
                    eprintln!("goosic: {error}");
                    sidebar.connection.set_label("○ Service offline");
                    sidebar
                        .status
                        .set_label(&format!("Could not start goosic-service.\n{error}"));
                    None
                }
            };

            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_hexpand(true);
            content.append(&search_bar);
            content.append(&scroller);
            content.append(official.widget());
            content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            content.append(&bar.root);
            let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            root.append(&sidebar.root);
            root.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            root.append(&content);

            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("Goosic")
                .default_width(1100)
                .default_height(760)
                .child(&root)
                .build();

            Shell {
                window,
                client,
                browser: RefCell::new(Browser::new()),
                player: RefCell::new(Player::new()),
                official,
                rows,
                scroller,
                search_bar,
                routes: sidebar.routes,
                bar,
                connection: sidebar.connection,
                status: sidebar.status,
                diagnostics: RefCell::new(String::new()),
                pending_preferences: RefCell::new(None),
                preference_token: Cell::new(0),
            }
        });
        shell.render();
        shell.refresh_player();
        shell.connect();
        shell
    }

    /// What the official page last reported about itself, for when playback does not start.
    pub fn page_diagnostics(&self) -> String {
        self.diagnostics.borrow().clone()
    }

    // MARK: service

    /// Sends a command and hands its answer to `done` on the main loop. A failure that means the
    /// channel is gone is also shown as the service going offline.
    fn send(
        self: &Rc<Self>,
        command: &str,
        payload: RequestPayload,
        done: impl FnOnce(&Rc<Shell>, Answer) + 'static,
    ) {
        let Some(client) = &self.client else {
            done(
                self,
                Err(TransportError::Unavailable(
                    "goosic-service is not running".to_owned(),
                )),
            );
            return;
        };
        let answer = bridge::request(client, command, payload);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = answer.await;
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if let Err(error) = &result {
                if error.invalidates_connection() {
                    shell.connection.set_label("○ Service offline");
                    shell.status.set_label(&error.to_string());
                }
            }
            done(&shell, result);
        });
    }

    /// The first exchange is `hello`, before anything else is asked, so a service that is missing
    /// or speaks another protocol version fails while the sidebar still says "Connecting".
    fn connect(self: &Rc<Self>) {
        if self.client.is_none() {
            return;
        }
        self.send("hello", RequestPayload::default(), |shell, answer| {
            shell.show_connection(answer)
        });
    }

    fn show_connection(self: &Rc<Self>, answer: Answer) {
        match answer {
            Ok(response) => {
                self.apply_lease(&response);
                let message = response
                    .payload
                    .and_then(|payload| payload.message)
                    .unwrap_or_else(|| "goosic-service ready".to_owned());
                eprintln!("goosic: connected — {message}");
                self.connection.set_label("● Rust service connected");
                self.status.set_label("");
                self.load_preferences();
            }
            Err(error) => {
                let (code, message) = error.describe();
                eprintln!("goosic: could not connect ({code}): {message}");
                self.connection.set_label("○ Service offline");
                self.status
                    .set_label(&format!("Could not reach goosic-service.\n{message}"));
            }
        }
    }

    /// Reads stored preferences and applies them before the first page loads, so the app opens
    /// where it was left rather than snapping there a moment later.
    fn load_preferences(self: &Rc<Self>) {
        self.send(
            "settings.get",
            RequestPayload::default(),
            |shell, answer| {
                let settings = answer
                    .ok()
                    .and_then(|response| response.payload)
                    .and_then(|payload| payload.settings);
                if let Some(settings) = settings {
                    shell.player.borrow_mut().apply_settings(&settings);
                    if let Some(route) = Route::from_raw(&settings.last_route) {
                        shell.browser.borrow_mut().navigate(route);
                        ui::select_route(&shell.routes, route);
                    }
                }
                shell.refresh_player();
                shell.show_new_page();
            },
        );
    }

    /// Queues a preference change, coalescing rapid ones such as a volume drag into one save.
    fn save_preferences(self: &Rc<Self>, patch: PreferencesPatch) {
        let merged = merge_preferences(self.pending_preferences.borrow_mut().take(), patch);
        *self.pending_preferences.borrow_mut() = Some(merged);
        let token = self.preference_token.get() + 1;
        self.preference_token.set(token);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(PREFERENCE_SAVE_DELAY, move || {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if shell.preference_token.get() != token {
                return;
            }
            let Some(patch) = shell.pending_preferences.borrow_mut().take() else {
                return;
            };
            let payload = RequestPayload {
                preferences: Some(patch),
                ..Default::default()
            };
            shell.send("settings.set", payload, |_, _| {});
        });
    }

    // MARK: navigation

    fn navigate(self: &Rc<Self>, route: Route) {
        self.browser.borrow_mut().navigate(route);
        self.show_new_page();
        self.save_preferences(PreferencesPatch {
            last_route: Some(route.raw_value().to_owned()),
            ..Default::default()
        });
    }

    fn open(self: &Rc<Self>, entity: EntityReference) {
        self.browser.borrow_mut().open(entity);
        self.show_new_page();
    }

    fn back(self: &Rc<Self>) {
        self.browser.borrow_mut().back();
        self.show_new_page();
    }

    fn retry(self: &Rc<Self>) {
        self.load_current(true);
    }

    fn submit_search(self: &Rc<Self>, text: String) {
        self.browser.borrow_mut().submit_search(&text);
        self.show_new_page();
    }

    fn select_filter(self: &Rc<Self>, filter: SearchFilter) {
        let changed = self.browser.borrow_mut().select_filter(filter);
        if changed {
            self.show_new_page();
        }
    }

    fn show_new_page(self: &Rc<Self>) {
        self.render();
        self.scroller.vadjustment().set_value(0.0);
        self.load_current(false);
    }

    fn load_current(self: &Rc<Self>, force: bool) {
        let key = self.browser.borrow().current_key();
        if let Some(key) = key {
            self.load(key, force);
        }
    }

    fn load(self: &Rc<Self>, key: CatalogKey, force: bool) {
        if self.client.is_none() {
            self.browser.borrow_mut().fail_offline(key);
            self.render();
            return;
        }
        let request = self.browser.borrow_mut().begin(&key, force);
        let Some((command, payload)) = request else {
            return;
        };
        self.render();
        self.send(command, payload, move |shell, answer| {
            shell.browser.borrow_mut().finish(key.clone(), answer);
            // An answer for a page the user has left is kept for when they come back, not drawn
            // over the page they are on now.
            let current = shell.browser.borrow().current_key();
            if current.as_ref() == Some(&key) {
                shell.render();
            }
        });
    }

    fn render(&self) {
        let (rows, searching) = {
            let browser = self.browser.borrow();
            (browser.rows(), browser.shows_search_bar())
        };
        ui::set_rows(&self.rows, rows);
        self.search_bar.set_visible(searching);
    }

    // MARK: playback

    /// Plays `track`, making `context` the queue when it is not empty.
    pub fn play(self: &Rc<Self>, track: Track, context: Rc<[Track]>) {
        let owner = {
            let player = self.player.borrow();
            if player.transition() != PlaybackTransition::Idle {
                Err(PENDING)
            } else if player.advertisement {
                Err(IN_ADVERTISEMENT)
            } else if player.lease.owner == Owner::LocalDownloadedFile {
                Err("A downloaded file is playing; stop it before playing from the catalog.")
            } else {
                Ok(player.lease.owner)
            }
        };
        let owner = match owner {
            Ok(owner) => owner,
            Err(reason) => return self.set_status(reason),
        };
        if self.client.is_none() {
            return self.set_status("Connect to the Rust service before playing.");
        }
        {
            let mut player = self.player.borrow_mut();
            if context.is_empty() {
                player.select(&track);
            } else {
                player.set_queue(context.to_vec(), &track);
            }
        }
        if owner == Owner::OfficialWebView {
            self.load_official(track);
        } else {
            self.claim_official(track);
        }
    }

    /// Asks Rust for the official player's lease; the page loads only once it is granted.
    fn claim_official(self: &Rc<Self>, track: Track) {
        let (token, generation) = {
            let mut player = self.player.borrow_mut();
            (
                player.begin_transition(PlaybackTransition::Claiming),
                player.lease.generation,
            )
        };
        self.set_status("Asking Rust for the official player…");
        let payload = RequestPayload {
            owner: Some(Owner::OfficialWebView),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("playback.claim", payload, move |shell, answer| {
            if !shell
                .player
                .borrow()
                .is_current(token, PlaybackTransition::Claiming)
            {
                return;
            }
            shell.player.borrow_mut().finish_transition(token);
            match answer {
                Ok(response) => {
                    shell.apply_lease(&response);
                    let granted = shell.player.borrow().lease.owner == Owner::OfficialWebView;
                    if granted {
                        shell.load_official(track);
                    } else {
                        shell.set_status("Rust did not grant the official player.");
                    }
                }
                Err(error) => {
                    let (_, message) = error.describe();
                    shell.set_status(&format!("Rust refused the official player: {message}"));
                }
            }
        });
    }

    /// Loads `track` into the official renderer under the lease already held.
    fn load_official(self: &Rc<Self>, track: Track) {
        let generation = {
            let mut player = self.player.borrow_mut();
            player.current = Some(track.clone());
            player.begin_track();
            player.begin_load();
            player.lease.generation
        };
        self.official.load(&track.video_id, generation);
        self.refresh_player();
    }

    /// A report from the official page that passed the bridge's checks.
    fn receive_official(self: &Rc<Self>, event: BridgeEvent) {
        let loaded = self.official.loaded_video_id();
        let (push, advance, sequence) = {
            let mut player = self.player.borrow_mut();
            let sample = ReportedSample {
                generation: event.generation,
                video_id: &event.video_id,
                current_time: event.current_time,
                duration: event.duration,
            };
            if !believes_sample(
                Owner::OfficialWebView,
                &player.lease,
                loaded.as_deref(),
                &sample,
            ) {
                return;
            }
            player.confirmed = true;
            player.paused = event.state != "playing";
            player.advertisement = event.is_advertisement;
            player.current_time = event.current_time;
            player.duration = event.duration;
            let sync = volume_sync(
                player.volume_applied_for_load,
                event.is_advertisement,
                event.volume,
                event.muted,
                player.volume,
                player.muted,
            );
            let push = match sync {
                VolumeSync::Ignore => None,
                VolumeSync::Follow { volume, muted } => {
                    player.volume = volume;
                    player.muted = muted;
                    None
                }
                VolumeSync::PushStored => {
                    player.volume_applied_for_load = true;
                    Some((player.volume, player.muted))
                }
                VolumeSync::AlreadyMatches => {
                    player.volume_applied_for_load = true;
                    None
                }
            };
            let settled = player
                .pending_seek
                .is_some_and(|seek| seek.is_settled_by(event.current_time, Instant::now()));
            if settled {
                player.pending_seek = None;
            }
            let advance = should_advance_after_end(
                &event.state,
                event.is_advertisement,
                &event.video_id,
                player.ended_video_id.as_deref(),
            );
            if advance {
                player.ended_video_id = Some(event.video_id.clone());
            }
            let title = player
                .current
                .as_ref()
                .map_or(event.video_id.clone(), |track| track.title.clone());
            player.status = if event.is_advertisement {
                "An advertisement is playing. Goosic reports it and never skips it.".to_owned()
            } else {
                match event.state.as_str() {
                    "playing" => format!("Playing {title}."),
                    "paused" => format!("Paused {title}."),
                    "ended" => format!("{title} ended."),
                    other => format!("The player reports {other}."),
                }
            };
            (push, advance, player.sample_sequence(event.sequence))
        };
        if let Some((volume, muted)) = push {
            self.official.set_volume(volume);
            self.official.set_muted(muted);
        }
        self.refresh_player();
        let payload = RequestPayload {
            owner: Some(Owner::OfficialWebView),
            generation: Some(event.generation),
            sequence: Some(sequence),
            marker: Some(
                if event.is_advertisement {
                    "advertisement"
                } else {
                    "audio"
                }
                .to_owned(),
            ),
            ..Default::default()
        };
        self.send("playback.sample", payload, |shell, answer| {
            // Rust stays authoritative for the lease; an acknowledgement only repeats it.
            if let Ok(response) = answer {
                shell.apply_lease(&response);
            }
        });
        if advance {
            self.advance_after_end();
        }
    }

    /// The official app followed its own queue. The requested track counts as finished, and
    /// Goosic's queue decides what plays, so the app never plays something nobody chose.
    fn official_moved_on(self: &Rc<Self>, finished: String) {
        let advance = {
            let mut player = self.player.borrow_mut();
            if player.lease.owner != Owner::OfficialWebView
                || player.ended_video_id.as_deref() == Some(finished.as_str())
            {
                false
            } else {
                player.ended_video_id = Some(finished);
                player.paused = true;
                player.confirmed = false;
                true
            }
        };
        if advance {
            self.refresh_player();
            self.advance_after_end();
        }
    }

    /// Moves to the next queued track when one finishes. Unlike Next this does not wrap: the end
    /// of the queue stops, so a single-track queue cannot loop forever on its own `ended` report.
    fn advance_after_end(self: &Rc<Self>) {
        let (autoplay, next) = {
            let player = self.player.borrow();
            let next = player
                .next_index(false)
                .and_then(|index| player.queue.get(index).cloned());
            (player.autoplay, next)
        };
        if !autoplay {
            return self.set_status("Track finished. Autoplay is off.");
        }
        match next {
            Some(track) => self.play(track, no_context()),
            None => self.extend_with_radio(),
        }
    }

    /// Continues past the end of the queue with the radio that follows the last track — what the
    /// previous Goosic called "auto radio".
    fn extend_with_radio(self: &Rc<Self>) {
        let seed = {
            let player = self.player.borrow();
            if player.lease.owner != Owner::OfficialWebView || player.radio_in_flight {
                None
            } else {
                player
                    .queued()
                    .cloned()
                    .or_else(|| player.current.clone())
                    .filter(|seed| player.radio_seed.as_deref() != Some(seed.video_id.as_str()))
            }
        };
        match seed {
            Some(seed) => self.begin_radio(seed, "Queue finished. Starting radio from"),
            None => self.set_status("Queue finished."),
        }
    }

    /// Replaces the queue with the radio that follows what is playing.
    fn start_radio(self: &Rc<Self>) {
        let seed = {
            let player = self.player.borrow();
            if player.radio_in_flight {
                None
            } else {
                player.current.clone()
            }
        };
        if let Some(seed) = seed {
            self.begin_radio(seed, "Starting radio from");
        }
    }

    fn begin_radio(self: &Rc<Self>, seed: Track, intro: &str) {
        {
            let mut player = self.player.borrow_mut();
            player.radio_in_flight = true;
            player.radio_seed = Some(seed.video_id.clone());
        }
        self.set_status(&format!("{intro} {}…", seed.title));
        let payload = RequestPayload {
            catalog_id: Some(seed.video_id.clone()),
            ..Default::default()
        };
        self.send("catalog.radio", payload, move |shell, answer| {
            shell.player.borrow_mut().radio_in_flight = false;
            let tracks = match answer {
                Ok(response) => response
                    .payload
                    .and_then(|payload| payload.catalog)
                    .map(|page| PageView::from_wire(&page).tracks)
                    .unwrap_or_default(),
                Err(error) => {
                    let (_, message) = error.describe();
                    return shell.set_status(&format!("Could not start radio: {message}"));
                }
            };
            let Some(first) = tracks.first().cloned() else {
                return shell.set_status("Radio had nothing to continue with.");
            };
            {
                let mut player = shell.player.borrow_mut();
                player.queue = std::iter::once(seed).chain(tracks).collect();
                player.index = 0;
            }
            shell.play(first, no_context());
        });
    }

    fn toggle_pause(self: &Rc<Self>) {
        let (idle, paused, queued) = {
            let player = self.player.borrow();
            (
                player.transition() == PlaybackTransition::Idle,
                player.paused,
                player.queued().cloned(),
            )
        };
        if !idle {
            return self.set_status(PENDING);
        }
        if self.official.loaded_video_id().is_some() {
            if paused {
                self.official.play();
                self.set_status("Play requested; waiting for the player to confirm.");
            } else {
                self.official.pause();
                self.set_status("Pause requested; waiting for the player to confirm.");
            }
            return;
        }
        match queued {
            Some(track) => self.play(track, no_context()),
            None => self.set_status("Choose a track to begin."),
        }
    }

    fn previous(self: &Rc<Self>) {
        let track = {
            let player = self.player.borrow();
            player
                .previous_index()
                .and_then(|index| player.queue.get(index).cloned())
        };
        if let Some(track) = track {
            self.play(track, no_context());
        }
    }

    fn next(self: &Rc<Self>) {
        // A deliberate Next wraps even with repeat off; only the end of a track stops.
        let track = {
            let player = self.player.borrow();
            player
                .next_index(true)
                .and_then(|index| player.queue.get(index).cloned())
        };
        if let Some(track) = track {
            self.play(track, no_context());
        }
    }

    fn seek(self: &Rc<Self>, position: f64) {
        let target = {
            let player = self.player.borrow();
            if player.advertisement {
                Err("Seeking is unavailable during advertisements.")
            } else if player.transition() != PlaybackTransition::Idle
                || !is_seekable(player.duration, player.lease.owner, player.advertisement)
            {
                Err("")
            } else {
                clamp_seek(position, player.duration).ok_or("")
            }
        };
        match target {
            Ok(target) => {
                self.player.borrow_mut().pending_seek = Some(PendingSeek {
                    position: target,
                    requested_at: Instant::now(),
                });
                self.official.seek(target);
                self.refresh_player();
            }
            Err(reason) if !reason.is_empty() => self.set_status(reason),
            // Put the slider back where the player really is.
            Err(_) => self.refresh_player(),
        }
    }

    fn set_volume(self: &Rc<Self>, volume: f64) {
        if self.player.borrow().advertisement {
            return self.set_status("Volume is unchanged during advertisements.");
        }
        let Some(volume) = clamp_volume(volume) else {
            return;
        };
        {
            let mut player = self.player.borrow_mut();
            player.volume = volume;
            player.muted = false;
            player.volume_applied_for_load = true;
        }
        self.official.set_volume(volume);
        self.save_preferences(PreferencesPatch {
            volume: Some(volume),
            muted: Some(false),
            ..Default::default()
        });
        self.refresh_player();
    }

    fn toggle_muted(self: &Rc<Self>) {
        let muted = {
            let mut player = self.player.borrow_mut();
            if player.advertisement {
                None
            } else {
                player.muted = !player.muted;
                player.volume_applied_for_load = true;
                Some(player.muted)
            }
        };
        let Some(muted) = muted else {
            return self.set_status("Mute is unavailable during advertisements.");
        };
        self.official.set_muted(muted);
        self.save_preferences(PreferencesPatch {
            muted: Some(muted),
            ..Default::default()
        });
        self.refresh_player();
    }

    fn toggle_shuffle(self: &Rc<Self>) {
        let shuffle = {
            let mut player = self.player.borrow_mut();
            player.shuffle = !player.shuffle;
            player.shuffle
        };
        self.save_preferences(PreferencesPatch {
            shuffle: Some(shuffle),
            ..Default::default()
        });
        self.refresh_player();
    }

    fn cycle_repeat(self: &Rc<Self>) {
        let repeat = {
            let mut player = self.player.borrow_mut();
            player.repeat = player.repeat.next();
            player.repeat
        };
        self.save_preferences(PreferencesPatch {
            repeat_mode: Some(repeat.raw_value().to_owned()),
            ..Default::default()
        });
        self.refresh_player();
    }

    fn toggle_autoplay(self: &Rc<Self>) {
        let autoplay = {
            let mut player = self.player.borrow_mut();
            player.autoplay = !player.autoplay;
            player.autoplay
        };
        self.save_preferences(PreferencesPatch {
            autoplay: Some(autoplay),
            ..Default::default()
        });
        self.refresh_player();
    }

    /// Stops playback and gives the lease back. The renderer is quiesced first and forgets its
    /// load's identity before Rust hears about it, so no late report can follow the release.
    fn release_playback(self: &Rc<Self>) {
        let start = {
            let mut player = self.player.borrow_mut();
            if player.transition() != PlaybackTransition::Idle {
                Err(PENDING)
            } else if player.lease.owner == Owner::None {
                Err("Nothing is playing.")
            } else {
                player.confirmed = false;
                Ok((
                    player.lease.owner,
                    player.begin_transition(PlaybackTransition::Releasing),
                ))
            }
        };
        let (owner, token) = match start {
            Ok(start) => start,
            Err(reason) => return self.set_status(reason),
        };
        self.set_status("Stopping playback…");
        let weak = Rc::downgrade(self);
        self.official.quiesce(move || {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if !shell
                .player
                .borrow()
                .is_current(token, PlaybackTransition::Releasing)
            {
                return;
            }
            shell.official.invalidate_expectations();
            let generation = shell.player.borrow().lease.generation;
            let payload = RequestPayload {
                owner: Some(owner),
                generation: Some(generation),
                ..Default::default()
            };
            shell.send("playback.release", payload, move |shell, answer| {
                if !shell
                    .player
                    .borrow()
                    .is_current(token, PlaybackTransition::Releasing)
                {
                    return;
                }
                let released = answer.is_ok();
                if let Ok(response) = &answer {
                    shell.apply_lease(response);
                }
                {
                    let mut player = shell.player.borrow_mut();
                    player.current = None;
                    player.begin_track();
                    player.finish_transition(token);
                }
                // A released page keeps reporting that it is paused; blanking it ends that.
                shell.official.detach(|| {});
                shell.set_status(if released {
                    "Playback stopped and released."
                } else {
                    "Rust did not confirm the release."
                });
            });
        });
    }

    fn host_status(self: &Rc<Self>, message: String) {
        let owned = self.player.borrow().lease.owner == Owner::OfficialWebView;
        if owned {
            self.set_status(&message);
        }
    }

    fn host_diagnostics(self: &Rc<Self>, report: String) {
        *self.diagnostics.borrow_mut() = report;
    }

    /// Records the lease a response carries, if it carries one.
    fn apply_lease(&self, response: &ResponseEnvelope) {
        let state = response
            .payload
            .as_ref()
            .and_then(|payload| payload.state.clone());
        if let Some(state) = state {
            self.player.borrow_mut().apply_lease(state);
            self.refresh_player();
        }
    }

    fn set_status(&self, message: &str) {
        self.player.borrow_mut().status = message.to_owned();
        self.refresh_player();
    }

    fn refresh_player(&self) {
        let loaded = self.official.loaded_video_id().is_some();
        let player = self.player.borrow();
        self.bar.update(&player, loaded);
    }
}

/// Turns a method into a callback that holds the shell weakly, so no widget's closure keeps the
/// shell alive on its own.
fn forward<A: 'static>(weak: &Weak<Shell>, method: fn(&Rc<Shell>, A)) -> impl Fn(A) + 'static {
    let weak = weak.clone();
    move |argument| {
        if let Some(shell) = weak.upgrade() {
            method(&shell, argument);
        }
    }
}

fn forward_unit(weak: &Weak<Shell>, method: fn(&Rc<Shell>)) -> impl Fn() + 'static {
    let weak = weak.clone();
    move || {
        if let Some(shell) = weak.upgrade() {
            method(&shell);
        }
    }
}
