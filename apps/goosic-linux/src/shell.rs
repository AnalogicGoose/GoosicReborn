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
use std::rc::{Rc, Weak};
use std::time::Instant;

use goosic_protocol::{
    AccountUpsert, AccountsSnapshot, Owner, PreferencesPatch, RequestPayload, ResponseEnvelope,
    SettingsSnapshot,
};
use goosic_shell_support::bridge::BridgeEvent;
use goosic_shell_support::catalog::{PageView, Track};
use goosic_shell_support::login::{self, LoginResult};
use goosic_shell_support::navigation::{
    CatalogKey, EntityReference, PlaybackTransition, Route, SearchFilter, Theme,
};
use goosic_shell_support::page_swap::{decide_early_switch, EarlySwitch, EarlySwitchDecision};
use goosic_shell_support::playback::{
    believes_sample, clamp_seek, clamp_volume, is_seekable, merge_preferences,
    should_advance_after_end, volume_sync, EndSample, PendingSeek, ReportedSample, VolumeSync,
    WatchChange, WatchSample, PREFERENCE_SAVE_DELAY,
};
use goosic_shell_support::{ServiceClient, TransportError};
use gtk::prelude::*;
use gtk::{gio, glib};
use serde_json::json;
use uuid::Uuid;

use crate::artwork::ArtworkCache;
use crate::backdrop::ArtworkBackdrop;
use crate::background::Background;
use crate::discord::{DiscordActivity, DiscordPresence};
use crate::full_player::FullPlayer;
use crate::glass::{GlassFrame, GlassGroup, GlassKind};
use crate::layout;
use crate::login_host::{LoginHandlers, LoginHost};
use crate::lyrics::{decode_imported_lyrics, Lyrics};
use crate::mini_player::MiniPlayer;
use crate::mpris::{Mpris, MprisHandlers};
use crate::official_host::{self, OfficialHost};
use crate::pages::LibrarySection;
use crate::pages::{Browser, ShellFacts};
use crate::personal_host::PersonalHost;
use crate::playback::Player;
use crate::player_bar::{BarActions, PlayerBar};
use crate::side_panels::{LyricsPanel, QueueEdit, QueuePanel};
use crate::status_icon::{StatusIcon, TrayHandlers};
use crate::web_profile;
use crate::{bridge, service, theme, ui};

#[path = "shell_presentation.rs"]
mod presentation;

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
    full_open: Cell<bool>,
    sidebar_open: Cell<bool>,
    layout_size: Cell<(i32, i32)>,
    main: gtk::Box,
    content: gtk::Box,
    glass: GlassGroup,
    sidebar: ui::Sidebar,
    sidebar_glass: GlassFrame,
    sidebar_toggle_glass: GlassFrame,
    backdrop: ArtworkBackdrop,
    full_player: FullPlayer,
    mini_player: MiniPlayer,
    discord: DiscordPresence,
    official: Rc<OfficialHost>,
    personal: Rc<PersonalHost>,
    mpris: Rc<Mpris>,
    background: Rc<Background>,
    status_icon: Rc<StatusIcon>,
    login: RefCell<Option<Rc<LoginHost>>>,
    /// The epoch of the accounts snapshot on screen, so a late answer cannot replace a newer one.
    account_epoch: Cell<u64>,
    catalog_epoch: Cell<u64>,
    accounts_read: Cell<bool>,
    rows: gio::ListStore,
    scroller: gtk::ScrolledWindow,
    search_bar: gtk::Box,
    find_bar: GlassFrame,
    find_entry: gtk::SearchEntry,
    bar: PlayerBar,
    bar_glass: GlassFrame,
    side: gtk::Box,
    side_glass: GlassFrame,
    queue_panel: QueuePanel,
    lyrics_panel: LyricsPanel,
    diagnostics: RefCell<String>,
    pending_preferences: RefCell<Option<PreferencesPatch>>,
    preference_token: Cell<u64>,
    theme_subscription: RefCell<Option<gio::SignalSubscription>>,
    sleep_deadline: Cell<Option<Instant>>,
    sleep_end_track: RefCell<Option<String>>,
}

fn no_context() -> Rc<[Track]> {
    Rc::from(Vec::new())
}

impl Shell {
    /// Builds the window, starts the service and asks it to say hello.
    pub fn start(app: &gtk::Application) -> Rc<Shell> {
        theme::install();
        let launched = service::launch();
        web_profile::clear_abandoned_staging();
        let artwork = ArtworkCache::new();
        let shell = Rc::new_cyclic(|weak: &Weak<Shell>| {
            let glass = GlassGroup::new();
            let discord = DiscordPresence::start();
            let initial_discord = discord.is_enabled();
            let actions = Rc::new(ui::Actions {
                navigate: Box::new(forward(weak, Shell::navigate)),
                open: Box::new(forward(weak, Shell::open)),
                play: Box::new({
                    let weak = weak.clone();
                    move |track, context| {
                        if let Some(shell) = weak.upgrade() {
                            shell.play(track, context);
                        }
                    }
                }),
                shuffle_play: Box::new(forward(weak, Shell::shuffle_play)),
                start_radio_for: Box::new(forward(weak, Shell::start_radio_for)),
                retry: Box::new(forward_unit(weak, Shell::retry)),
                back: Box::new(forward_unit(weak, Shell::back)),
                set_theme: Box::new(forward(weak, Shell::set_theme)),
                set_hide_explicit: Box::new(forward(weak, Shell::set_hide_explicit)),
                set_start_page: Box::new(forward(weak, Shell::set_start_page)),
                set_reduce_motion: Box::new(forward(weak, Shell::set_reduce_motion)),
                set_artwork_background: Box::new(forward(weak, Shell::set_artwork_background)),
                set_discord_status: Box::new(forward(weak, Shell::set_discord_status)),
                set_debug_mode: Box::new(forward(weak, Shell::set_debug_mode)),
                toggle_autoplay: Box::new(forward_unit(weak, Shell::toggle_autoplay)),
                toggle_shuffle: Box::new(forward_unit(weak, Shell::toggle_shuffle)),
                import_legacy: Box::new(forward_unit(weak, Shell::import_legacy_preferences)),
                load_more: Box::new(forward_unit(weak, Shell::load_more)),
                sign_in: Box::new(forward_unit(weak, Shell::sign_in)),
                switch_account: Box::new(forward(weak, Shell::switch_account)),
                sign_out: Box::new(forward_unit(weak, Shell::sign_out)),
                remove_account: Box::new(forward(weak, Shell::remove_account)),
                select_library_section: Box::new(forward(weak, Shell::select_library_section)),
                create_playlist: Box::new(forward_unit(weak, Shell::create_playlist)),
                manage_playlist: Box::new(forward(weak, Shell::manage_playlist)),
                save_playlist: Box::new({
                    let weak = weak.clone();
                    move |id, saved| {
                        if let Some(shell) = weak.upgrade() {
                            shell.save_playlist(id, saved);
                        }
                    }
                }),
                like_track: Box::new(forward(weak, Shell::like_track)),
                unlike_track: Box::new(forward(weak, Shell::unlike_track)),
                subscribe_artist: Box::new({
                    let weak = weak.clone();
                    move |id, subscribed| {
                        if let Some(shell) = weak.upgrade() {
                            shell.subscribe_artist(id, subscribed);
                        }
                    }
                }),
                add_to_playlist: Box::new(forward(weak, Shell::add_to_playlist)),
                can_edit_library: Box::new({
                    let weak = weak.clone();
                    move || {
                        weak.upgrade()
                            .is_some_and(|shell| shell.facts.borrow().active_account_id.is_some())
                    }
                }),
                is_refused: Box::new({
                    let weak = weak.clone();
                    move |video_id| {
                        weak.upgrade()
                            .is_some_and(|shell| shell.player.borrow().refused_video_ids.contains(video_id))
                    }
                }),
                queue_track: Box::new({
                    let weak = weak.clone();
                    move |track, next| {
                        if let Some(shell) = weak.upgrade() {
                            shell.enqueue(track, next);
                        }
                    }
                }),
                submit_search: Box::new(forward(weak, Shell::submit_search)),
                clear_recent_searches: Box::new(forward_unit(weak, Shell::clear_recent_searches)),
                check_for_updates: Box::new({
                    let weak = weak.clone();
                    move || {
                        if let Some(shell) = weak.upgrade() {
                            shell.check_for_updates(true);
                        }
                    }
                }),
                open_url: Box::new(forward(weak, Shell::open_url)),
                artwork: artwork.clone(),
            });
            let (scroller, rows) = ui::page_list(actions);
            let search_bar = ui::search_bar(
                forward(weak, Shell::submit_search),
                forward(weak, Shell::select_filter),
                &glass,
            );
            let (find_bar, find_entry) = ui::find_bar(
                forward(weak, Shell::set_track_find),
                forward_unit(weak, Shell::close_find_bar),
                &glass,
            );
            let sidebar = ui::sidebar(ui::SidebarActions {
                navigate: Box::new(forward(weak, Shell::navigate)),
                select_library: Box::new(forward(weak, Shell::select_library_section)),
                open_entity: Box::new(forward(weak, Shell::open)),
                sign_in: Box::new(forward_unit(weak, Shell::sign_in)),
                switch_account: Box::new(forward(weak, Shell::switch_account)),
                sign_out: Box::new(forward_unit(weak, Shell::sign_out)),
                artwork: artwork.clone(),
            });
            let official = OfficialHost::new(official_host::Handlers {
                on_event: Box::new(forward(weak, Shell::receive_official)),
                on_status: Box::new(forward(weak, Shell::host_status)),
                on_page_advanced: Box::new(forward(weak, Shell::official_moved_on)),
                on_page_swapped: Box::new(forward(weak, Shell::official_swapped_early)),
                on_diagnostics: Box::new(forward(weak, Shell::host_diagnostics)),
            });
            let personal = PersonalHost::new();
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
            let bar_actions = Rc::new(BarActions {
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
                expand: Box::new(forward_unit(weak, Shell::toggle_full_player)),
                mini_player: Box::new(forward_unit(weak, Shell::open_mini_player)),
                import_lyrics: Box::new(forward_unit(weak, Shell::import_timed_lyrics)),
                like_current: Box::new(forward_unit(weak, Shell::like_current_track)),
                sleep: Box::new(forward(weak, Shell::set_sleep_timer)),
            });
            let bar = PlayerBar::new(bar_actions.clone(), artwork.clone());
            let full_player = FullPlayer::new(
                bar_actions.clone(),
                artwork.clone(),
                forward_unit(weak, Shell::close_full_player),
                forward_unit(weak, Shell::toggle_fullscreen),
            );
            let mini_player = MiniPlayer::new(
                app,
                bar_actions,
                artwork.clone(),
                forward_unit(weak, Shell::raise),
            );
            let backdrop = ArtworkBackdrop::new(artwork.clone());
            {
                let glass = glass.clone();
                backdrop.set_on_update(move || glass.invalidate());
            }
            let queue_panel = QueuePanel::new(
                forward(weak, Shell::play_queued),
                forward(weak, Shell::edit_queue),
                artwork.clone(),
            );
            let lyrics_panel = LyricsPanel::new(forward(weak, Shell::seek));

            let client = match launched {
                Ok(client) => Some(client),
                Err(error) => {
                    eprintln!("goosic: {error}");
                    sidebar.connection.set_label("Offline");
                    sidebar
                        .status
                        .set_label("The playback service could not start.");
                    sidebar.status.set_tooltip_text(Some(&error.to_string()));
                    None
                }
            };

            let panels = gtk::Box::new(gtk::Orientation::Vertical, 0);
            panels.append(&queue_panel.root);
            panels.append(&lyrics_panel.root);
            let side = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            side.set_hexpand(true);
            side.set_vexpand(true);
            side.add_css_class("goosic-side-panel");
            side.append(&panels);

            let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
            main.set_hexpand(true);
            main.set_vexpand(true);
            main.set_margin_start(212);
            main.set_margin_end(0);
            main.set_margin_top(0);
            main.append(&scroller);
            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_hexpand(true);
            content.set_vexpand(true);
            content.append(&main);

            // Base scene holds ONLY backdrop + scrolling page content (no hidden WebKit views),
            // so every frosted `GlassFrame` sibling blurs the live artwork and scrolled rows cleanly.
            let base_scene = gtk::Overlay::new();
            base_scene.set_child(Some(&backdrop.widget));
            base_scene.add_overlay(&content);
            let underlay_widget: gtk::Widget = base_scene.clone().upcast();

            let sidebar_glass = glass.wrap(&sidebar.root, GlassKind::Sidebar, Some(&underlay_widget));
            sidebar_glass.set_halign(gtk::Align::Start);
            sidebar_glass.set_valign(gtk::Align::Fill);
            sidebar.root.add_css_class("goosic-sidebar");

            let side_glass = glass.wrap(&side, GlassKind::SidePanel, Some(&underlay_widget));
            side_glass.set_width_request(320);
            side_glass.set_halign(gtk::Align::End);
            side_glass.set_valign(gtk::Align::Fill);
            side_glass.set_visible(false);

            bar.root.add_css_class("goosic-player-pill");
            let bar_glass = glass.wrap(&bar.root, GlassKind::PlayerPill, Some(&underlay_widget));
            bar_glass.set_halign(gtk::Align::Fill);
            bar_glass.set_valign(gtk::Align::End);
            bar_glass.set_margin_start(232);
            bar_glass.set_margin_end(20);
            bar_glass.set_margin_bottom(14);

            let sidebar_toggle = gtk::Button::builder()
                .icon_name("sidebar-show-symbolic")
                .tooltip_text("Toggle sidebar")
                .width_request(34)
                .height_request(34)
                .build();
            sidebar_toggle.add_css_class("goosic-sidebar-toggle");
            sidebar_toggle.connect_clicked({
                let toggle = forward_unit(weak, Shell::toggle_sidebar);
                move |_| toggle()
            });
            let sidebar_toggle_glass =
                glass.wrap(&sidebar_toggle, GlassKind::SearchPill, Some(&underlay_widget));
            sidebar_toggle_glass.set_halign(gtk::Align::Start);
            sidebar_toggle_glass.set_valign(gtk::Align::Start);
            sidebar_toggle_glass.set_margin_start(14);
            sidebar_toggle_glass.set_margin_top(14);

            search_bar.set_valign(gtk::Align::Start);
            search_bar.add_css_class("goosic-search-chrome");
            // Bind search pill, segmented bar, and find bar to `base_scene`.
            glass.set_underlay_all(&underlay_widget);

            {
                let glass = glass.clone();
                scroller.vadjustment().connect_value_changed(move |_| {
                    glass.invalidate();
                });
            }

            let root = gtk::Overlay::new();
            root.set_child(Some(&base_scene));
            root.add_overlay(official.widget());
            root.add_overlay(personal.widget());
            root.add_overlay(&search_bar);
            root.add_overlay(&find_bar);
            root.add_overlay(&sidebar_glass);
            root.add_overlay(&side_glass);
            root.add_overlay(&bar_glass);
            root.add_overlay(&sidebar_toggle_glass);
            root.add_overlay(&full_player.root);

            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("Goosic")
                .default_width(1180)
                .default_height(780)
                .child(&root)
                .build();
            window.set_size_request(480, 560);
            window.add_css_class("goosic-shell");
            theme::apply_window(&window, Theme::System);
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

            let facts = ShellFacts {
                discord_status: initial_discord,
                ..ShellFacts::default()
            };

            Shell {
                window,
                client,
                browser: RefCell::new(Browser::new()),
                player: RefCell::new(Player::new()),
                lyrics: RefCell::new(Lyrics::new()),
                facts: RefCell::new(facts),
                queue_visible: Cell::new(false),
                full_open: Cell::new(false),
                sidebar_open: Cell::new(true),
                layout_size: Cell::new((1180, 780)),
                main,
                content,
                glass,
                sidebar,
                sidebar_glass,
                sidebar_toggle_glass,
                backdrop,
                full_player,
                mini_player,
                discord,
                official,
                personal,
                mpris,
                background,
                status_icon,
                login: RefCell::new(None),
                account_epoch: Cell::new(0),
                catalog_epoch: Cell::new(0),
                accounts_read: Cell::new(false),
                rows,
                scroller,
                search_bar,
                find_bar,
                find_entry,
                bar,
                bar_glass,
                side,
                side_glass,
                queue_panel,
                lyrics_panel,
                diagnostics: RefCell::new(String::new()),
                pending_preferences: RefCell::new(None),
                preference_token: Cell::new(0),
                theme_subscription: RefCell::new(None),
                sleep_deadline: Cell::new(None),
                sleep_end_track: RefCell::new(None),
            }
        });
        let weak = Rc::downgrade(&shell);
        *shell.theme_subscription.borrow_mut() = theme::watch_system(move || {
            if let Some(shell) = weak.upgrade() {
                theme::apply_window(&shell.window, shell.facts.borrow().theme);
                shell.refresh_player();
            }
        });
        shell.wire_keyboard();
        shell.wire_layout();
        shell.wire_sleep_timer();
        shell.wire_lyrics_clock();
        shell.render();
        shell.refresh_player();
        shell.connect();
        if shell.client.is_some() {
            let weak = Rc::downgrade(&shell);
            glib::timeout_add_seconds_local_once(4, move || {
                if let Some(shell) = weak.upgrade() {
                    shell.check_for_updates(false);
                }
            });
        }
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
                    shell.sidebar.connection.set_label("Offline");
                    shell.sidebar.status.set_label(&error.to_string());
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
                self.sidebar.connection.set_label("Browsing as a guest");
                self.sidebar.status.set_label("");
                self.sidebar.status.set_visible(false);
                self.load_preferences();
                self.load_accounts();
            }
            Err(error) => {
                let (code, message) = error.describe();
                eprintln!("goosic: could not connect ({code}): {message}");
                self.sidebar.connection.set_label("Offline");
                self.sidebar
                    .status
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
                    let route = match settings.start_page.as_str() {
                        "library" | "liked" => Route::Library,
                        "last" => Route::from_raw(&settings.last_route).unwrap_or(Route::Home),
                        _ => Route::Home,
                    };
                    {
                        let mut browser = shell.browser.borrow_mut();
                        browser.navigate(route);
                        if settings.start_page == "liked" {
                            browser.select_library_section(LibrarySection::Songs);
                        }
                    }
                    ui::select_route(&shell.sidebar.routes, route);
                }
                shell.refresh_player();
                shell.show_new_page();
            },
        );
    }

    /// Takes on stored preferences, whether read at launch or imported from the previous Goosic.
    /// Where the app opens is left to the caller: an import must not move the user.
    fn adopt_settings(&self, settings: &SettingsSnapshot) {
        self.player.borrow_mut().apply_settings(settings);
        let theme = Theme::named(&settings.theme);
        {
            let mut facts = self.facts.borrow_mut();
            facts.theme = theme;
            facts.legacy_imported = settings.imported_from_legacy;
            facts.legacy_available = settings.legacy_available;
            facts.hide_explicit = settings.hide_explicit;
            facts.start_page = settings.start_page.clone();
            facts.reduce_motion = settings.reduce_motion;
            facts.artwork_background = settings.artwork_background;
            facts.autoplay = settings.autoplay;
            facts.shuffle = settings.shuffle;
        }
        if let Some(gtk_settings) = gtk::Settings::default() {
            gtk_settings.set_gtk_enable_animations(!settings.reduce_motion);
        }
        theme::apply(theme);
        theme::apply_window(&self.window, theme);
        self.sync_appearance();
        self.queue_visible.set(settings.queue_visible);
        self.sync_side_panels();
    }

    fn sync_appearance(&self) {
        let light = self.window.has_css_class("goosic-light");
        let high_contrast = self.window.has_css_class("goosic-high-contrast");
        let reduce_motion = self.facts.borrow().reduce_motion;
        self.backdrop.set_appearance(light, self.full_open.get());
        self.glass.set_appearance(light, high_contrast);
        self.full_player.set_appearance(reduce_motion, high_contrast);
        self.mini_player.set_appearance(reduce_motion, high_contrast);
        self.lyrics_panel
            .set_appearance(light, high_contrast, reduce_motion);
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
        theme::apply_window(&self.window, theme);
        self.sync_appearance();
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

    fn set_hide_explicit(self: &Rc<Self>, enabled: bool) {
        self.facts.borrow_mut().hide_explicit = enabled;
        self.save_preferences(PreferencesPatch {
            hide_explicit: Some(enabled),
            ..Default::default()
        });
        self.render_soon();
    }

    fn set_start_page(self: &Rc<Self>, page: String) {
        self.facts.borrow_mut().start_page = page.clone();
        self.save_preferences(PreferencesPatch {
            start_page: Some(page),
            ..Default::default()
        });
        self.render_soon();
    }

    fn set_artwork_background(self: &Rc<Self>, enabled: bool) {
        self.facts.borrow_mut().artwork_background = enabled;
        self.save_preferences(PreferencesPatch {
            artwork_background: Some(enabled),
            ..Default::default()
        });
        self.refresh_player();
        self.render_soon();
    }

    fn set_reduce_motion(self: &Rc<Self>, enabled: bool) {
        self.facts.borrow_mut().reduce_motion = enabled;
        if let Some(gtk_settings) = gtk::Settings::default() {
            gtk_settings.set_gtk_enable_animations(!enabled);
        }
        self.sync_appearance();
        self.save_preferences(PreferencesPatch {
            reduce_motion: Some(enabled),
            ..Default::default()
        });
        self.render_soon();
    }

    fn set_discord_status(self: &Rc<Self>, enabled: bool) {
        self.facts.borrow_mut().discord_status = enabled;
        self.discord.set_enabled(enabled);
        self.refresh_player();
        self.render_soon();
    }

    fn set_debug_mode(self: &Rc<Self>, enabled: bool) {
        self.facts.borrow_mut().debug_mode = enabled;
        self.render_soon();
    }

    fn check_for_updates(self: &Rc<Self>, user_initiated: bool) {
        if matches!(
            self.facts.borrow().update_status,
            crate::updater::UpdateStatus::Checking
        ) {
            return;
        }
        if user_initiated {
            self.facts.borrow_mut().update_status = crate::updater::UpdateStatus::Checking;
            self.render_soon();
        }
        let weak = Rc::downgrade(self);
        crate::updater::check(move |status| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if !user_initiated && matches!(status, crate::updater::UpdateStatus::Failed(_)) {
                return;
            }
            if let crate::updater::UpdateStatus::Available { ref version, .. } = status {
                shell.set_status(&format!(
                    "Goosic v{version} is available in Settings → Updates."
                ));
            }
            shell.facts.borrow_mut().update_status = status;
            shell.render_soon();
        });
    }

    fn open_url(self: &Rc<Self>, url: String) {
        if !url.starts_with("https://") {
            return;
        }
        gtk::UriLauncher::new(&url).launch(
            Some(&self.window),
            None::<&gio::Cancellable>,
            |_| {},
        );
    }

    fn clear_recent_searches(self: &Rc<Self>) {
        self.browser.borrow_mut().clear_recent_searches();
        self.render();
    }

    fn set_track_find(self: &Rc<Self>, text: String) {
        self.browser.borrow_mut().set_track_find(&text);
        self.render();
    }

    pub(super) fn open_find_bar(self: &Rc<Self>) {
        if !self.browser.borrow().can_find_tracks() {
            return self.search_focus();
        }
        self.find_bar.set_visible(true);
        self.find_entry.grab_focus();
        self.glass.invalidate();
    }

    pub(super) fn close_find_bar(self: &Rc<Self>) {
        if !self.find_bar.is_visible() {
            return;
        }
        self.find_bar.set_visible(false);
        self.find_entry.set_text("");
        self.browser.borrow_mut().set_track_find("");
        self.render();
    }

    pub(super) fn open_mini_player(self: &Rc<Self>) {
        self.refresh_player();
        self.mini_player.present();
    }

    /// Opens a native file picker (XDG FileChooser portal in Flatpak) to load session-only
    /// timed-lyrics JSON for the currently playing track without touching playback authority.
    pub(super) fn import_timed_lyrics(self: &Rc<Self>) {
        let Some(video_id) = self
            .player
            .borrow()
            .current
            .as_ref()
            .map(|t| t.video_id.clone())
        else {
            return self.set_status("Play a song before importing timed lyrics.");
        };
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Goosic Timed Lyrics (*.json)"));
        filter.add_pattern("*.json");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title("Import Timed Lyrics")
            .filters(&filters)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                let Ok(file) = result else {
                    return;
                };
                let label = file
                    .basename()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "local file".to_owned());
                match file.load_contents(None::<&gio::Cancellable>) {
                    Ok((bytes, _)) => match decode_imported_lyrics(&bytes) {
                        Ok(document) => {
                            let still_same = shell
                                .player
                                .borrow()
                                .current
                                .as_ref()
                                .is_some_and(|t| t.video_id == video_id);
                            if !still_same {
                                return;
                            }
                            {
                                let mut lyrics = shell.lyrics.borrow_mut();
                                lyrics.visible = true;
                                lyrics.install_imported(&video_id, document, &label);
                            }
                            shell.queue_visible.set(false);
                            shell.sync_side_panels();
                            shell.set_status(&format!("Imported timed lyrics from {label}."));
                        }
                        Err(reason) => shell.set_status(&reason),
                    },
                    Err(error) => {
                        shell.set_status(&format!("Could not read lyrics file: {error}"));
                    }
                }
            },
        );
    }

    fn subscribe_artist(self: &Rc<Self>, artist_id: String, subscribed: bool) {
        if self.facts.borrow().active_account_id.is_none() {
            return;
        }
        let op = if subscribed {
            "subscribeArtist"
        } else {
            "unsubscribeArtist"
        };
        let msg = if subscribed {
            "Subscribed to artist."
        } else {
            "Unsubscribed from artist."
        };
        self.mutate_library(op, json!({"channelId": artist_id}), msg);
    }

    fn start_radio_for(self: &Rc<Self>, track: Track) {
        if self.player.borrow().radio_in_flight {
            return;
        }
        self.begin_radio(track, "Starting radio from");
    }

    /// Lightweight 30fps clock driving word-timed lyric sweeps, animated interlude dots,
    /// and spring row scrolling ONLY while lyrics are visible and music is actively playing.
    fn wire_lyrics_clock(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
            let Some(shell) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let lyrics = shell.lyrics.borrow();
            if !lyrics.visible || lyrics.document().is_none() {
                return glib::ControlFlow::Continue;
            }
            let player = shell.player.borrow();
            if !player.confirmed || player.paused || player.advertisement {
                return glib::ControlFlow::Continue;
            }
            let pos = player.interpolated_position(Instant::now());
            if shell.full_open.get() {
                shell.full_player.lyrics.update(&lyrics, pos);
            } else if shell.side_glass.is_visible() {
                shell.lyrics_panel.update(&lyrics, pos);
            }
            glib::ControlFlow::Continue
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
        if self.layout_size.get().0 < 820 {
            self.sidebar_open.set(false);
            self.apply_layout();
        }
        self.browser.borrow_mut().navigate(route);
        // Selecting a row does not activate it, so this cannot come back here.
        ui::select_route(&self.sidebar.routes, route);
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

    /// Fetches the next part of the page on screen, if it continues and is not fetching it already.
    fn load_more(self: &Rc<Self>) {
        let Some(key) = self.browser.borrow().current_key() else {
            return;
        };
        let request = self.browser.borrow_mut().begin_more(&key);
        let Some((cursor, payload)) = request else {
            return;
        };
        let epoch = self.catalog_epoch.get();
        self.render();
        if let Some((browse_id, title, shape)) = self.personal_source(&key) {
            let weak = Rc::downgrade(self);
            let requested_cursor = cursor.clone();
            self.personal
                .browse(&browse_id, &title, Some(&cursor), shape, move |answer| {
                    let Some(shell) = weak.upgrade() else {
                        return;
                    };
                    if shell.catalog_epoch.get() != epoch {
                        return;
                    }
                    shell.browser.borrow_mut().finish_more_personal(
                        key.clone(),
                        &requested_cursor,
                        answer,
                    );
                    if shell.browser.borrow().current_key().as_ref() == Some(&key) {
                        shell.render();
                    }
                });
            return;
        }
        self.send("catalog.continue", payload, move |shell, answer| {
            if shell.catalog_epoch.get() != epoch {
                return;
            }
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

    fn select_library_section(self: &Rc<Self>, section: LibrarySection) {
        ui::select_library_section(&self.sidebar.routes, section);
        if self.browser.borrow_mut().select_library_section(section) {
            self.show_new_page();
        }
    }

    fn create_playlist(self: &Rc<Self>) {
        if self.facts.borrow().active_account_id.is_none() {
            return;
        }
        let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
        form.set_margin_start(20);
        form.set_margin_end(20);
        form.set_margin_top(20);
        form.set_margin_bottom(20);
        let entry = gtk::Entry::builder()
            .placeholder_text("Playlist name")
            .build();
        form.append(&entry);
        let create = gtk::Button::with_label("Create private playlist");
        form.append(&create);
        let window = gtk::Window::builder()
            .title("Create playlist")
            .transient_for(&self.window)
            .modal(true)
            .default_width(320)
            .child(&form)
            .build();
        let weak = Rc::downgrade(self);
        let dismiss = window.downgrade();
        create.connect_clicked(move |_| {
            let title = entry.text().trim().to_owned();
            if let Some(shell) = weak.upgrade() {
                if title.is_empty() {
                    shell.set_status("Enter a playlist name.");
                } else {
                    shell.mutate_library(
                        "createPlaylist",
                        json!({"title": title, "privacy": "PRIVATE"}),
                        "Playlist created.",
                    );
                    if let Some(window) = dismiss.upgrade() {
                        window.close();
                    }
                }
            }
        });
        window.present();
    }

    fn like_track(self: &Rc<Self>, video_id: String) {
        self.mutate_library(
            "rateTrack",
            json!({"videoId": video_id, "status": "LIKE"}),
            "Song liked.",
        );
    }

    fn unlike_track(self: &Rc<Self>, video_id: String) {
        self.mutate_library(
            "rateTrack",
            json!({"videoId": video_id, "status": "INDIFFERENT"}),
            "Song removed from liked songs.",
        );
    }

    fn save_playlist(self: &Rc<Self>, playlist_id: String, saved: bool) {
        self.mutate_library(
            "ratePlaylist",
            json!({"playlistId": playlist_id, "saved": saved}),
            if saved {
                "Playlist saved to Library."
            } else {
                "Playlist removed from Library."
            },
        );
    }

    fn manage_playlist(self: &Rc<Self>, playlist_id: String) {
        if self.facts.borrow().active_account_id.is_none() {
            return;
        }
        let weak = Rc::downgrade(self);
        let epoch = self.catalog_epoch.get();
        self.personal
            .mutate("listUserPlaylists", json!({}), move |result| {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                if shell.catalog_epoch.get() != epoch {
                    return;
                }
                let owned = match result {
                    Ok(value) => value,
                    Err(message) => {
                        shell.set_status(&message);
                        return;
                    }
                };
                let bare = playlist_id.strip_prefix("VL").unwrap_or(&playlist_id);
                let found = owned["value"].as_array().and_then(|playlists| {
                    playlists
                        .iter()
                        .find(|item| item["id"].as_str() == Some(bare))
                });
                let Some(found) = found else {
                    shell.set_status("Only a playlist you own can be managed.");
                    return;
                };
                let title = found["title"].as_str().unwrap_or("Playlist").to_owned();
                shell.show_playlist_manager(playlist_id, title);
            });
    }

    fn show_playlist_manager(self: &Rc<Self>, playlist_id: String, title: String) {
        let form = gtk::Box::new(gtk::Orientation::Vertical, 10);
        form.set_margin_start(20);
        form.set_margin_end(20);
        form.set_margin_top(20);
        form.set_margin_bottom(20);
        let name = gtk::Entry::builder().text(&title).build();
        form.append(&gtk::Label::new(Some("Playlist name")));
        form.append(&name);
        let rename = gtk::Button::with_label("Rename playlist");
        let public = gtk::Button::with_label("Make public");
        let private = gtk::Button::with_label("Make private");
        let delete = gtk::Button::with_label("Delete playlist…");
        for button in [&rename, &public, &private, &delete] {
            form.append(button);
        }
        let window = gtk::Window::builder()
            .title("Manage playlist")
            .transient_for(&self.window)
            .modal(true)
            .default_width(360)
            .child(&form)
            .build();
        let weak = Rc::downgrade(self);
        let id = playlist_id.clone();
        rename.connect_clicked(move |_| {
            if let Some(shell) = weak.upgrade() {
                let title = name.text().trim().to_owned();
                if title.is_empty() {
                    shell.set_status("Enter a playlist name.");
                } else {
                    shell.mutate_library(
                        "renamePlaylist",
                        json!({"playlistId": id, "title": title}),
                        "Playlist renamed.",
                    );
                }
            }
        });
        for (button, privacy, label) in [
            (&public, "PUBLIC", "Playlist is public."),
            (&private, "PRIVATE", "Playlist is private."),
        ] {
            let weak = Rc::downgrade(self);
            let id = playlist_id.clone();
            button.connect_clicked(move |_| {
                if let Some(shell) = weak.upgrade() {
                    shell.mutate_library(
                        "setPlaylistPrivacy",
                        json!({"playlistId": id, "privacy": privacy}),
                        label,
                    );
                }
            });
        }
        let weak = Rc::downgrade(self);
        let parent = window.downgrade();
        delete.connect_clicked(move |_| {
            if let (Some(shell), Some(parent)) = (weak.upgrade(), parent.upgrade()) {
                shell.confirm_playlist_deletion(&parent, playlist_id.clone());
            }
        });
        window.present();
    }

    fn confirm_playlist_deletion(self: &Rc<Self>, parent: &gtk::Window, playlist_id: String) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.set_margin_start(20);
        content.set_margin_end(20);
        content.set_margin_top(20);
        content.set_margin_bottom(20);
        content.append(&gtk::Label::new(Some("Delete this playlist permanently?")));
        let cancel = gtk::Button::with_label("Keep playlist");
        let delete = gtk::Button::with_label("Delete playlist");
        content.append(&cancel);
        content.append(&delete);
        let window = gtk::Window::builder()
            .title("Delete playlist")
            .transient_for(parent)
            .modal(true)
            .child(&content)
            .build();
        let dismiss = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(window) = dismiss.upgrade() {
                window.close();
            }
        });
        let weak = Rc::downgrade(self);
        let dismiss = window.downgrade();
        let manager = parent.downgrade();
        delete.connect_clicked(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.delete_playlist(playlist_id.clone());
            }
            if let Some(window) = dismiss.upgrade() {
                window.close();
            }
            if let Some(window) = manager.upgrade() {
                window.close();
            }
        });
        window.present();
    }

    fn delete_playlist(self: &Rc<Self>, playlist_id: String) {
        let account = self.facts.borrow().active_account_id.clone();
        let weak = Rc::downgrade(self);
        self.personal.mutate(
            "deletePlaylist",
            json!({"playlistId": playlist_id}),
            move |result| {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                if shell.facts.borrow().active_account_id != account {
                    return;
                }
                match result {
                    Ok(_) => {
                        shell.browser.borrow_mut().clear_account_pages();
                        shell.navigate(Route::Library);
                        shell.set_status("Playlist deleted.");
                    }
                    Err(message) => shell.set_status(&message),
                }
            },
        );
    }

    fn add_to_playlist(self: &Rc<Self>, video_id: String) {
        if self.facts.borrow().active_account_id.is_none() {
            return;
        }
        let weak = Rc::downgrade(self);
        let epoch = self.catalog_epoch.get();
        self.personal
            .mutate("listUserPlaylists", json!({}), move |result| {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                if shell.catalog_epoch.get() != epoch {
                    return;
                }
                let value = match result {
                    Ok(value) => value,
                    Err(message) => {
                        shell.set_status(&message);
                        return;
                    }
                };
                let playlists = value
                    .get("value")
                    .and_then(|value| value.as_array())
                    .cloned()
                    .unwrap_or_default();
                if playlists.is_empty() {
                    shell.set_status("Create a playlist in Library before adding a song.");
                    return;
                }
                let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
                list.set_margin_start(20);
                list.set_margin_end(20);
                list.set_margin_top(20);
                list.set_margin_bottom(20);
                let window = gtk::Window::builder()
                    .title("Add to playlist")
                    .transient_for(&shell.window)
                    .modal(true)
                    .default_width(320)
                    .child(&list)
                    .build();
                for playlist in &playlists {
                    if let (Some(id), Some(title)) =
                        (playlist["id"].as_str(), playlist["title"].as_str())
                    {
                        let button = gtk::Button::with_label(title);
                        let weak = Rc::downgrade(&shell);
                        let dismiss = window.downgrade();
                        let playlist_id = id.to_owned();
                        let video_id = video_id.clone();
                        button.connect_clicked(move |_| {
                            if let Some(shell) = weak.upgrade() {
                                shell.mutate_library(
                                    "addToPlaylist",
                                    json!({"playlistId": playlist_id, "videoId": video_id}),
                                    "Song added to playlist.",
                                );
                            }
                            if let Some(window) = dismiss.upgrade() {
                                window.close();
                            }
                        });
                        list.append(&button);
                    }
                }
                window.present();
            });
    }

    fn mutate_library(
        self: &Rc<Self>,
        operation: &str,
        args: serde_json::Value,
        success: &'static str,
    ) {
        let account = self.facts.borrow().active_account_id.clone();
        if account.is_none() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.personal.mutate(operation, args, move |result| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if shell.facts.borrow().active_account_id != account {
                return;
            }
            match result {
                Ok(_) => {
                    shell
                        .catalog_epoch
                        .set(shell.catalog_epoch.get().wrapping_add(1));
                    shell.browser.borrow_mut().clear_account_pages();
                    shell.set_status(success);
                    shell.load_current(true);
                }
                Err(message) => shell.set_status(&message),
            }
        });
    }

    fn show_new_page(self: &Rc<Self>) {
        self.catalog_epoch
            .set(self.catalog_epoch.get().wrapping_add(1));
        self.browser.borrow_mut().cancel_loading();
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
        if let Some((browse_id, title, shape)) = self.personal_source(&key) {
            if !self.browser.borrow_mut().begin_personal(&key, force) {
                return;
            }
            let epoch = self.catalog_epoch.get();
            let weak = Rc::downgrade(self);
            self.render();
            let is_playlists = browse_id == LibrarySection::Playlists.browse_id();
            self.personal
                .browse(&browse_id, &title, None, shape, move |answer| {
                    let Some(shell) = weak.upgrade() else {
                        return;
                    };
                    if shell.catalog_epoch.get() != epoch {
                        return;
                    }
                    if matches!(key, CatalogKey::Playlist(_) | CatalogKey::Album(_))
                        && answer.as_ref().map_or(true, |page| {
                            page.shelves.is_empty() && page.tracks.is_empty()
                        })
                    {
                        shell.load_anonymous(key, true);
                        return;
                    }
                    if is_playlists {
                        if let Ok(page) = &answer {
                            let view = PageView::from_wire(page);
                            let playlists: Vec<goosic_shell_support::catalog::Card> = view
                                .shelves
                                .into_iter()
                                .flat_map(|shelf| shelf.cards)
                                .collect();
                            shell.sidebar.set_playlists(&playlists);
                        }
                    }
                    shell
                        .browser
                        .borrow_mut()
                        .finish_personal(key.clone(), answer);
                    if shell.browser.borrow().current_key().as_ref() == Some(&key) {
                        shell.render();
                    }
                });
            return;
        }
        self.load_anonymous(key, force);
    }

    fn load_anonymous(self: &Rc<Self>, key: CatalogKey, force: bool) {
        let request = self.browser.borrow_mut().begin(&key, force);
        let Some((command, payload)) = request else {
            return;
        };
        let epoch = self.catalog_epoch.get();
        self.render();
        self.send(command, payload, move |shell, answer| {
            if shell.catalog_epoch.get() != epoch {
                return;
            }
            shell.browser.borrow_mut().finish(key.clone(), answer);
            // An answer for a page the user has left is kept for when they come back, not drawn
            // over the page they are on now.
            let current = shell.browser.borrow().current_key();
            if current.as_ref() == Some(&key) {
                shell.render();
            }
        });
    }

    fn personal_source(&self, key: &CatalogKey) -> Option<(String, String, &'static str)> {
        self.facts.borrow().active_account_id.as_ref()?;
        match key {
            CatalogKey::Route(Route::Home) => {
                Some(("FEmusic_home".into(), "Home".into(), "shelves"))
            }
            CatalogKey::Route(Route::Library) => {
                let section = self.browser.borrow().library_section();
                Some((
                    section.browse_id().into(),
                    section.title().into(),
                    section.shape(),
                ))
            }
            CatalogKey::Playlist(id) => Some((
                if id.starts_with("VL") {
                    id.clone()
                } else {
                    format!("VL{id}")
                },
                "Playlist".into(),
                "tracks",
            )),
            CatalogKey::Album(id) => Some((id.clone(), "Album".into(), "tracks")),
            _ => None,
        }
    }

    fn render(&self) {
        let facts = self.facts.borrow().clone();
        self.sidebar
            .account
            .set_label(facts.account.as_deref().unwrap_or("Guest"));
        let detail = if !facts.connected {
            "Offline"
        } else if facts.account.is_some() {
            "Signed in"
        } else {
            "Browsing as a guest"
        };
        self.sidebar.connection.set_label(detail);
        self.sidebar.avatar.set_label(
            &facts
                .account
                .as_deref()
                .and_then(|name| name.chars().next())
                .unwrap_or('G')
                .to_uppercase()
                .to_string(),
        );
        self.sidebar.update_account_menu(&facts);
        let (rows, searching) = {
            let browser = self.browser.borrow();
            (browser.rows(&facts), browser.shows_search_bar())
        };
        ui::set_rows(&self.rows, rows);
        self.search_bar.set_visible(searching);
        self.apply_layout();
    }

    fn shuffle_play(self: &Rc<Self>, tracks: Rc<[Track]>) {
        if tracks.is_empty() {
            return;
        }
        if !self.player.borrow().shuffle {
            self.toggle_shuffle();
        }
        self.play(tracks[0].clone(), tracks);
    }

    fn like_current_track(self: &Rc<Self>) {
        let id = self
            .player
            .borrow()
            .current
            .as_ref()
            .map(|track| track.video_id.clone());
        if let Some(id) = id {
            self.like_track(id);
        }
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
                if !player.queued().is_some_and(|queued| queued.id == track.id) {
                    player.select(&track);
                }
            } else {
                player.set_queue(context.to_vec(), &track);
            }
        }
        match owner {
            Owner::OfficialWebView => self.load_official(track),
            _ => self.claim_official(track),
        }
    }

    /// Plays the queue from `index`, for a row chosen in the queue panel.
    fn play_queued(self: &Rc<Self>, index: usize) {
        let track = self.player.borrow().queue.get(index).cloned();
        if let Some(track) = track {
            let reason = {
                let player = self.player.borrow();
                if self.facts.borrow().account_busy {
                    Some(ACCOUNT_CHANGING)
                } else if player.transition() != PlaybackTransition::Idle {
                    Some(PENDING)
                } else if player.advertisement {
                    Some(IN_ADVERTISEMENT)
                } else {
                    None
                }
            };
            if let Some(reason) = reason {
                return self.set_status(reason);
            }
            if self.client.is_none() {
                return self.set_status("Connect to the Rust service before playing.");
            }
            {
                let mut player = self.player.borrow_mut();
                if player.index != index {
                    player.queue_changed();
                }
                player.index = index;
            }
            self.play(track, no_context());
        }
    }

    fn enqueue(self: &Rc<Self>, track: Track, next: bool) {
        if self.facts.borrow().account_busy
            || self.player.borrow().transition() != PlaybackTransition::Idle
        {
            return self.set_status(PENDING);
        }
        let title = track.title.clone();
        {
            let mut player = self.player.borrow_mut();
            let at = if next && !player.queue.is_empty() {
                (player.index + 1).min(player.queue.len())
            } else {
                player.queue.len()
            };
            player.queue.insert(at, track);
            player.queue_changed();
        }
        self.set_status(&format!(
            "Added {title} {}.",
            if next { "to play next" } else { "to the queue" }
        ));
        self.queue_visible.set(true);
        self.lyrics.borrow_mut().visible = false;
        self.save_preferences(PreferencesPatch {
            queue_visible: Some(true),
            ..Default::default()
        });
        self.sync_side_panels();
    }

    fn edit_queue(self: &Rc<Self>, edit: QueueEdit) {
        if self.facts.borrow().account_busy
            || self.player.borrow().transition() != PlaybackTransition::Idle
        {
            return self.set_status(PENDING);
        }
        {
            let mut player = self.player.borrow_mut();
            match edit {
                QueueEdit::Remove(index) => {
                    player.remove_queued(index);
                }
                QueueEdit::Move(from, to) => {
                    player.move_queued(from, to);
                }
                QueueEdit::ClearUpcoming => player.clear_upcoming(),
                QueueEdit::ClearPlayed => player.clear_played(),
                QueueEdit::UndoClear => {
                    player.undo_clear(Instant::now());
                }
            }
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(shell) = weak.upgrade() {
                shell.refresh_player();
            }
        });
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
        let generation = self.player.borrow().lease.generation;
        if !self
            .player
            .borrow()
            .holds(Owner::OfficialWebView, generation)
        {
            self.set_status("Rust has not granted the official player.");
            return;
        }
        let (generation, changed, volume, muted, start_paused) = {
            let mut player = self.player.borrow_mut();
            let changed =
                player.current.as_ref().map(|current| &current.video_id) != Some(&track.video_id);
            player.current = Some(track.clone());
            player.begin_track();
            player.begin_load();
            let start_paused = player.take_pause_next_load();
            (
                player.lease.generation,
                changed,
                player.volume,
                player.muted,
                start_paused,
            )
        };
        self.official
            .load(&track.video_id, generation, volume, muted, true);
        if start_paused {
            self.official.pause();
        }
        if changed {
            self.lyrics.borrow_mut().track_changed();
            self.load_lyrics();
        }
        self.refresh_player();
    }

    /// Handles YouTube Music replacing a video before the requested song was heard playing.
    /// Another version of the same song is adopted once under the same lease; anything else
    /// is treated as a refusal, marked dimmed, and skipped while preserving any user pause.
    fn official_swapped_early(self: &Rc<Self>, swap: official_host::PageSwapReport) {
        if self.facts.borrow().account_busy {
            return;
        }
        let (owner, loaded, current_title, generation, volume, muted, listener_paused) = {
            let player = self.player.borrow();
            (
                player.lease.owner,
                self.official.loaded_video_id(),
                player.current.as_ref().map(|t| t.title.clone()),
                player.lease.generation,
                player.volume,
                player.muted,
                player.listener_paused,
            )
        };
        if owner != Owner::OfficialWebView {
            return;
        }
        let early = EarlySwitch {
            requested_video_id: &swap.requested_video_id,
            page_video_id: &swap.page_video_id,
            page_title: &swap.page_title,
            may_adopt: swap.may_adopt,
        };
        match decide_early_switch(
            &early,
            loaded.as_deref(),
            current_title.as_deref().unwrap_or(""),
        ) {
            EarlySwitchDecision::Ignore => {}
            EarlySwitchDecision::Adopt { video_id } => {
                self.player.borrow_mut().begin_load();
                self.official
                    .load(&video_id, generation, volume, muted, false);
                if listener_paused {
                    self.official.pause();
                }
            }
            EarlySwitchDecision::Refused => {
                let title = current_title.unwrap_or_else(|| "This song".to_owned());
                {
                    let mut player = self.player.borrow_mut();
                    let refused_id = player
                        .current
                        .as_ref()
                        .map(|t| t.video_id.clone())
                        .unwrap_or_else(|| swap.requested_video_id.clone());
                    player.refused_video_ids.insert(refused_id);
                    player.pause_next_load = listener_paused;
                }
                self.official.pause();
                self.official_moved_on(swap.requested_video_id);
                self.player.borrow_mut().pause_next_load = false;
                self.set_status(&format!(
                    "“{title}” isn’t available to play here, so it was skipped."
                ));
                self.render_soon();
            }
        }
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
            let now = Instant::now();
            player.last_sample_at = Some(now);
            let listener_paused = player.listener_paused;
            let watched = player.watch.observe(
                &WatchSample {
                    state: &event.state,
                    position: event.current_time,
                    duration: event.duration,
                    is_advertisement: event.is_advertisement,
                    listener_paused,
                    // A pause request sets the listener's intent at once, before the page
                    // confirms it, so there is no separate pending pause to report.
                    pause_requested: false,
                },
                now,
            );
            match watched {
                WatchChange::Interrupted(kind) => eprintln!(
                    "goosic: playback interrupted ({kind:?}) at {:.1}/{:.1}s of {}, state {}{}",
                    event.current_time,
                    event.duration,
                    event.video_id,
                    event.state,
                    if event.is_advertisement {
                        ", advertisement"
                    } else {
                        ""
                    }
                ),
                WatchChange::Recovered => {
                    eprintln!("goosic: playback recovered at {:.1}s", event.current_time)
                }
                WatchChange::Unchanged => {}
            }
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
                VolumeSync::Follow { volume, muted } => ((volume - player.volume).abs() > 0.01
                    || muted != player.muted)
                    .then_some((player.volume, player.muted)),
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
                &EndSample {
                    state: &event.state,
                    is_advertisement: event.is_advertisement,
                    video_id: &event.video_id,
                    position: event.current_time,
                    duration: event.duration,
                },
                player.ended_video_id.as_deref(),
                player.listener_paused,
            );
            if advance {
                player.ended_video_id = Some(event.video_id.clone());
            }
            // The page's own autoplay is not guaranteed: on a fresh profile YouTube Music loads
            // the track paused. The user asked for this track to play, so the load's first paused
            // report is answered with one play request — once, so a pause the user makes is kept.
            let nudge = player.start_pending
                && !player.listener_paused
                && !event.is_advertisement
                && event.state == "paused";
            if nudge || event.state == "playing" {
                player.start_pending = false;
            }
            let title = player
                .current
                .as_ref()
                .map_or(event.video_id.clone(), |track| track.title.clone());
            player.status = if let Some(kind) = player.watch.interruption() {
                kind.message(event.is_advertisement).to_owned()
            } else if event.is_advertisement {
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
        let current = self
            .player
            .borrow()
            .current
            .as_ref()
            .map(|track| track.video_id.clone());
        if current.is_some() && *self.sleep_end_track.borrow() == current {
            self.sleep_end_track.borrow_mut().take();
            self.pause_for_sleep();
            return;
        }
        let (autoplay, next) = {
            let player = self.player.borrow();
            let next = player.next_index(false);
            (player.autoplay, next)
        };
        if !autoplay {
            return self.set_status("Track finished. Autoplay is off.");
        }
        match next {
            Some(index) => self.play_queued(index),
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
                player.queue_changed();
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
                if !fresh.is_empty() {
                    player.queue_changed();
                }
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
        let generation = self.player.borrow().lease.generation;
        if self
            .player
            .borrow()
            .holds(Owner::OfficialWebView, generation)
            && self.official.is_loaded_for(generation)
        {
            self.player.borrow_mut().listener_paused = !paused;
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
        let index = self.player.borrow().previous_index();
        if let Some(index) = index {
            self.play_queued(index);
        }
    }

    fn next(self: &Rc<Self>) {
        let index = self.player.borrow().next_index(true);
        if let Some(index) = index {
            self.play_queued(index);
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
                {
                    let mut player = self.player.borrow_mut();
                    player.pending_seek = Some(PendingSeek {
                        position: target,
                        requested_at: Instant::now(),
                    });
                }
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
        self.official.set_muted(false);
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
        self.facts.borrow_mut().shuffle = shuffle;
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
        self.facts.borrow_mut().autoplay = autoplay;
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
        let previous = self.facts.borrow().active_account_id.clone();
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
        if self.facts.borrow().active_account_id != previous {
            self.catalog_epoch
                .set(self.catalog_epoch.get().wrapping_add(1));
            self.browser.borrow_mut().clear_account_pages();
        }
        self.render_soon();
        // An account remembered from the last run is where this run starts, not a change to make:
        // nothing plays yet, so its profile is bound directly.
        if initial && self.player.borrow().lease.owner == Owner::None {
            self.official
                .bind_profile(profile.unwrap_or(goosic_shell_support::bridge::GUEST_PROFILE_ID));
        }
        if initial {
            self.personal.bind(profile);
            self.load_current(false);
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
        let profile = self
            .facts
            .borrow()
            .active_account_id
            .as_deref()
            .map(|_| self.active_profile());
        self.personal.bind(profile);
        self.render_soon();
        self.set_status(message);
        self.load_current(false);
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
        self.personal.bind(None);
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
        self.discord.clear();
        self.mini_player.window.set_visible(false);
        if let Some(app) = self.window.application() {
            app.quit();
        }
    }

    // MARK: side panels

    pub fn toggle_queue(self: &Rc<Self>) {
        let visible = !self.queue_visible.get();
        self.queue_visible.set(visible);
        if visible {
            self.lyrics.borrow_mut().visible = false;
        }
        self.save_preferences(PreferencesPatch {
            queue_visible: Some(visible),
            ..Default::default()
        });
        self.sync_side_panels();
    }

    pub fn toggle_lyrics(self: &Rc<Self>) {
        let visible = {
            let mut lyrics = self.lyrics.borrow_mut();
            lyrics.visible = !lyrics.visible;
            lyrics.visible
        };
        if visible && self.queue_visible.replace(false) {
            self.save_preferences(PreferencesPatch {
                queue_visible: Some(false),
                ..Default::default()
            });
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
        if self.full_open.get() && queue {
            self.full_player.mount_queue(&self.queue_panel.root);
        } else if let Some(panels) = self.side.first_child().and_downcast::<gtk::Box>() {
            if self.queue_panel.root.parent().as_ref() != Some(panels.upcast_ref()) {
                if let Some(parent) = self.queue_panel.root.parent().and_downcast::<gtk::Box>() {
                    parent.remove(&self.queue_panel.root);
                }
                panels.prepend(&self.queue_panel.root);
            }
        }
        self.queue_panel.root.set_visible(queue);
        self.lyrics_panel.root.set_visible(lyrics);
        self.side_glass
            .set_visible((queue || lyrics) && !self.full_open.get());
        self.bar.set_panels(lyrics, queue);
        self.full_player.set_panels(lyrics, queue);
        self.apply_layout();
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
            self.queue_panel
                .set_undo_available(player.can_undo_clear(Instant::now()));
        }
        let lyrics = self.lyrics.borrow();
        let seekable = self.facts.borrow().connected
            && !self.facts.borrow().account_busy
            && player.transition() == PlaybackTransition::Idle
            && is_seekable(player.duration, player.lease.owner, player.advertisement);
        self.lyrics_panel.set_seekable(seekable);
        self.full_player.lyrics.set_seekable(seekable);
        if lyrics.visible {
            // During an advertisement the clock is the ad's, not the song's, so nothing is lit.
            let position = if player.advertisement {
                -1.0
            } else {
                player.current_time
            };
            self.lyrics_panel.update(&lyrics, position);
            if self.full_open.get() {
                self.full_player.lyrics.update(&lyrics, position);
            }
        }
    }

    fn set_status(&self, message: &str) {
        self.player.borrow_mut().status = message.to_owned();
        self.refresh_player();
    }

    fn refresh_player(&self) {
        let loaded = self.official.loaded_video_id().is_some();
        {
            let player = self.player.borrow();
            self.bar.update(&player, loaded);
            let cancel_end_timer = self.sleep_end_track.borrow().as_ref().is_some_and(|id| {
                player
                    .current
                    .as_ref()
                    .is_none_or(|track| &track.video_id != id)
            });
            if cancel_end_timer {
                self.sleep_end_track.borrow_mut().take();
            }
            self.full_player.update(&player, loaded);
            self.mini_player.update(&player, loaded);
            self.sync_appearance();
            self.backdrop.update(
                player.current.as_ref().and_then(|t| t.thumbnail.as_deref()),
                (self.facts.borrow().artwork_background || self.full_open.get())
                    && !self.window.has_css_class("goosic-high-contrast"),
            );
            let discord_activity = if player.confirmed && !player.paused && !player.advertisement {
                player.current.as_ref().and_then(|track| {
                    DiscordActivity::from_playback(
                        &track.title,
                        &track.artist,
                        &track.album,
                        track.thumbnail.as_deref(),
                        player.current_time,
                        player.duration,
                    )
                })
            } else {
                None
            };
            self.discord.update(discord_activity);
            let snapshot = player.snapshot();
            self.mpris.update(&snapshot);
            self.status_icon.update(&snapshot);
            // An advertisement is audible too; only a confirmed, unpaused renderer counts.
            self.background
                .set_audible(player.confirmed && !player.paused);
        }
        self.refresh_panels();
        self.glass.invalidate();
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

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use crate::pages::PageRow;
    use gtk::{gdk, graphene};

    fn settle() {
        let context = glib::MainContext::default();
        let until = Instant::now() + std::time::Duration::from_millis(350);
        while Instant::now() < until {
            while Instant::now() < until && context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn capture(window: &(impl IsA<gtk::Window> + IsA<gtk::Widget> + IsA<gtk::Native>), name: &str) {
        let Some(directory) = std::env::var_os("GOOSIC_UI_CAPTURE_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let child = window.child().unwrap();
        let snapshot = gtk::Snapshot::new();
        let base = if window.has_css_class("goosic-high-contrast") {
            gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)
        } else if window.has_css_class("goosic-immersive") {
            gdk::RGBA::new(0.071, 0.071, 0.078, 1.0)
        } else if window.has_css_class("goosic-light") {
            gdk::RGBA::new(0.984, 0.984, 0.992, 1.0)
        } else {
            gdk::RGBA::new(0.125, 0.125, 0.125, 1.0)
        };
        snapshot.append_color(
            &base,
            &graphene::Rect::new(0.0, 0.0, child.width() as f32, child.height() as f32),
        );
        let paintable = gtk::WidgetPaintable::new(Some(&child));
        paintable.snapshot(
            &snapshot,
            f64::from(child.width()),
            f64::from(child.height()),
        );
        let node = snapshot.to_node().expect("a visible window renders");
        let rect = graphene::Rect::new(0.0, 0.0, child.width() as f32, child.height() as f32);
        let texture = window
            .renderer()
            .unwrap()
            .render_texture(&node, Some(&rect));
        texture
            .save_to_png(directory.join(format!("{name}.png")))
            .unwrap();
    }

    fn button_with_tooltip(widget: &gtk::Widget, tooltip: &str) -> Option<gtk::Button> {
        if widget.tooltip_text().as_deref() == Some(tooltip) {
            if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
                return Some(button);
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(button) = button_with_tooltip(&widget, tooltip) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn check_volume_geometry(shell: &Rc<Shell>, bar: &PlayerBar, name: &str) {
        let before = bar.root.compute_bounds(&shell.window).unwrap();
        let speaker = button_with_tooltip(bar.root.upcast_ref(), "Volume (Ctrl+M to mute)")
            .expect("volume remains discoverable");
        speaker.emit_clicked();
        settle();
        let after = bar.root.compute_bounds(&shell.window).unwrap();
        assert_eq!(before, after, "volume must not resize or move the player");
        capture(&shell.window, name);
        shell.escape();
        settle();
        assert!(!bar.close_volume(), "Escape already dismissed volume");
        assert_eq!(shell.full_open.get(), name.starts_with("full"));
    }

    #[test]
    #[ignore = "requires a display and isolated XDG directories; never signs in or plays audio"]
    fn desktop_layout_keeps_controls_inside_the_window() {
        gtk::init().unwrap();
        let app = gtk::Application::builder()
            .application_id("io.github.analogicgoose.Goosic.ParityTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let remote = "https://i.ytimg.com/vi/parity-fixture/hqdefault.jpg";
        let bytes: Vec<u8> = (0..256 * 256)
            .flat_map(|index| {
                let (x, y) = (index % 256, index / 256);
                [
                    ((x + y) / 2) as u8,
                    (40 + x / 4) as u8,
                    (170 - y / 3) as u8,
                    255,
                ]
            })
            .collect();
        let texture = gdk::MemoryTexture::new(
            256,
            256,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(bytes),
            256 * 4,
        );
        let cache = glib::user_cache_dir().join("goosic/artwork");
        std::fs::create_dir_all(&cache).unwrap();
        texture
            .save_to_png(cache.join(format!(
                "{}.img",
                goosic_shell_support::artwork::cache_key(remote)
            )))
            .unwrap();
        let shell = Shell::start(&app);
        assert!(
            shell.client.is_none(),
            "run with GOOSIC_SERVICE_PATH pointing at a missing fixture path"
        );
        let tracks: Rc<[Track]> = (0..12)
            .map(|index| Track {
                id: format!("fixture-{index}"),
                video_id: format!("fixture-{index}"),
                title: [
                    "Midnight City",
                    "Everything In Its Right Place",
                    "Space Song",
                    "The Less I Know The Better",
                ][index % 4]
                    .into(),
                artist: ["M83", "Radiohead", "Beach House", "Tame Impala"][index % 4].into(),
                subtitle: String::new(),
                album: "A very long album title to exercise ellipsizing".into(),
                artist_id: None,
                album_id: None,
                duration: "4:04".into(),
                explicit: false,
                thumbnail: Some(remote.into()),
            })
            .collect::<Vec<_>>()
            .into();
        {
            let mut player = shell.player.borrow_mut();
            player.queue = tracks.to_vec();
            player.current = Some(tracks[0].clone());
            player.duration = 244.0;
            player.current_time = 62.0;
            player.status = "Playback fixture".into();
        }
        shell.facts.borrow_mut().theme = Theme::Dark;
        theme::apply(Theme::Dark);
        theme::apply_window(&shell.window, Theme::Dark);
        shell.refresh_player();

        let cards = vec![
            goosic_shell_support::catalog::Card {
                id: "vid-1".into(),
                title: "Midnight City (Official Video)".into(),
                subtitle: "Video · M83 · 42M views".into(),
                action: Some(goosic_shell_support::catalog::CardAction::Play(Box::new(
                    tracks[0].clone(),
                ))),
                thumbnail: Some(remote.into()),
            },
            goosic_shell_support::catalog::Card {
                id: "alb-1".into(),
                title: "Hurry Up, We're Dreaming".into(),
                subtitle: "Album · M83".into(),
                action: Some(goosic_shell_support::catalog::CardAction::Show(
                    EntityReference::Album("MPRE1".into()),
                )),
                thumbnail: Some(remote.into()),
            },
            goosic_shell_support::catalog::Card {
                id: "alb-2".into(),
                title: "Kid A".into(),
                subtitle: "Album · Radiohead".into(),
                action: Some(goosic_shell_support::catalog::CardAction::Show(
                    EntityReference::Album("MPRE2".into()),
                )),
                thumbnail: Some(remote.into()),
            },
            goosic_shell_support::catalog::Card {
                id: "art-1".into(),
                title: "Beach House".into(),
                subtitle: "Artist · 12M monthly audience".into(),
                action: Some(goosic_shell_support::catalog::CardAction::Show(
                    EntityReference::Artist("UC1".into()),
                )),
                thumbnail: Some(remote.into()),
            },
            goosic_shell_support::catalog::Card {
                id: "alb-3".into(),
                title: "Currents".into(),
                subtitle: "Album · Tame Impala".into(),
                action: Some(goosic_shell_support::catalog::CardAction::Show(
                    EntityReference::Album("MPRE3".into()),
                )),
                thumbnail: Some(remote.into()),
            },
        ];
        shell.sidebar.set_playlists(&cards[1..4]);
        ui::set_rows(
            &shell.rows,
            vec![
                PageRow::Header {
                    title: "Home".into(),
                    subtitle: String::new(),
                },
                PageRow::CardShelf {
                    title: "Listen again".into(),
                    cards: cards.clone(),
                },
                PageRow::CardShelf {
                    title: "Mixed for you".into(),
                    cards: cards[1..].to_vec(),
                },
                PageRow::ShelfTitle("Quick picks".into()),
                PageRow::TrackShelf(tracks.clone()),
            ],
        );
        shell.window.present();
        settle();
        capture(&shell.window, "home-wide");
        assert!(
            shell.sidebar.root.width() <= 230,
            "a long status must not stretch the sidebar"
        );
        check_volume_geometry(&shell, &shell.bar, "home-volume-wide");
        shell.toggle_queue();
        settle();
        capture(&shell.window, "queue-wide");
        shell.toggle_queue();

        // Capture Search landing page (frosted pill + segmented picker + recent searches + colored Browse cards)
        shell.browser.borrow_mut().submit_search("M83 Midnight City");
        shell.browser.borrow_mut().submit_search("Beach House");
        shell.browser.borrow_mut().submit_search("");
        shell.search_focus();
        settle();
        capture(&shell.window, "search-wide");

        // Capture Artist Hero page with sampled tint and frosted round actions
        ui::set_rows(
            &shell.rows,
            vec![
                PageRow::Back,
                PageRow::ArtistHero {
                    artist_id: "UC1".into(),
                    name: "Beach House".into(),
                    header: Box::new(goosic_protocol::ArtistHeader {
                        banner: Some(remote.into()),
                        portrait: None,
                        audience: Some("12.4M monthly audience".into()),
                        description: Some("Dream pop duo formed in Baltimore.".into()),
                        subscribers: Some("1.8M".into()),
                    }),
                    tracks: tracks.clone(),
                },
                PageRow::ShelfTitle("Top Songs".into()),
                PageRow::Track {
                    track: Box::new(tracks[2].clone()),
                    context: tracks.clone(),
                },
            ],
        );
        shell.search_bar.set_visible(false);
        shell.apply_layout();
        settle();
        capture(&shell.window, "artist-hero-wide");

        // Capture Mini Player window
        shell.open_mini_player();
        settle();
        capture(&shell.mini_player.window, "mini-player");
        shell.mini_player.window.set_visible(false);

        shell.toggle_full_player();
        settle();
        capture(&shell.window, "full-cover");
        assert!(shell.full_player.root.is_visible());
        assert!(
            shell.official.widget().is_visible(),
            "the renderer stays mapped in full-player mode"
        );
        assert!(shell.content.is_visible());
        shell.toggle_queue();
        settle();
        capture(&shell.window, "full-queue-wide");
        assert!(shell.queue_panel.root.is_mapped());
        assert_ne!(
            shell.queue_panel.root.parent(),
            shell.side.first_child(),
            "the same queue is mounted in the full player"
        );
        shell.window.set_default_size(480, 640);
        settle();
        capture(&shell.window, "full-queue-narrow");
        shell.toggle_queue();
        shell.window.set_default_size(1180, 780);
        settle();
        shell.layout_size.set((shell.window.width(), shell.window.height()));
        shell.apply_layout();
        shell.lyrics.borrow_mut().visible = true;
        shell.lyrics.borrow_mut().finish(
            &tracks[0].video_id,
            Some(&tracks[0].video_id),
            Ok(ResponseEnvelope::success(
                "lyrics-fixture",
                goosic_protocol::ResponsePayload {
                    lyrics: Some(goosic_protocol::LyricsDocument {
                        source: "Fixture".into(),
                        synced: true,
                        truncated: false,
                        lines: vec![
                            goosic_protocol::LyricsLine {
                                at_ms: 20_000,
                                end_ms: Some(38_000),
                                text: "Waiting in a car".into(),
                                words: vec![],
                            },
                            goosic_protocol::LyricsLine {
                                at_ms: 40_000,
                                end_ms: Some(58_000),
                                text: "Waiting for a ride in the dark".into(),
                                words: vec![],
                            },
                            goosic_protocol::LyricsLine {
                                at_ms: 60_000,
                                end_ms: Some(68_000),
                                text: "The night city grows around us".into(),
                                words: vec![
                                    goosic_protocol::LyricsWord {
                                        at_ms: 60_000,
                                        end_ms: Some(61_000),
                                        text: "The ".into(),
                                    },
                                    goosic_protocol::LyricsWord {
                                        at_ms: 61_000,
                                        end_ms: Some(62_500),
                                        text: "night ".into(),
                                    },
                                    goosic_protocol::LyricsWord {
                                        at_ms: 62_500,
                                        end_ms: Some(64_000),
                                        text: "city ".into(),
                                    },
                                    goosic_protocol::LyricsWord {
                                        at_ms: 64_000,
                                        end_ms: Some(66_000),
                                        text: "grows ".into(),
                                    },
                                    goosic_protocol::LyricsWord {
                                        at_ms: 66_000,
                                        end_ms: Some(68_000),
                                        text: "around us".into(),
                                    },
                                ],
                            },
                            goosic_protocol::LyricsLine {
                                at_ms: 74_000,
                                end_ms: Some(82_000),
                                text: "Look at the horizon glow".into(),
                                words: vec![],
                            },
                            goosic_protocol::LyricsLine {
                                at_ms: 84_000,
                                end_ms: Some(92_000),
                                text: "The city is my church tonight".into(),
                                words: vec![],
                            },
                            goosic_protocol::LyricsLine {
                                at_ms: 94_000,
                                end_ms: Some(102_000),
                                text: "It wraps me in the sparkling twilight".into(),
                                words: vec![],
                            },
                        ],
                    }),
                    ..Default::default()
                },
            )),
        );
        shell.sync_side_panels();
        settle();
        shell.refresh_panels();
        settle();
        capture(&shell.window, "full-lyrics-wide");
        shell.window.set_default_size(480, 640);
        settle();
        capture(&shell.window, "full-lyrics-narrow");
        shell.close_full_player();
        settle();
        capture(&shell.window, "home-narrow");
        let geometry = shell.bar.root.compute_bounds(&shell.window).unwrap();
        assert!(geometry.x() >= 0.0);
        assert!(
            geometry.x() + geometry.width() <= shell.window.width() as f32 + 1.0,
            "transport escaped the window: {geometry:?}, window {}",
            shell.window.width()
        );
        assert!(
            shell.window.width() <= 500,
            "the narrow layout must not force a wide window"
        );
        check_volume_geometry(&shell, &shell.bar, "volume-narrow");
        shell.lyrics.borrow_mut().visible = false;
        shell.toggle_queue();
        settle();
        assert_eq!(shell.queue_panel.root.parent(), shell.side.first_child());
        capture(&shell.window, "queue-narrow");
        shell.toggle_queue();
        shell.set_sleep_timer(15);
        assert!(shell.sleep_deadline.get().is_some());
        shell.set_sleep_timer(-1);
        assert!(shell.sleep_deadline.get().is_none());
        assert_eq!(shell.sleep_end_track.borrow().as_deref(), Some("fixture-0"));
        shell.set_sleep_timer(0);
        assert!(shell.sleep_end_track.borrow().is_none());
        shell.search_focus();
        settle();
        capture(&shell.window, "search-narrow");
        assert!(shell.search_bar.is_mapped());
        shell.navigate(Route::Settings);
        settle();
        capture(&shell.window, "settings-narrow");
        assert!(
            shell.window.width() <= 500,
            "settings must fit narrow windows"
        );
        shell.facts.borrow_mut().theme = Theme::Light;
        theme::apply(Theme::Light);
        theme::apply_window(&shell.window, Theme::Light);
        shell.apply_layout();
        settle();
        capture(&shell.window, "settings-light-narrow");
        shell.toggle_full_player();
        settle();
        capture(&shell.window, "full-from-light-narrow");
        assert!(shell.window.has_css_class("goosic-immersive"));
        shell.window.add_css_class("goosic-high-contrast");
        shell.refresh_player();
        settle();
        capture(&shell.window, "full-high-contrast-narrow");
        assert!(!shell.backdrop.widget.is_visible());
        shell.window.destroy();
    }
}
