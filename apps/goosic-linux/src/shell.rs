//! The shell's state for the life of the application, and the window it shows.
//!
//! The service connection belongs to the application rather than to a window, because closing the
//! window will not end the conversation once the shell plays in the background. Navigation lives
//! in a `Browser`, playback in a `Player` and lyrics in a `Lyrics`, all of which decide without
//! GTK; this file carries answers from the service and the renderer into them, and redraws.
//!
//! Every playback change goes through Rust first. The shell asks for the lease, and only once it is
//! granted does a renderer load anything; it releases the lease only after the renderer has been
//! quiesced, so there is no moment when a page plays that Rust does not know about.
//!
//! One discipline runs through the whole file: no host is called and nothing is redrawn while a
//! `RefCell` borrow is open. Hosts report back synchronously, and a report that finds the state
//! already borrowed is a panic rather than a bug report.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Instant;

use goosic_protocol::{
    AccountUpsert, AccountsSnapshot, DownloadedTrack, Owner, PreferencesPatch, RequestPayload,
    ResponseEnvelope, SettingsSnapshot,
};
use goosic_shell_support::bridge::BridgeEvent;
use goosic_shell_support::catalog::{PageView, Track};
use goosic_shell_support::login::{self, LoginResult};
use goosic_shell_support::navigation::{
    CatalogKey, EntityReference, PlaybackTransition, Route, SearchFilter, Theme,
};
use goosic_shell_support::playback::{
    believes_sample, clamp_seek, clamp_volume, is_seekable, merge_preferences,
    should_advance_after_end, volume_sync, PendingSeek, ReportedSample, VolumeSync,
    PREFERENCE_SAVE_DELAY,
};
use goosic_shell_support::{ServiceClient, TransportError};
use gtk::prelude::*;
use gtk::{gio, glib};
use uuid::Uuid;

use crate::artwork::ArtworkCache;
use crate::background::Background;
use crate::local_host::{LocalEvent, LocalHandlers, LocalHost};
use crate::login_host::{LoginHandlers, LoginHost};
use crate::lyrics::Lyrics;
use crate::mpris::{Mpris, MprisHandlers};
use crate::official_host::{self, OfficialHost};
use crate::pages::{Browser, DownloadsState, ShellFacts};
use crate::playback::Player;
use crate::player_bar::{BarActions, PlayerBar};
use crate::side_panels::{LyricsPanel, QueuePanel};
use crate::status_icon::{StatusIcon, TrayHandlers};
use crate::web_profile;
use crate::{bridge, service, theme, ui};

type Answer = Result<ResponseEnvelope, TransportError>;

const PENDING: &str = "A playback change is still being confirmed; try again in a moment.";
const IN_ADVERTISEMENT: &str =
    "Track changes are unavailable while the official player shows an advertisement.";
const ACCOUNT_CHANGING: &str = "Playback is unavailable while the account changes.";

pub struct Shell {
    pub window: gtk::ApplicationWindow,
    client: Option<ServiceClient>,
    browser: RefCell<Browser>,
    player: RefCell<Player>,
    lyrics: RefCell<Lyrics>,
    facts: RefCell<ShellFacts>,
    queue_visible: Cell<bool>,
    official: Rc<OfficialHost>,
    local: Rc<LocalHost>,
    mpris: Rc<Mpris>,
    background: Rc<Background>,
    status_icon: Rc<StatusIcon>,
    login: RefCell<Option<Rc<LoginHost>>>,
    /// The epoch of the accounts snapshot on screen, so a late answer cannot replace a newer one.
    account_epoch: Cell<u64>,
    accounts_read: Cell<bool>,
    rows: gio::ListStore,
    scroller: gtk::ScrolledWindow,
    search_bar: gtk::Box,
    routes: gtk::ListBox,
    bar: PlayerBar,
    side: gtk::Box,
    queue_panel: QueuePanel,
    lyrics_panel: LyricsPanel,
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
        web_profile::clear_abandoned_staging();
        let artwork = ArtworkCache::new();
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
                set_theme: Box::new(forward(weak, Shell::set_theme)),
                import_legacy: Box::new(forward_unit(weak, Shell::import_legacy_preferences)),
                load_more: Box::new(forward_unit(weak, Shell::load_more)),
                play_download: Box::new(forward(weak, Shell::play_download)),
                refresh_downloads: Box::new(forward_unit(weak, Shell::load_downloads)),
                import_downloads: Box::new(forward_unit(weak, Shell::import_downloads)),
                sign_in: Box::new(forward_unit(weak, Shell::sign_in)),
                switch_account: Box::new(forward(weak, Shell::switch_account)),
                sign_out: Box::new(forward_unit(weak, Shell::sign_out)),
                remove_account: Box::new(forward(weak, Shell::remove_account)),
                artwork: artwork.clone(),
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
            let local = LocalHost::new(LocalHandlers {
                on_event: Box::new(forward(weak, Shell::receive_local)),
                on_status: Box::new(forward(weak, Shell::local_status)),
            });
            let mpris = Mpris::start(MprisHandlers {
                snapshot: Box::new({
                    let weak = weak.clone();
                    move || match weak.upgrade() {
                        Some(shell) => shell.player.borrow().snapshot(),
                        None => Player::new().snapshot(),
                    }
                }),
                toggle_pause: Box::new(forward_unit(weak, Shell::toggle_pause)),
                next: Box::new(forward_unit(weak, Shell::next)),
                previous: Box::new(forward_unit(weak, Shell::previous)),
                stop: Box::new(forward_unit(weak, Shell::release_playback)),
                seek: Box::new(forward(weak, Shell::seek)),
                set_volume: Box::new(forward(weak, Shell::set_volume)),
                raise: Box::new(forward_unit(weak, Shell::raise)),
                quit: Box::new(forward_unit(weak, Shell::quit)),
            });
            let bar = PlayerBar::new(
                BarActions {
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
                    lyrics: Box::new(forward_unit(weak, Shell::toggle_lyrics)),
                    queue: Box::new(forward_unit(weak, Shell::toggle_queue)),
                },
                artwork.clone(),
            );
            let queue_panel = QueuePanel::new(forward(weak, Shell::play_queued));
            let lyrics_panel = LyricsPanel::new();

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

            let panels = gtk::Box::new(gtk::Orientation::Vertical, 0);
            panels.set_width_request(320);
            panels.append(&queue_panel.root);
            panels.append(&lyrics_panel.root);
            let side = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            side.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            side.append(&panels);
            side.set_visible(false);

            let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            page.set_vexpand(true);
            let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
            main.set_hexpand(true);
            main.append(&search_bar);
            main.append(&scroller);
            page.append(&main);
            page.append(&side);

            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_hexpand(true);
            content.append(&page);
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
            let background = Background::start(app, &window);
            let status_icon = StatusIcon::start(TrayHandlers {
                snapshot: Box::new({
                    let weak = weak.clone();
                    move || match weak.upgrade() {
                        Some(shell) => shell.player.borrow().snapshot(),
                        None => Player::new().snapshot(),
                    }
                }),
                show: Box::new(forward_unit(weak, Shell::raise)),
                toggle_pause: Box::new(forward_unit(weak, Shell::toggle_pause)),
                next: Box::new(forward_unit(weak, Shell::next)),
                previous: Box::new(forward_unit(weak, Shell::previous)),
                quit: Box::new(forward_unit(weak, Shell::quit)),
            });

            Shell {
                window,
                client,
                browser: RefCell::new(Browser::new()),
                player: RefCell::new(Player::new()),
                lyrics: RefCell::new(Lyrics::new()),
                facts: RefCell::new(ShellFacts::default()),
                queue_visible: Cell::new(false),
                official,
                local,
                mpris,
                background,
                status_icon,
                login: RefCell::new(None),
                account_epoch: Cell::new(0),
                accounts_read: Cell::new(false),
                rows,
                scroller,
                search_bar,
                routes: sidebar.routes,
                bar,
                side,
                queue_panel,
                lyrics_panel,
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
                    shell.facts.borrow_mut().connected = false;
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
                self.facts.borrow_mut().connected = true;
                self.connection.set_label("● Rust service connected");
                self.status.set_label("");
                self.load_preferences();
                self.load_accounts();
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
                    shell.adopt_settings(&settings);
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

    /// Takes on stored preferences, whether read at launch or imported from the previous Goosic.
    /// Where the app opens is left to the caller: an import must not move the user.
    fn adopt_settings(&self, settings: &SettingsSnapshot) {
        let (volume, muted) = {
            let mut player = self.player.borrow_mut();
            player.apply_settings(settings);
            (player.volume, player.muted)
        };
        self.local.set_volume(volume);
        self.local.set_muted(muted);
        let theme = Theme::named(&settings.theme);
        {
            let mut facts = self.facts.borrow_mut();
            facts.theme = theme;
            facts.legacy_imported = settings.imported_from_legacy;
            facts.legacy_available = settings.legacy_available;
        }
        theme::apply(theme);
        self.queue_visible.set(settings.queue_visible);
        self.sync_side_panels();
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

    pub fn set_theme(self: &Rc<Self>, theme: Theme) {
        self.facts.borrow_mut().theme = theme;
        theme::apply(theme);
        // Redrawn once the current signal has returned: the choice may come from the settings
        // page's own toggle, and rebuilding the page inside that toggle's signal would destroy it
        // mid-emission.
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(shell) = weak.upgrade() {
                shell.render();
            }
        });
        self.save_preferences(PreferencesPatch {
            theme: Some(theme.raw_value().to_owned()),
            ..Default::default()
        });
    }

    /// Reads the previous Goosic's preferences. The service reads them without changing them and
    /// never carries credentials over.
    fn import_legacy_preferences(self: &Rc<Self>) {
        if !self.facts.borrow().legacy_available {
            return self.set_status("No previous Goosic preferences were found on this machine.");
        }
        self.set_status("Importing preferences from the previous Goosic…");
        self.send(
            "settings.importLegacy",
            RequestPayload::default(),
            |shell, answer| match answer {
                Ok(response) => {
                    let payload = response.payload.unwrap_or_default();
                    if let Some(settings) = &payload.settings {
                        shell.adopt_settings(settings);
                    }
                    shell.render();
                    shell.set_status(
                        payload
                            .message
                            .as_deref()
                            .unwrap_or("Imported preferences from the previous Goosic."),
                    );
                }
                Err(error) => {
                    let (_, message) = error.describe();
                    shell.set_status(&format!("Could not import previous preferences: {message}"));
                }
            },
        );
    }

    // MARK: navigation

    pub fn navigate(self: &Rc<Self>, route: Route) {
        self.browser.borrow_mut().navigate(route);
        // Selecting a row does not activate it, so this cannot come back here.
        ui::select_route(&self.routes, route);
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
        if self.browser.borrow().route() == Route::Downloads {
            return self.load_downloads();
        }
        self.load_current(true);
    }

    /// Fetches the next part of the page on screen, if it continues and is not fetching it already.
    fn load_more(self: &Rc<Self>) {
        let Some(key) = self.browser.borrow().current_key() else {
            return;
        };
        let request = self.browser.borrow_mut().begin_more(&key);
        let Some((cursor, payload)) = request else {
            return;
        };
        self.render();
        self.send("catalog.continue", payload, move |shell, answer| {
            shell
                .browser
                .borrow_mut()
                .finish_more(key.clone(), &cursor, answer);
            let current = shell.browser.borrow().current_key();
            if current.as_ref() == Some(&key) {
                shell.render();
            }
        });
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
        let downloads_unread = self.browser.borrow().route() == Route::Downloads
            && self.facts.borrow().downloads == DownloadsState::NotRead;
        if downloads_unread {
            self.load_downloads();
        }
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
        let facts = self.facts.borrow().clone();
        let (rows, searching) = {
            let browser = self.browser.borrow();
            (browser.rows(&facts), browser.shows_search_bar())
        };
        ui::set_rows(&self.rows, rows);
        self.search_bar.set_visible(searching);
    }

    // MARK: playback

    /// Plays `track`, making `context` the queue when it is not empty.
    pub fn play(self: &Rc<Self>, track: Track, context: Rc<[Track]>) {
        let owner = {
            let player = self.player.borrow();
            if self.facts.borrow().account_busy {
                Err(ACCOUNT_CHANGING)
            } else if player.transition() != PlaybackTransition::Idle {
                Err(PENDING)
            } else if player.advertisement {
                Err(IN_ADVERTISEMENT)
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
        match owner {
            Owner::OfficialWebView => self.load_official(track),
            // The downloaded file stops and its lease goes back before the page may claim.
            Owner::LocalDownloadedFile => {
                self.leave_local_then(move |shell| shell.claim_official(track))
            }
            _ => self.claim_official(track),
        }
    }

    /// Plays the queue from `index`, for a row chosen in the queue panel.
    fn play_queued(self: &Rc<Self>, index: usize) {
        let track = self.player.borrow().queue.get(index).cloned();
        if let Some(track) = track {
            self.play(track, no_context());
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
        let (generation, changed) = {
            let mut player = self.player.borrow_mut();
            let changed =
                player.current.as_ref().map(|current| &current.video_id) != Some(&track.video_id);
            player.current = Some(track.clone());
            player.begin_track();
            player.begin_load();
            (player.lease.generation, changed)
        };
        self.official.load(&track.video_id, generation);
        if changed {
            self.lyrics.borrow_mut().track_changed();
            self.load_lyrics();
        }
        self.refresh_player();
    }

    /// A report from the official page that passed the bridge's checks.
    fn receive_official(self: &Rc<Self>, event: BridgeEvent) {
        let loaded = self.official.loaded_video_id();
        let (push, advance, sequence, nudge) = {
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
            // The page's own autoplay is not guaranteed: on a fresh profile YouTube Music loads
            // the track paused. The user asked for this track to play, so the load's first paused
            // report is answered with one play request — once, so a pause the user makes is kept.
            let nudge = player.start_pending && !event.is_advertisement && event.state == "paused";
            if nudge || event.state == "playing" {
                player.start_pending = false;
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
            (push, advance, player.sample_sequence(event.sequence), nudge)
        };
        if let Some((volume, muted)) = push {
            self.official.set_volume(volume);
            self.official.set_muted(muted);
        }
        if nudge {
            self.official.play();
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
        let (station, seed) = {
            let player = self.player.borrow();
            if player.lease.owner != Owner::OfficialWebView || player.radio_in_flight {
                (None, None)
            } else {
                let station = player.radio_seed.clone().zip(player.radio_cursor.clone());
                let seed = player
                    .queued()
                    .cloned()
                    .or_else(|| player.current.clone())
                    .filter(|seed| player.radio_seed.as_deref() != Some(seed.video_id.as_str()));
                (station, seed)
            }
        };
        if let Some((seed, cursor)) = station {
            return self.continue_radio(seed, cursor);
        }
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
            let still_wanted = {
                let mut player = shell.player.borrow_mut();
                player.radio_in_flight = false;
                // A list chosen while this was in flight cleared the seed; it is not replaced.
                player.radio_seed.as_deref() == Some(seed.video_id.as_str())
            };
            if !still_wanted {
                return;
            }
            let page = match answer {
                Ok(response) => response.payload.and_then(|payload| payload.catalog),
                Err(error) => {
                    let (_, message) = error.describe();
                    return shell.set_status(&format!("Could not start radio: {message}"));
                }
            };
            let cursor = page.as_ref().and_then(|page| page.next_cursor.clone());
            let tracks = page
                .map(|page| PageView::from_wire(&page).tracks)
                .unwrap_or_default();
            let Some(first) = tracks.first().cloned() else {
                return shell.set_status("Radio had nothing to continue with.");
            };
            {
                let mut player = shell.player.borrow_mut();
                player.queue = std::iter::once(seed).chain(tracks).collect();
                player.index = 0;
                player.radio_cursor = cursor;
            }
            shell.play(first, no_context());
        });
    }

    /// Asks the station that is playing for its next part and appends it, so a radio keeps its
    /// character rather than drifting through a new station seeded by its own last track.
    fn continue_radio(self: &Rc<Self>, seed: String, cursor: String) {
        self.player.borrow_mut().radio_in_flight = true;
        self.set_status("Queue finished. Continuing the radio…");
        let payload = RequestPayload {
            catalog_id: Some(seed.clone()),
            continuation: Some(cursor.clone()),
            ..Default::default()
        };
        self.send("catalog.radio", payload, move |shell, answer| {
            let same_station = {
                let mut player = shell.player.borrow_mut();
                player.radio_in_flight = false;
                player.radio_seed.as_deref() == Some(seed.as_str())
                    && player.radio_cursor.as_deref() == Some(cursor.as_str())
            };
            if !same_station {
                return;
            }
            let page = match answer {
                Ok(response) => response.payload.and_then(|payload| payload.catalog),
                Err(error) => {
                    shell.player.borrow_mut().radio_cursor = None;
                    let (code, message) = error.describe();
                    return shell.set_status(&if code == "catalogEmpty" {
                        "The radio has nothing more to play.".to_owned()
                    } else {
                        format!("Could not continue the radio: {message}")
                    });
                }
            };
            let next_cursor = page.as_ref().and_then(|page| page.next_cursor.clone());
            let first = {
                let mut player = shell.player.borrow_mut();
                // A station repeats itself across parts; a track already queued is not queued twice.
                let fresh: Vec<Track> = page
                    .map(|page| PageView::from_wire(&page).tracks)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|track| {
                        !player
                            .queue
                            .iter()
                            .any(|queued| queued.video_id == track.video_id)
                    })
                    .collect();
                player.radio_cursor = next_cursor;
                let first = fresh.first().cloned();
                player.queue.extend(fresh);
                first
            };
            match first {
                Some(track) => shell.play(track, no_context()),
                None => shell.set_status("The radio has nothing more to play."),
            }
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
        let local_owns = self.player.borrow().lease.owner == Owner::LocalDownloadedFile;
        if local_owns && self.local.is_loaded() {
            if paused {
                self.local.play();
            } else {
                self.local.pause();
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
                let owner = {
                    let mut player = self.player.borrow_mut();
                    player.pending_seek = Some(PendingSeek {
                        position: target,
                        requested_at: Instant::now(),
                    });
                    player.lease.owner
                };
                if owner == Owner::LocalDownloadedFile {
                    self.local.seek(target);
                } else {
                    self.official.seek(target);
                }
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
        self.local.set_volume(volume);
        self.local.set_muted(false);
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
        self.local.set_muted(muted);
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
        if owner == Owner::LocalDownloadedFile {
            // GStreamer stops synchronously, so there is nothing to wait for before telling Rust.
            self.local.stop();
            return self.send_release(owner, token);
        }
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
            shell.send_release(owner, token);
        });
    }

    fn send_release(self: &Rc<Self>, owner: Owner, token: u64) {
        let generation = self.player.borrow().lease.generation;
        let payload = RequestPayload {
            owner: Some(owner),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("playback.release", payload, move |shell, answer| {
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
                player.start_pending = false;
                player.finish_transition(token);
            }
            shell.lyrics.borrow_mut().track_changed();
            shell.load_lyrics();
            if owner == Owner::OfficialWebView {
                // A released page keeps reporting that it is paused; blanking it ends that.
                shell.official.detach(|| {});
            }
            shell.set_status(if released {
                "Playback stopped and released."
            } else {
                "Rust did not confirm the release."
            });
        });
    }

    /// Quiesces the official page and gives its lease back, then carries on with `then`.
    fn leave_official_then(self: &Rc<Self>, then: impl FnOnce(&Rc<Shell>) + 'static) {
        let token = {
            let mut player = self.player.borrow_mut();
            player.confirmed = false;
            player.begin_transition(PlaybackTransition::Releasing)
        };
        self.set_status("Stopping the official player first…");
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
                owner: Some(Owner::OfficialWebView),
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
                shell.player.borrow_mut().finish_transition(token);
                match answer {
                    Ok(response) => {
                        shell.apply_lease(&response);
                        shell.official.detach(|| {});
                        then(shell);
                    }
                    Err(error) => {
                        let (_, message) = error.describe();
                        shell.set_status(&format!(
                            "Rust did not release the official player: {message}"
                        ));
                    }
                }
            });
        });
    }

    /// Stops the downloaded file and gives its lease back, then carries on with `then`.
    fn leave_local_then(self: &Rc<Self>, then: impl FnOnce(&Rc<Shell>) + 'static) {
        self.local.stop();
        let (token, generation) = {
            let mut player = self.player.borrow_mut();
            player.confirmed = false;
            (
                player.begin_transition(PlaybackTransition::Releasing),
                player.lease.generation,
            )
        };
        self.set_status("Stopping the downloaded file first…");
        let payload = RequestPayload {
            owner: Some(Owner::LocalDownloadedFile),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("playback.release", payload, move |shell, answer| {
            if !shell
                .player
                .borrow()
                .is_current(token, PlaybackTransition::Releasing)
            {
                return;
            }
            shell.player.borrow_mut().finish_transition(token);
            match answer {
                Ok(response) => {
                    shell.apply_lease(&response);
                    then(shell);
                }
                Err(error) => {
                    let (_, message) = error.describe();
                    shell.set_status(&format!(
                        "Rust did not release the downloaded file: {message}"
                    ));
                }
            }
        });
    }

    // MARK: downloads

    /// Reads the downloaded files the service knows about.
    fn load_downloads(self: &Rc<Self>) {
        if self.facts.borrow().downloads_busy {
            return;
        }
        self.facts.borrow_mut().downloads_busy = true;
        self.render();
        self.send(
            "downloads.list",
            RequestPayload::default(),
            |shell, answer| {
                {
                    let mut facts = shell.facts.borrow_mut();
                    facts.downloads_busy = false;
                    facts.downloads = match answer {
                        Ok(response) => DownloadsState::Read(
                            response
                                .payload
                                .and_then(|payload| payload.downloads)
                                .unwrap_or_default(),
                        ),
                        Err(error) => DownloadsState::Failed(error.describe().1),
                    };
                }
                shell.render();
            },
        );
    }

    /// Imports the finalized files a previous Goosic left on disk. The service references them
    /// where they are and never starts a downloader.
    fn import_downloads(self: &Rc<Self>) {
        if self.facts.borrow().downloads_busy {
            return;
        }
        self.facts.borrow_mut().downloads_busy = true;
        self.render();
        self.set_status("Reading finalized files from the previous Goosic…");
        self.send(
            "downloads.importLegacy",
            RequestPayload::default(),
            |shell, answer| {
                let message = match answer {
                    Ok(response) => {
                        let payload = response.payload.unwrap_or_default();
                        if let Some(tracks) = payload.downloads {
                            shell.facts.borrow_mut().downloads = DownloadsState::Read(tracks);
                        }
                        payload.message.unwrap_or_else(|| {
                            "Imported downloaded files from the previous Goosic.".to_owned()
                        })
                    }
                    Err(error) => {
                        let (code, message) = error.describe();
                        if code == "legacyNotFound" {
                            "No previous Goosic downloads were found on this machine.".to_owned()
                        } else {
                            format!("Could not import downloaded files: {message}")
                        }
                    }
                };
                shell.facts.borrow_mut().downloads_busy = false;
                shell.render();
                shell.set_status(&message);
            },
        );
    }

    /// Plays a downloaded file: Rust grants the local lease, the service returns the decoded file,
    /// and only then does GStreamer open it.
    pub fn play_download(self: &Rc<Self>, track: DownloadedTrack) {
        let owner = {
            let player = self.player.borrow();
            if self.facts.borrow().account_busy {
                Err(ACCOUNT_CHANGING)
            } else if !track.available {
                Err("This downloaded file is missing from disk. Refresh Downloads to check again.")
            } else if player.advertisement {
                Err("A downloaded file cannot start while the official player shows an advertisement.")
            } else if player.transition() != PlaybackTransition::Idle {
                Err(PENDING)
            } else {
                Ok(player.lease.owner)
            }
        };
        let owner = match owner {
            Ok(owner) => owner,
            Err(reason) => return self.set_status(reason),
        };
        if self.client.is_none() {
            return self
                .set_status("Connect to the Rust service before playing a downloaded file.");
        }
        match owner {
            Owner::LocalDownloadedFile => {
                // One file replacing another keeps the lease, but the old file stops first.
                self.local.stop_for_replacement();
                let token = self
                    .player
                    .borrow_mut()
                    .begin_transition(PlaybackTransition::PreparingLocal);
                self.prepare_local(track, token);
            }
            Owner::OfficialWebView => {
                self.leave_official_then(move |shell| shell.claim_local(track))
            }
            _ => self.claim_local(track),
        }
    }

    fn claim_local(self: &Rc<Self>, track: DownloadedTrack) {
        let (token, generation) = {
            let mut player = self.player.borrow_mut();
            (
                player.begin_transition(PlaybackTransition::Claiming),
                player.lease.generation,
            )
        };
        self.set_status("Asking Rust for local playback…");
        let payload = RequestPayload {
            owner: Some(Owner::LocalDownloadedFile),
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
                    let granted = shell.player.borrow().lease.owner == Owner::LocalDownloadedFile;
                    if granted {
                        let token = shell
                            .player
                            .borrow_mut()
                            .begin_transition(PlaybackTransition::PreparingLocal);
                        shell.prepare_local(track, token);
                    } else {
                        shell.set_status("Rust did not grant local playback.");
                    }
                }
                Err(error) => {
                    let (_, message) = error.describe();
                    shell.set_status(&format!("Rust refused local playback: {message}"));
                }
            }
        });
    }

    fn prepare_local(self: &Rc<Self>, track: DownloadedTrack, token: u64) {
        let generation = self.player.borrow().lease.generation;
        self.set_status(&format!("Preparing {}…", track.title));
        let payload = RequestPayload {
            owner: Some(Owner::LocalDownloadedFile),
            generation: Some(generation),
            catalog_id: Some(track.video_id.clone()),
            ..Default::default()
        };
        self.send("downloads.prepare", payload, move |shell, answer| {
            if !shell
                .player
                .borrow()
                .is_current(token, PlaybackTransition::PreparingLocal)
            {
                return;
            }
            let path = match answer {
                Ok(response) => response
                    .payload
                    .and_then(|payload| payload.local_file)
                    .filter(|path| !path.is_empty()),
                Err(error) => {
                    let (_, message) = error.describe();
                    return shell.fail_local(
                        token,
                        format!("Could not prepare the downloaded file: {message}"),
                    );
                }
            };
            let Some(path) = path else {
                return shell.fail_local(token, "Rust did not return a decoded file.".to_owned());
            };
            let (generation, volume, muted) = {
                let player = shell.player.borrow();
                (player.lease.generation, player.volume, player.muted)
            };
            shell.local.set_volume(volume);
            shell.local.set_muted(muted);
            if let Err(reason) = shell
                .local
                .prepare(Path::new(&path), &track.video_id, generation)
            {
                return shell.fail_local(token, reason);
            }
            {
                let mut player = shell.player.borrow_mut();
                // A downloaded file is its own listening context, not a place in a catalog list.
                player.queue.clear();
                player.index = 0;
                player.radio_seed = None;
                player.radio_cursor = None;
                player.current = Some(downloaded_track(&track));
                player.begin_track();
                // Started deliberately just below; there is no page autoplay to make up for.
                player.start_pending = false;
            }
            if !shell.local.play() {
                return shell.fail_local(token, "GStreamer did not start the file.".to_owned());
            }
            shell.player.borrow_mut().finish_transition(token);
            shell.lyrics.borrow_mut().track_changed();
            shell.load_lyrics();
            shell.set_status(&format!("Playing {} from disk.", track.title));
        });
    }

    /// Stops the local renderer and gives the lease back, so a file that could not play never
    /// holds the speakers in Rust's eyes.
    fn fail_local(self: &Rc<Self>, token: u64, message: String) {
        if !self
            .player
            .borrow()
            .is_current(token, PlaybackTransition::PreparingLocal)
        {
            return;
        }
        self.local.stop();
        let generation = {
            let mut player = self.player.borrow_mut();
            player.current = None;
            player.begin_track();
            player.start_pending = false;
            player.lease.generation
        };
        let payload = RequestPayload {
            owner: Some(Owner::LocalDownloadedFile),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("playback.release", payload, move |shell, answer| {
            if let Ok(response) = &answer {
                shell.apply_lease(response);
            }
            shell.player.borrow_mut().finish_transition(token);
            shell.set_status(&message);
        });
    }

    /// A report from GStreamer about the downloaded file.
    fn receive_local(self: &Rc<Self>, event: LocalEvent) {
        let loaded = self.local.loaded_video_id();
        let advance = {
            let mut player = self.player.borrow_mut();
            let sample = ReportedSample {
                generation: event.generation,
                video_id: &event.video_id,
                current_time: event.current_time,
                duration: event.duration,
            };
            if !believes_sample(
                Owner::LocalDownloadedFile,
                &player.lease,
                loaded.as_deref(),
                &sample,
            ) {
                return;
            }
            player.confirmed = true;
            player.paused = event.state != "playing";
            player.advertisement = false;
            player.current_time = event.current_time;
            player.duration = event.duration;
            let settled = player
                .pending_seek
                .is_some_and(|seek| seek.is_settled_by(event.current_time, Instant::now()));
            if settled {
                player.pending_seek = None;
            }
            let advance = should_advance_after_end(
                event.state,
                false,
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
            player.status = match event.state {
                "playing" => format!("Playing {title} from disk."),
                "paused" => format!("Paused {title}."),
                _ => format!("{title} ended."),
            };
            advance
        };
        self.refresh_player();
        let payload = RequestPayload {
            owner: Some(Owner::LocalDownloadedFile),
            generation: Some(event.generation),
            sequence: Some(event.sequence),
            marker: Some("audio".to_owned()),
            ..Default::default()
        };
        self.send("playback.sample", payload, |shell, answer| {
            if let Ok(response) = answer {
                shell.apply_lease(&response);
            }
        });
        if advance {
            self.advance_after_end();
        }
    }

    fn local_status(self: &Rc<Self>, message: String) {
        let owned = self.player.borrow().lease.owner == Owner::LocalDownloadedFile;
        if owned {
            self.set_status(&message);
        }
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

    // MARK: accounts

    fn load_accounts(self: &Rc<Self>) {
        self.send(
            "accounts.get",
            RequestPayload::default(),
            |shell, answer| match answer {
                Ok(response) => {
                    if let Some(snapshot) = response.payload.and_then(|payload| payload.accounts) {
                        let initial = !shell.accounts_read.get();
                        shell.apply_accounts(snapshot, initial);
                    }
                }
                Err(error) => {
                    let (_, message) = error.describe();
                    shell.set_status(&format!("Could not read accounts: {message}"));
                }
            },
        );
    }

    /// Takes a snapshot of the accounts unless a newer one is already shown, so a late answer
    /// cannot bring back an account that was just removed.
    fn apply_accounts(self: &Rc<Self>, snapshot: AccountsSnapshot, initial: bool) {
        if !login::accepts_snapshot_epoch(snapshot.epoch, self.account_epoch.get(), initial) {
            return;
        }
        self.account_epoch.set(snapshot.epoch);
        self.accounts_read.set(true);
        let profile = {
            let mut facts = self.facts.borrow_mut();
            let active = login::active_account(&snapshot);
            facts.account = active.map(|account| account.display_name.clone());
            facts.active_account_id = active.map(|account| account.id.clone());
            let profile =
                active.and_then(|account| Uuid::parse_str(&account.webkit_profile_id).ok());
            facts.accounts = snapshot.accounts.clone();
            profile
        };
        self.render_soon();
        // An account remembered from the last run is where this run starts, not a change to make:
        // nothing plays yet, so its profile is bound directly.
        if initial && self.player.borrow().lease.owner == Owner::None {
            self.official
                .bind_profile(profile.unwrap_or(goosic_shell_support::bridge::GUEST_PROFILE_ID));
        }
    }

    /// The web profile of `account_id`, or the guest's.
    fn profile_for(&self, account_id: Option<&str>) -> Uuid {
        let facts = self.facts.borrow();
        account_id
            .and_then(|id| facts.accounts.iter().find(|account| account.id == id))
            .and_then(|account| Uuid::parse_str(&account.webkit_profile_id).ok())
            .unwrap_or(goosic_shell_support::bridge::GUEST_PROFILE_ID)
    }

    fn active_profile(&self) -> Uuid {
        let active = self.facts.borrow().active_account_id.clone();
        self.profile_for(active.as_deref())
    }

    /// Opens Google's sign-in in a window of its own, once playback has let go.
    pub fn sign_in(self: &Rc<Self>) {
        if !self.begin_account_operation() {
            return;
        }
        self.prepare_for_account_change(
            |shell| shell.open_sign_in(),
            |shell, message| shell.end_account_operation(&message),
        );
    }

    pub fn switch_account(self: &Rc<Self>, id: String) {
        let switchable = {
            let facts = self.facts.borrow();
            facts.accounts.iter().any(|account| account.id == id)
                && facts.active_account_id.as_deref() != Some(id.as_str())
        };
        if !switchable || !self.begin_account_operation() {
            return;
        }
        self.change_account(Some(id.clone()), Some(id), None);
    }

    pub fn sign_out(self: &Rc<Self>) {
        let signed_in = self.facts.borrow().active_account_id.is_some();
        if !signed_in || !self.begin_account_operation() {
            return;
        }
        self.change_account(None, None, None);
    }

    pub fn remove_account(self: &Rc<Self>, id: String) {
        let (known, target) = {
            let facts = self.facts.borrow();
            let known = facts.accounts.iter().any(|account| account.id == id);
            let active = facts.active_account_id.clone();
            let target = if active.as_deref() == Some(id.as_str()) {
                None
            } else {
                active
            };
            (known, target)
        };
        if !known {
            return;
        }
        let removed = self.profile_for(Some(&id));
        if !self.begin_account_operation() {
            return;
        }
        self.change_account(Some(id), target, Some(removed));
    }

    fn begin_account_operation(self: &Rc<Self>) -> bool {
        let busy = self.facts.borrow().account_busy;
        let (advertisement, transition) = {
            let player = self.player.borrow();
            (player.advertisement, player.transition())
        };
        let refusal = if !login::can_interact(busy) {
            Some("An account change is already in progress.")
        } else if advertisement {
            Some("Accounts cannot change during an advertisement.")
        } else if !login::can_start_account_transition(advertisement, transition) {
            Some(PENDING)
        } else if self.client.is_none() {
            Some("Connect to the Rust service before changing accounts.")
        } else {
            None
        };
        if let Some(refusal) = refusal {
            self.set_status(refusal);
            return false;
        }
        self.facts.borrow_mut().account_busy = true;
        self.render_soon();
        true
    }

    fn end_account_operation(self: &Rc<Self>, message: &str) {
        self.facts.borrow_mut().account_busy = false;
        self.render_soon();
        self.set_status(message);
    }

    /// Stops whatever plays, gives its lease back and blanks the page before an account changes,
    /// so no renderer is bound to one account while Rust already answers for another.
    fn prepare_for_account_change(
        self: &Rc<Self>,
        then: impl FnOnce(&Rc<Shell>) + 'static,
        fail: impl FnOnce(&Rc<Shell>, String) + 'static,
    ) {
        let owner = self.player.borrow().lease.owner;
        if owner == Owner::None {
            let weak = Rc::downgrade(self);
            self.official.detach(move || {
                if let Some(shell) = weak.upgrade() {
                    then(&shell);
                }
            });
            return;
        }
        let token = {
            let mut player = self.player.borrow_mut();
            player.confirmed = false;
            player.begin_transition(PlaybackTransition::Releasing)
        };
        self.set_status("Stopping playback before the account changes…");
        let weak = Rc::downgrade(self);
        let release = move || {
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
            if owner == Owner::LocalDownloadedFile {
                shell.local.stop();
            } else {
                shell.official.invalidate_expectations();
            }
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
                match answer {
                    Ok(response) => {
                        shell.apply_lease(&response);
                        let weak = Rc::downgrade(shell);
                        shell.official.detach(move || {
                            if let Some(shell) = weak.upgrade() {
                                shell.player.borrow_mut().finish_transition(token);
                                then(&shell);
                            }
                        });
                    }
                    Err(error) => {
                        shell.player.borrow_mut().finish_transition(token);
                        let (_, message) = error.describe();
                        fail(
                            shell,
                            format!("Could not stop playback for the account change: {message}"),
                        );
                    }
                }
            });
        };
        if owner == Owner::OfficialWebView {
            self.official.quiesce(release);
        } else {
            release();
        }
    }

    fn open_sign_in(self: &Rc<Self>) {
        self.clear_account_scoped();
        let (completed, cancelled) = (Rc::downgrade(self), Rc::downgrade(self));
        let host = LoginHost::open(
            &self.window,
            LoginHandlers {
                on_completed: Box::new(move |result| {
                    if let Some(shell) = completed.upgrade() {
                        shell.finish_sign_in(result);
                    }
                }),
                on_cancelled: Box::new(move || {
                    if let Some(shell) = cancelled.upgrade() {
                        shell.login.borrow_mut().take();
                        shell.end_account_operation("Sign-in cancelled.");
                    }
                }),
            },
        );
        *self.login.borrow_mut() = Some(host);
        self.set_status("Sign in with Google in the window that opened.");
    }

    /// Stores the account Rust will know. Nothing about the sign-in is promoted yet.
    fn finish_sign_in(self: &Rc<Self>, result: LoginResult) {
        self.login.borrow_mut().take();
        let upsert = AccountUpsert {
            id: Some(result.account_id.hyphenated().to_string()),
            webkit_profile_id: result.profile_id.hyphenated().to_string(),
            display_name: result.summary.display_name.clone(),
            email: result.summary.email.clone(),
            channel: result.summary.channel.clone(),
            avatar_url: result.summary.avatar_url.clone(),
        };
        self.set_status("Saving the account…");
        let payload = RequestPayload {
            account: Some(upsert),
            ..Default::default()
        };
        self.send(
            "accounts.upsert",
            payload,
            move |shell, answer| match answer {
                Ok(response) => {
                    if let Some(snapshot) = response.payload.and_then(|payload| payload.accounts) {
                        shell.apply_accounts(snapshot, false);
                    }
                    shell.activate_signed_in(result);
                }
                Err(error) => {
                    web_profile::discard_staging(result.profile_id);
                    let (_, message) = error.describe();
                    shell.end_account_operation(&format!("Could not save the account: {message}"));
                }
            },
        );
    }

    /// Activates the stored account, then makes the staged sign-in its profile. Only when all three
    /// have happened is anything kept; otherwise the account is removed again and the staging
    /// deleted, so a failure never leaves half an account behind.
    fn activate_signed_in(self: &Rc<Self>, result: LoginResult) {
        let account_id = result.account_id.hyphenated().to_string();
        let (token, generation) = {
            let mut player = self.player.borrow_mut();
            (
                player.begin_transition(PlaybackTransition::Releasing),
                player.lease.generation,
            )
        };
        let payload = RequestPayload {
            account_id: Some(account_id.clone()),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("accounts.activate", payload, move |shell, answer| {
            if !shell
                .player
                .borrow()
                .is_current(token, PlaybackTransition::Releasing)
            {
                return;
            }
            let activated = match answer {
                Ok(response) => {
                    shell.apply_lease(&response);
                    if let Some(snapshot) = response.payload.and_then(|payload| payload.accounts) {
                        shell.apply_accounts(snapshot, false);
                    }
                    Ok(())
                }
                Err(error) => Err(error.describe().1),
            };
            let promoted =
                activated.is_ok() && web_profile::promote_staging(result.profile_id).is_ok();
            if login::can_commit_staging(true, activated.is_ok(), promoted) {
                shell.official.bind_profile(result.profile_id);
                shell.player.borrow_mut().finish_transition(token);
                shell.end_account_operation(&format!(
                    "Signed in as {}.",
                    result.summary.display_name
                ));
                return;
            }
            let message = match activated {
                Err(message) => format!("Rust did not activate the account: {message}"),
                Ok(()) => "The sign-in could not be kept on disk.".to_owned(),
            };
            shell.roll_back_sign_in(account_id.clone(), result.profile_id, token, message);
        });
    }

    fn roll_back_sign_in(
        self: &Rc<Self>,
        account_id: String,
        profile: Uuid,
        token: u64,
        message: String,
    ) {
        web_profile::discard_staging(profile);
        let generation = self.player.borrow().lease.generation;
        let payload = RequestPayload {
            account_id: Some(account_id),
            generation: Some(generation),
            ..Default::default()
        };
        self.send("accounts.remove", payload, move |shell, answer| {
            let message = match answer {
                Ok(response) => {
                    shell.apply_lease(&response);
                    if let Some(snapshot) = response.payload.and_then(|payload| payload.accounts) {
                        shell.apply_accounts(snapshot, false);
                    }
                    message
                }
                Err(_) => format!(
                    "{message} Removing the half-made account also failed; reopen Settings \
                     before trying again."
                ),
            };
            shell.official.bind_profile(shell.active_profile());
            shell.player.borrow_mut().finish_transition(token);
            shell.end_account_operation(&message);
        });
    }

    /// Switches, signs out or removes. The page is rebound only once Rust has confirmed the change,
    /// so a refused change leaves the previous profile as it was.
    fn change_account(
        self: &Rc<Self>,
        account_id: Option<String>,
        target: Option<String>,
        removed: Option<Uuid>,
    ) {
        let command = if removed.is_some() {
            "accounts.remove"
        } else {
            login::activation_command(target.as_deref())
        };
        self.prepare_for_account_change(
            move |shell| {
                shell.clear_account_scoped();
                let (token, generation) = {
                    let mut player = shell.player.borrow_mut();
                    (
                        player.begin_transition(PlaybackTransition::Releasing),
                        player.lease.generation,
                    )
                };
                shell.set_status("Changing account…");
                let payload = RequestPayload {
                    account_id,
                    generation: Some(generation),
                    ..Default::default()
                };
                shell.send(command, payload, move |shell, answer| {
                    if !shell
                        .player
                        .borrow()
                        .is_current(token, PlaybackTransition::Releasing)
                    {
                        return;
                    }
                    shell.player.borrow_mut().finish_transition(token);
                    match answer {
                        Ok(response) => {
                            shell.apply_lease(&response);
                            match response.payload.and_then(|payload| payload.accounts) {
                                Some(snapshot) => shell.apply_accounts(snapshot, false),
                                None => shell.load_accounts(),
                            }
                            shell
                                .official
                                .bind_profile(shell.profile_for(target.as_deref()));
                            // Deleted only after the page has let go of it.
                            if let Some(profile) = removed {
                                web_profile::delete_profile(profile);
                            }
                            shell.end_account_operation(if removed.is_some() {
                                "Account removed from this machine."
                            } else if target.is_none() {
                                "Signed out."
                            } else {
                                "Active account changed."
                            });
                        }
                        Err(error) => {
                            let (_, message) = error.describe();
                            shell.end_account_operation(&format!(
                                "The account change failed: {message}"
                            ));
                        }
                    }
                });
            },
            |shell, message| shell.end_account_operation(&message),
        );
    }

    /// Forgets what played under the previous account, so nothing from one account is shown or
    /// resumed under another.
    fn clear_account_scoped(self: &Rc<Self>) {
        {
            let mut player = self.player.borrow_mut();
            player.queue.clear();
            player.index = 0;
            player.current = None;
            player.radio_seed = None;
            player.radio_cursor = None;
            player.radio_in_flight = false;
            player.begin_track();
            player.start_pending = false;
        }
        self.lyrics.borrow_mut().track_changed();
        self.load_lyrics();
        self.refresh_player();
    }

    /// Redraws once the current signal has returned, for changes that may start from a button on
    /// the very page being redrawn.
    fn render_soon(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(shell) = weak.upgrade() {
                shell.render();
            }
        });
    }

    /// Shows the window, wherever it was — hidden in the background or behind other windows.
    pub fn raise(self: &Rc<Self>) {
        self.window.present();
    }

    /// Ends Goosic. Quitting is explicit; closing the window is not quitting.
    pub fn quit(self: &Rc<Self>) {
        if let Some(app) = self.window.application() {
            app.quit();
        }
    }

    // MARK: side panels

    pub fn toggle_queue(self: &Rc<Self>) {
        let visible = !self.queue_visible.get();
        self.queue_visible.set(visible);
        self.save_preferences(PreferencesPatch {
            queue_visible: Some(visible),
            ..Default::default()
        });
        self.sync_side_panels();
    }

    pub fn toggle_lyrics(self: &Rc<Self>) {
        {
            let mut lyrics = self.lyrics.borrow_mut();
            lyrics.visible = !lyrics.visible;
        }
        self.load_lyrics();
        self.sync_side_panels();
    }

    /// Looks up lyrics for what is playing, if the panel is open and they are not loaded yet.
    fn load_lyrics(self: &Rc<Self>) {
        let request = {
            let player = self.player.borrow();
            self.lyrics.borrow_mut().begin(player.current.as_ref())
        };
        if let Some((requested_for, query)) = request {
            let payload = RequestPayload {
                lyrics: Some(query),
                ..Default::default()
            };
            self.send("lyrics.get", payload, move |shell, answer| {
                let current = shell
                    .player
                    .borrow()
                    .current
                    .as_ref()
                    .map(|track| track.video_id.clone());
                shell
                    .lyrics
                    .borrow_mut()
                    .finish(&requested_for, current.as_deref(), answer);
                // The track may have changed while this was in flight; ask for the new one.
                shell.load_lyrics();
            });
        }
        self.refresh_panels();
    }

    fn sync_side_panels(&self) {
        let queue = self.queue_visible.get();
        let lyrics = self.lyrics.borrow().visible;
        self.queue_panel.root.set_visible(queue);
        self.lyrics_panel.root.set_visible(lyrics);
        self.side.set_visible(queue || lyrics);
        self.bar.set_panels(lyrics, queue);
        self.refresh_panels();
    }

    fn refresh_panels(&self) {
        let player = self.player.borrow();
        if self.queue_visible.get() {
            // The queue marks a row only when that row really is what plays.
            let current = player.current.as_ref().and_then(|current| {
                player
                    .queue
                    .get(player.index)
                    .filter(|queued| queued.id == current.id)
                    .map(|_| player.index)
            });
            self.queue_panel.update(&player.queue, current);
        }
        let lyrics = self.lyrics.borrow();
        if lyrics.visible {
            // During an advertisement the clock is the ad's, not the song's, so nothing is lit.
            let position = if player.advertisement {
                -1.0
            } else {
                player.current_time
            };
            self.lyrics_panel.update(&lyrics, position);
        }
    }

    fn set_status(&self, message: &str) {
        self.player.borrow_mut().status = message.to_owned();
        self.refresh_player();
    }

    fn refresh_player(&self) {
        let loaded = self.official.loaded_video_id().is_some() || self.local.is_loaded();
        {
            let player = self.player.borrow();
            self.bar.update(&player, loaded);
            let snapshot = player.snapshot();
            self.mpris.update(&snapshot);
            self.status_icon.update(&snapshot);
            // An advertisement is audible too; only a confirmed, unpaused renderer counts.
            self.background
                .set_audible(player.confirmed && !player.paused);
        }
        self.refresh_panels();
    }
}

/// A downloaded file as the player and the now-playing bar show a track.
fn downloaded_track(track: &DownloadedTrack) -> Track {
    Track {
        id: track.video_id.clone(),
        title: track.title.clone(),
        subtitle: "Downloaded".to_owned(),
        artist: track.artist.clone(),
        artist_id: None,
        album: String::new(),
        album_id: None,
        duration: String::new(),
        video_id: track.video_id.clone(),
        explicit: false,
        thumbnail: None,
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
