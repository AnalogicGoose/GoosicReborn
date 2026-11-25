//! The floating glass player capsule along the bottom of the window, matching the macOS design
//! language and layout in `docs/UI_DESIGN.md`.
//!
//! It shows only what the player confirmed. A play request does not turn the button into Pause;
//! a validated report from the renderer does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use goosic_protocol::Owner;
use goosic_shell_support::navigation::{PlaybackTransition, RepeatMode};
use goosic_shell_support::playback::{
    displayed_position, is_seekable, now_playing_subtitle, time_text, volume_gain, volume_position,
};
use gtk::glib;
use gtk::prelude::*;

use crate::artwork::ArtworkCache;
use crate::playback::Player;

/// A drag on the position slider becomes one seek once the pointer rests this long, rather than a
/// seek per pixel.
const SEEK_SETTLE: Duration = Duration::from_millis(200);

const ARTWORK_SIZE: i32 = 36;

/// What the bar and full player can ask the shell to do.
pub struct BarActions {
    pub toggle_pause: Box<dyn Fn()>,
    pub previous: Box<dyn Fn()>,
    pub next: Box<dyn Fn()>,
    pub seek: Box<dyn Fn(f64)>,
    pub volume: Box<dyn Fn(f64)>,
    pub mute: Box<dyn Fn()>,
    pub shuffle: Box<dyn Fn()>,
    pub repeat: Box<dyn Fn()>,
    pub autoplay: Box<dyn Fn()>,
    pub radio: Box<dyn Fn()>,
    pub stop: Box<dyn Fn()>,
    pub lyrics: Box<dyn Fn()>,
    pub queue: Box<dyn Fn()>,
    pub expand: Box<dyn Fn()>,
    pub mini_player: Box<dyn Fn()>,
    pub import_lyrics: Box<dyn Fn()>,
    pub like_current: Box<dyn Fn()>,
    /// 0 cancels; -1 stops at the end of the current song; positive values are minutes.
    pub sleep: Box<dyn Fn(i32)>,
}

pub struct PlayerBar {
    pub root: gtk::Box,
    main_row: gtk::Box,
    bottom_row: gtk::Box,
    transport: gtk::Box,
    controls: gtk::Box,
    stacked: Cell<bool>,
    artwork_cache: Rc<ArtworkCache>,
    artwork_slot: gtk::Box,
    /// The thumbnail the artwork shows, so it is replaced only when the track changes.
    artwork_shown: RefCell<Option<String>>,
    title: gtk::Label,
    subtitle: gtk::Label,
    status: gtk::Label,
    previous: gtk::Button,
    play_pause: gtk::Button,
    next: gtk::Button,
    elapsed: gtk::Label,
    position: gtk::Scale,
    total: gtk::Label,
    mute: gtk::Button,
    volume: gtk::Scale,
    volume_button: gtk::Button,
    volume_close: gtk::Button,
    utility: gtk::Stack,
    shuffle: gtk::ToggleButton,
    repeat: gtk::Button,
    autoplay: gtk::ToggleButton,
    radio: gtk::Button,
    stop: gtk::Button,
    lyrics: gtk::ToggleButton,
    queue: gtk::ToggleButton,
    /// True while a drag on the position slider has not settled, so reports do not pull it back.
    seeking: Rc<Cell<bool>>,
    /// True while `update` is writing to widgets, so programmatic updates are not mistaken for clicks.
    updating: Rc<Cell<bool>>,
}

impl PlayerBar {
    pub fn new(actions: Rc<BarActions>, artwork_cache: Rc<ArtworkCache>) -> PlayerBar {
        let seeking = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));

        let title = single_line("Nothing playing");
        title.set_markup("<b>Nothing playing</b>");
        title.add_css_class("goosic-player-title");
        let subtitle = single_line("Choose a track to begin");
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("goosic-player-subtitle");
        let status = single_line("");
        status.add_css_class("dim-label");
        status.set_visible(false);

        let previous = icon_button("media-skip-backward-symbolic", "Previous");
        let play_pause = icon_button("media-playback-start-symbolic", "Play");
        let next = icon_button("media-skip-forward-symbolic", "Next");
        let mute = icon_button("audio-volume-high-symbolic", "Mute");
        let repeat = icon_button("media-playlist-repeat-symbolic", "Repeat off");
        repeat.add_css_class("goosic-subtle-control");
        let stop = gtk::Button::with_label("Stop playback");
        let radio = gtk::Button::with_label("Start radio");
        let shuffle = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .tooltip_text("Shuffle")
            .valign(gtk::Align::Center)
            .build();
        shuffle.add_css_class("goosic-subtle-control");
        let autoplay = gtk::ToggleButton::with_label("Autoplay");
        autoplay.set_tooltip_text(Some("Keep playing when the queue runs out"));
        let lyrics = gtk::ToggleButton::builder()
            .icon_name("chat-message-new-symbolic")
            .tooltip_text("Show lyrics")
            .valign(gtk::Align::Center)
            .build();
        let queue = gtk::ToggleButton::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Show Up Next")
            .valign(gtk::Align::Center)
            .build();

        connect(&previous, &actions, |a| (a.previous)());
        connect(&play_pause, &actions, |a| (a.toggle_pause)());
        connect(&next, &actions, |a| (a.next)());
        connect(&mute, &actions, |a| (a.mute)());
        connect(&repeat, &actions, |a| (a.repeat)());
        connect(&stop, &actions, |a| (a.stop)());
        connect(&radio, &actions, |a| (a.radio)());
        connect_toggle(&shuffle, &actions, &updating, |a| (a.shuffle)());
        connect_toggle(&autoplay, &actions, &updating, |a| (a.autoplay)());
        connect_toggle(&lyrics, &actions, &updating, |a| (a.lyrics)());
        connect_toggle(&queue, &actions, &updating, |a| (a.queue)());

        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        position.set_draw_value(false);
        position.set_hexpand(true);
        position.add_css_class("goosic-progress");
        {
            let actions = actions.clone();
            let seeking = seeking.clone();
            let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
            position.connect_change_value(move |_, _, value| {
                seeking.set(true);
                if let Some(previous) = pending.borrow_mut().take() {
                    previous.remove();
                }
                let (actions, seeking, slot) = (actions.clone(), seeking.clone(), pending.clone());
                let source = glib::timeout_add_local_once(SEEK_SETTLE, move || {
                    slot.borrow_mut().take();
                    seeking.set(false);
                    (actions.seek)(value);
                });
                *pending.borrow_mut() = Some(source);
                glib::Propagation::Proceed
            });
        }

        // Slider works in audio-taper positions [0, 1]; gain is position^3 via goosic-shell-support.
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_width_request(104);
        volume.set_valign(gtk::Align::Center);
        volume.add_css_class("goosic-volume");
        {
            let actions = actions.clone();
            let updating = updating.clone();
            volume.connect_change_value(move |_, _, value| {
                if !updating.get() {
                    (actions.volume)(volume_gain(value.clamp(0.0, 1.0)));
                }
                glib::Propagation::Proceed
            });
        }

        let elapsed = gtk::Label::new(Some("0:00"));
        elapsed.add_css_class("goosic-time");
        elapsed.set_visible(false);
        let total = gtk::Label::new(Some("-0:00"));
        total.add_css_class("goosic-time");
        total.set_visible(false);

        let artwork_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        artwork_slot.append(&placeholder_artwork());

        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_valign(gtk::Align::Center);
        text.set_hexpand(true);
        text.append(&title);
        text.append(&subtitle);
        text.append(&status);

        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        transport.set_valign(gtk::Align::Center);
        transport.append(&shuffle);
        transport.append(&previous);
        transport.append(&play_pause);
        transport.append(&next);
        transport.append(&repeat);
        play_pause.add_css_class("goosic-play-button");

        let cover_button = gtk::Button::builder()
            .child(&artwork_slot)
            .tooltip_text("Open full player (Ctrl+Shift+F)")
            .valign(gtk::Align::Center)
            .build();
        cover_button.add_css_class("goosic-cover-button");
        connect(&cover_button, &actions, |a| (a.expand)());

        let track_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        track_row.set_hexpand(true);
        track_row.set_valign(gtk::Align::Center);
        track_row.append(&cover_button);
        track_row.append(&text);

        let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        timeline.add_css_class("goosic-player-timeline");
        timeline.append(&elapsed);
        timeline.append(&position);
        timeline.append(&total);

        // macOS layout: artwork + metadata on top, thin capsule progress directly beneath both.
        let playing = gtk::Box::new(gtk::Orientation::Vertical, 3);
        playing.set_hexpand(true);
        playing.set_valign(gtk::Align::Center);
        playing.add_css_class("goosic-player-metadata");
        playing.append(&track_row);
        playing.append(&timeline);

        let more = build_more_menu(&actions, &autoplay, &radio, &stop, &mute);

        // Homogeneous stack ensures opening the inline volume capsule never changes the bar's
        // width, height, or transport alignment (matching macOS `NativeMacPlayerBar`).
        let utility = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .valign(gtk::Align::Center)
            .build();

        let volume_button = icon_button("audio-volume-high-symbolic", "Volume (Ctrl+M to mute)");
        let volume_close = icon_button("audio-volume-high-symbolic", "Hide volume controls");

        let panels = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        panels.set_halign(gtk::Align::End);
        panels.set_valign(gtk::Align::Center);
        panels.append(&lyrics);
        panels.append(&queue);
        panels.append(&volume_button);

        let volume_capsule = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        volume_capsule.set_halign(gtk::Align::End);
        volume_capsule.set_valign(gtk::Align::Center);
        volume_capsule.add_css_class("goosic-volume-capsule");
        volume_capsule.append(&volume);
        volume_capsule.append(&volume_close);

        utility.add_named(&panels, Some("panels"));
        utility.add_named(&volume_capsule, Some("volume"));
        utility.set_visible_child_name("panels");

        {
            let utility = utility.clone();
            volume_button.connect_clicked(move |_| {
                utility.set_visible_child_name("volume");
            });
        }
        {
            let utility = utility.clone();
            volume_close.connect_clicked(move |_| {
                utility.set_visible_child_name("panels");
            });
        }

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        controls.set_valign(gtk::Align::Center);
        controls.append(&more);
        controls.append(&utility);

        let main_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        main_row.set_valign(gtk::Align::Center);
        main_row.append(&transport);
        main_row.append(&playing);
        main_row.append(&controls);

        let bottom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bottom_row.set_halign(gtk::Align::Center);
        bottom_row.set_visible(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.set_height_request(62);
        root.set_valign(gtk::Align::Center);
        root.append(&main_row);
        root.append(&bottom_row);

        let hover = gtk::EventControllerMotion::new();
        let (elapsed_hover, total_hover) = (elapsed.clone(), total.clone());
        hover.connect_enter(move |_, _, _| {
            elapsed_hover.set_visible(true);
            total_hover.set_visible(true);
        });
        let (elapsed_hover, total_hover) = (elapsed.clone(), total.clone());
        hover.connect_leave(move |_| {
            elapsed_hover.set_visible(false);
            total_hover.set_visible(false);
        });
        timeline.add_controller(hover);

        PlayerBar {
            root,
            main_row,
            bottom_row,
            transport,
            controls,
            stacked: Cell::new(false),
            artwork_cache,
            artwork_slot,
            artwork_shown: RefCell::new(None),
            title,
            subtitle,
            status,
            previous,
            play_pause,
            next,
            elapsed,
            position,
            total,
            mute,
            volume,
            volume_button,
            volume_close,
            utility,
            shuffle,
            repeat,
            autoplay,
            radio,
            stop,
            lyrics,
            queue,
            seeking,
            updating,
        }
    }

    pub fn set_stacked(&self, stacked: bool) {
        if self.stacked.replace(stacked) == stacked {
            return;
        }
        if stacked {
            self.main_row.remove(&self.transport);
            self.main_row.remove(&self.controls);
            self.bottom_row.append(&self.transport);
            self.bottom_row.append(&self.controls);
            self.root.add_css_class("goosic-player-stacked");
        } else {
            self.bottom_row.remove(&self.transport);
            self.bottom_row.remove(&self.controls);
            self.main_row.prepend(&self.transport);
            self.main_row.append(&self.controls);
            self.root.remove_css_class("goosic-player-stacked");
        }
        self.bottom_row.set_visible(stacked);
    }

    pub fn close_volume(&self) -> bool {
        let open = self.utility.visible_child_name().as_deref() == Some("volume");
        self.utility.set_visible_child_name("panels");
        open
    }

    /// Redraws the bar from the player's state.
    pub fn update(&self, player: &Player, official_loaded: bool) {
        self.updating.set(true);
        let track = player.current.as_ref();
        self.title.set_markup(&format!(
            "<b>{}</b>",
            glib::markup_escape_text(if player.advertisement {
                "Advertisement"
            } else {
                track.map_or("Nothing playing", |t| t.title.as_str())
            })
        ));
        self.subtitle.set_label(&now_playing_subtitle(track));
        self.status.set_label(&player.status);
        self.status.set_visible(
            player.advertisement
                || player.watch.interruption().is_some()
                || ["Could not", "unavailable", "failed", "refused"]
                    .iter()
                    .any(|word| player.status.contains(word)),
        );
        self.root.set_tooltip_text(Some(&player.status));
        self.show_artwork(track.and_then(|track| track.thumbnail.clone()));

        let idle = player.transition() == PlaybackTransition::Idle;
        let playing = player.confirmed && !player.paused;
        self.play_pause.set_icon_name(if playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        self.play_pause
            .set_tooltip_text(Some(if playing { "Pause" } else { "Play" }));
        self.play_pause
            .set_sensitive(idle && (official_loaded || !player.queue.is_empty()));
        let can_skip = idle && !player.queue.is_empty() && !player.advertisement;
        self.previous.set_sensitive(can_skip);
        self.next.set_sensitive(can_skip);

        self.position.set_sensitive(is_seekable(
            player.duration,
            player.lease.owner,
            player.advertisement,
        ));
        if !self.seeking.get() {
            let shown =
                displayed_position(player.current_time, player.pending_seek, Instant::now());
            let total_secs = player.duration.max(1.0);
            let adjustment = self.position.adjustment();
            adjustment.set_upper(total_secs);
            adjustment.set_value(shown);
            self.elapsed.set_label(&time_text(shown));
            self.total.set_label(&if player.duration > 0.0 {
                format!("-{}", time_text((player.duration - shown).max(0.0)))
            } else {
                "--:--".to_owned()
            });
        }

        let pos = if player.muted {
            0.0
        } else {
            volume_position(player.volume)
        };
        self.volume.set_value(pos);
        self.volume.set_sensitive(!player.advertisement);
        let speaker_icon = if player.muted || player.volume <= 0.001 {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        self.mute.set_icon_name(speaker_icon);
        self.mute.set_sensitive(!player.advertisement);
        self.volume_button.set_icon_name(speaker_icon);
        self.volume_close.set_icon_name(speaker_icon);

        self.shuffle.set_active(player.shuffle);
        let (icon, tip) = match player.repeat {
            RepeatMode::Off => ("media-playlist-repeat-symbolic", "Repeat off"),
            RepeatMode::All => ("media-playlist-repeat-symbolic", "Repeat all"),
            RepeatMode::One => ("media-playlist-repeat-song-symbolic", "Repeat one"),
        };
        self.repeat.set_icon_name(icon);
        self.repeat.set_tooltip_text(Some(tip));
        if player.repeat == RepeatMode::Off {
            self.repeat.remove_css_class("goosic-control-active");
            self.repeat.add_css_class("goosic-subtle-control");
        } else {
            self.repeat.remove_css_class("goosic-subtle-control");
            self.repeat.add_css_class("goosic-control-active");
        }
        self.autoplay.set_active(player.autoplay);
        self.radio
            .set_sensitive(idle && player.current.is_some() && !player.radio_in_flight);
        self.stop
            .set_sensitive(idle && player.lease.owner != Owner::None);
        self.updating.set(false);
    }

    /// Reflects which side panels are open, without that counting as a click.
    pub fn set_panels(&self, lyrics: bool, queue: bool) {
        self.updating.set(true);
        self.lyrics.set_active(lyrics);
        self.queue.set_active(queue);
        self.updating.set(false);
    }

    fn show_artwork(&self, thumbnail: Option<String>) {
        if *self.artwork_shown.borrow() == thumbnail {
            return;
        }
        while let Some(child) = self.artwork_slot.first_child() {
            self.artwork_slot.remove(&child);
        }
        let frame = placeholder_artwork();
        if let Some(picture) = frame
            .child()
            .and_downcast::<gtk::Overlay>()
            .and_then(|overlay| overlay.last_child())
            .and_downcast::<gtk::Picture>()
        {
            self.artwork_cache.show_picture(thumbnail.as_deref(), &picture);
        }
        self.artwork_slot.append(&frame);
        *self.artwork_shown.borrow_mut() = thumbnail;
    }
}

pub fn build_more_menu(
    actions: &Rc<BarActions>,
    autoplay: &gtk::ToggleButton,
    radio: &gtk::Button,
    stop: &gtk::Button,
    mute: &gtk::Button,
) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    popover.add_css_class("goosic-popover");
    let extra = gtk::Box::new(gtk::Orientation::Vertical, 4);
    extra.set_margin_top(6);
    extra.set_margin_bottom(6);
    extra.set_margin_start(6);
    extra.set_margin_end(6);

    let full = gtk::Button::with_label("Full-Screen Player");
    {
        let (actions, close) = (actions.clone(), popover.clone());
        full.connect_clicked(move |_| {
            close.popdown();
            (actions.expand)();
        });
    }
    extra.append(&full);
    let mini = gtk::Button::with_label("Mini Player");
    {
        let (actions, close) = (actions.clone(), popover.clone());
        mini.connect_clicked(move |_| {
            close.popdown();
            (actions.mini_player)();
        });
    }
    extra.append(&mini);
    let import = gtk::Button::with_label("Import Timed Lyrics…");
    {
        let (actions, close) = (actions.clone(), popover.clone());
        import.connect_clicked(move |_| {
            close.popdown();
            (actions.import_lyrics)();
        });
    }
    extra.append(&import);
    for (label, is_lyrics) in [("Lyrics", true), ("Up Next", false)] {
        let button = gtk::Button::with_label(label);
        let (actions, close) = (actions.clone(), popover.clone());
        button.connect_clicked(move |_| {
            close.popdown();
            if is_lyrics {
                (actions.lyrics)();
            } else {
                (actions.queue)();
            }
        });
        extra.append(&button);
    }
    extra.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    extra.append(radio);
    extra.append(autoplay);
    extra.append(mute);
    extra.append(stop);
    extra.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let sleep_label = gtk::Label::builder()
        .label("Sleep timer")
        .xalign(0.0)
        .margin_start(8)
        .margin_top(4)
        .build();
    sleep_label.add_css_class("dim-label");
    extra.append(&sleep_label);
    for (label, minutes) in [
        ("15 minutes", 15),
        ("30 minutes", 30),
        ("45 minutes", 45),
        ("60 minutes", 60),
        ("End of this song", -1),
        ("Turn off sleep timer", 0),
    ] {
        let button = gtk::Button::with_label(label);
        let (actions, close) = (actions.clone(), popover.clone());
        button.connect_clicked(move |_| {
            close.popdown();
            (actions.sleep)(minutes);
        });
        extra.append(&button);
    }
    popover.set_child(Some(&extra));
    gtk::MenuButton::builder()
        .icon_name("view-more-horizontal-symbolic")
        .tooltip_text("More playback controls")
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build()
}

/// A note in a rounded frame, with a cover-fitted picture laid over it.
fn placeholder_artwork() -> gtk::Frame {
    let note = gtk::Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(16)
        .build();
    note.add_css_class("dim-label");
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .width_request(ARTWORK_SIZE)
        .height_request(ARTWORK_SIZE)
        .build();
    let overlay = gtk::Overlay::builder().child(&note).build();
    overlay.add_overlay(&picture);
    let frame = gtk::Frame::builder()
        .child(&overlay)
        .width_request(ARTWORK_SIZE)
        .height_request(ARTWORK_SIZE)
        .valign(gtk::Align::Center)
        .build();
    frame.set_overflow(gtk::Overflow::Hidden);
    frame
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build()
}

fn single_line(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build()
}

fn connect(button: &gtk::Button, actions: &Rc<BarActions>, action: fn(&BarActions)) {
    let actions = actions.clone();
    button.connect_clicked(move |_| action(&actions));
}

fn connect_toggle(
    toggle: &gtk::ToggleButton,
    actions: &Rc<BarActions>,
    updating: &Rc<Cell<bool>>,
    action: fn(&BarActions),
) {
    let (actions, updating) = (actions.clone(), updating.clone());
    toggle.connect_toggled(move |_| {
        if !updating.get() {
            action(&actions);
        }
    });
}
