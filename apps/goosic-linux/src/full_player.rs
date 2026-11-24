//! Full-window immersive playback presentation, matching `NativeMacFullPlayer.swift`:
//! top-leading close/fullscreen glass capsule, top-trailing volume glass capsule,
//! bottom-trailing Lyrics/Up Next glass capsule, large rounded cover with paused step-back,
//! circular Like/More actions beside title and subtitle, clean white timeline, and monochrome
//! transport over a dark saturated artwork field.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use goosic_shell_support::navigation::{PlaybackTransition, RepeatMode};
use goosic_shell_support::playback::{
    displayed_position, is_seekable, now_playing_subtitle, time_text, volume_gain, volume_position,
};
use gtk::glib;
use gtk::prelude::*;

use crate::artwork::ArtworkCache;
use crate::layout;
use crate::playback::Player;
use crate::player_bar::BarActions;
use crate::side_panels::LyricsPanel;

const SEEK_SETTLE: Duration = Duration::from_millis(200);

pub struct FullPlayer {
    pub root: gtk::Overlay,
    pub lyrics: LyricsPanel,
    container: gtk::Box,
    body: gtk::Box,
    cover_column: gtk::Box,
    cover_slot: gtk::Box,
    cover_button: gtk::Button,
    title: gtk::Label,
    subtitle: gtk::Label,
    like_button: gtk::Button,
    elapsed: gtk::Label,
    remaining: gtk::Label,
    position: gtk::Scale,
    shuffle: gtk::ToggleButton,
    previous: gtk::Button,
    play_pause: gtk::Button,
    next: gtk::Button,
    repeat: gtk::Button,
    volume: gtk::Scale,
    mute: gtk::Button,
    lyrics_toggle: gtk::ToggleButton,
    queue_toggle: gtk::ToggleButton,
    cache: Rc<ArtworkCache>,
    shown: RefCell<Option<String>>,
    queue: RefCell<Option<gtk::Box>>,
    seeking: Rc<Cell<bool>>,
    updating: Rc<Cell<bool>>,
    paused_visual: Cell<bool>,
    base_cover_size: Cell<i32>,
}

impl FullPlayer {
    pub fn new(
        actions: Rc<BarActions>,
        cache: Rc<ArtworkCache>,
        close: impl Fn() + 'static,
        fullscreen: impl Fn() + 'static,
    ) -> Self {
        let seeking = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));

        let container = gtk::Box::new(gtk::Orientation::Vertical, 16);
        container.add_css_class("goosic-full-player");
        container.set_hexpand(true);
        container.set_vexpand(true);

        // Top chrome: leading close + fullscreen glass capsule, trailing volume glass capsule.
        let top_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_bar.set_valign(gtk::Align::Start);

        let leading_capsule = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        leading_capsule.add_css_class("goosic-glass-capsule");
        let close_btn = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text("Close full player (Esc)")
            .build();
        let close_for_cover = Rc::new(close);
        {
            let close_cb = close_for_cover.clone();
            close_btn.connect_clicked(move |_| close_cb());
        }
        let full_btn = gtk::Button::builder()
            .icon_name("view-fullscreen-symbolic")
            .tooltip_text("Toggle full screen (F11)")
            .build();
        full_btn.connect_clicked(move |_| fullscreen());
        leading_capsule.append(&close_btn);
        leading_capsule.append(&full_btn);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_width_request(126);
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
        let mute = gtk::Button::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Mute (Ctrl+M)")
            .valign(gtk::Align::Center)
            .build();
        {
            let actions = actions.clone();
            mute.connect_clicked(move |_| (actions.mute)());
        }
        let volume_capsule = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        volume_capsule.add_css_class("goosic-glass-capsule");
        volume_capsule.add_css_class("goosic-full-volume-capsule");
        volume_capsule.append(&volume);
        volume_capsule.append(&mute);

        top_bar.append(&leading_capsule);
        top_bar.append(&spacer);
        top_bar.append(&volume_capsule);
        container.append(&top_bar);

        // Cover art — clickable to exit full player, matching macOS.
        let cover_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cover_slot.set_halign(gtk::Align::Center);
        cover_slot.set_valign(gtk::Align::Center);
        cover_slot.add_css_class("goosic-full-cover");
        cover_slot.set_overflow(gtk::Overflow::Hidden);
        cover_slot.append(&placeholder_cover(360));

        let cover_button = gtk::Button::builder()
            .child(&cover_slot)
            .halign(gtk::Align::Center)
            .tooltip_text("Close full player (Esc)")
            .build();
        cover_button.add_css_class("goosic-full-cover-button");
        {
            let close_cb = close_for_cover.clone();
            cover_button.connect_clicked(move |_| close_cb());
        }

        // Track title + "Artist — Album" on left, circular glass Like + More on right.
        let title = gtk::Label::builder()
            .label("Nothing playing")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        title.add_css_class("goosic-full-title");

        let subtitle = gtk::Label::builder()
            .label("Choose a track to begin")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("goosic-full-subtitle");

        let text_col = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_col.set_hexpand(true);
        text_col.set_valign(gtk::Align::Center);
        text_col.append(&title);
        text_col.append(&subtitle);

        let like_button = gtk::Button::builder()
            .icon_name("emote-love-symbolic")
            .tooltip_text("Like")
            .valign(gtk::Align::Center)
            .build();
        like_button.add_css_class("goosic-glass-circle");
        {
            let actions = actions.clone();
            like_button.connect_clicked(move |_| (actions.like_current)());
        }

        let more_menu = build_full_more(&actions);
        more_menu.add_css_class("goosic-glass-circle-menu");

        let meta_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        meta_row.set_hexpand(true);
        meta_row.append(&text_col);
        meta_row.append(&like_button);
        meta_row.append(&more_menu);

        // Progress scrubber + elapsed / -remaining.
        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        position.set_draw_value(false);
        position.set_hexpand(true);
        position.add_css_class("goosic-full-progress");
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
        let elapsed = gtk::Label::builder().label("0:00").xalign(0.0).build();
        elapsed.add_css_class("goosic-time");
        let remaining = gtk::Label::builder()
            .label("-0:00")
            .xalign(1.0)
            .hexpand(true)
            .build();
        remaining.add_css_class("goosic-time");
        let times = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        times.append(&elapsed);
        times.append(&remaining);

        let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        progress_box.append(&position);
        progress_box.append(&times);

        // Monochrome transport row: Shuffle · Prev · Play/Pause · Next · Repeat.
        let shuffle = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .tooltip_text("Shuffle")
            .valign(gtk::Align::Center)
            .build();
        shuffle.add_css_class("goosic-full-transport-btn");
        let previous = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-skip-backward-symbolic")
                    .pixel_size(20)
                    .build(),
            )
            .tooltip_text("Previous")
            .valign(gtk::Align::Center)
            .build();
        previous.add_css_class("goosic-full-transport-btn");
        let play_pause = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-playback-start-symbolic")
                    .pixel_size(28)
                    .build(),
            )
            .tooltip_text("Play")
            .valign(gtk::Align::Center)
            .build();
        play_pause.add_css_class("goosic-full-play-btn");
        let next = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-skip-forward-symbolic")
                    .pixel_size(20)
                    .build(),
            )
            .tooltip_text("Next")
            .valign(gtk::Align::Center)
            .build();
        next.add_css_class("goosic-full-transport-btn");
        let repeat = gtk::Button::builder()
            .icon_name("media-playlist-repeat-symbolic")
            .tooltip_text("Repeat off")
            .valign(gtk::Align::Center)
            .build();
        repeat.add_css_class("goosic-full-transport-btn");

        {
            let (actions, updating) = (actions.clone(), updating.clone());
            shuffle.connect_toggled(move |_| {
                if !updating.get() {
                    (actions.shuffle)();
                }
            });
        }
        {
            let actions = actions.clone();
            previous.connect_clicked(move |_| (actions.previous)());
        }
        {
            let actions = actions.clone();
            play_pause.connect_clicked(move |_| (actions.toggle_pause)());
        }
        {
            let actions = actions.clone();
            next.connect_clicked(move |_| (actions.next)());
        }
        {
            let actions = actions.clone();
            repeat.connect_clicked(move |_| (actions.repeat)());
        }

        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        transport.set_homogeneous(true);
        transport.set_hexpand(true);
        transport.add_css_class("goosic-full-transport");
        transport.append(&shuffle);
        transport.append(&previous);
        transport.append(&play_pause);
        transport.append(&next);
        transport.append(&repeat);

        let controls_col = gtk::Box::new(gtk::Orientation::Vertical, 12);
        controls_col.set_halign(gtk::Align::Center);
        controls_col.set_width_request(360);
        controls_col.append(&meta_row);
        controls_col.append(&progress_box);
        controls_col.append(&transport);

        let cover_column = gtk::Box::new(gtk::Orientation::Vertical, 18);
        cover_column.set_hexpand(true);
        cover_column.set_valign(gtk::Align::Center);
        cover_column.set_halign(gtk::Align::Center);
        cover_column.append(&cover_button);
        cover_column.append(&controls_col);

        let lyrics = LyricsPanel::new({
            let actions = actions.clone();
            move |pos| (actions.seek)(pos)
        });
        lyrics.set_immersive(true);
        lyrics.root.add_css_class("goosic-full-lyrics");
        lyrics.root.set_hexpand(true);
        lyrics.root.set_vexpand(true);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 48);
        body.set_vexpand(true);
        body.set_hexpand(true);
        body.append(&cover_column);
        body.append(&lyrics.root);
        container.append(&body);

        // Bottom-trailing glass capsule for switching Lyrics / Up Next (macOS `paneButton`).
        let lyrics_toggle = gtk::ToggleButton::builder()
            .icon_name("chat-message-new-symbolic")
            .tooltip_text("Lyrics")
            .valign(gtk::Align::Center)
            .build();
        lyrics_toggle.add_css_class("goosic-pane-toggle");
        let queue_toggle = gtk::ToggleButton::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Up Next")
            .valign(gtk::Align::Center)
            .build();
        queue_toggle.add_css_class("goosic-pane-toggle");
        {
            let (actions, updating) = (actions.clone(), updating.clone());
            lyrics_toggle.connect_toggled(move |_| {
                if !updating.get() {
                    (actions.lyrics)();
                }
            });
        }
        {
            let (actions, updating) = (actions.clone(), updating.clone());
            queue_toggle.connect_toggled(move |_| {
                if !updating.get() {
                    (actions.queue)();
                }
            });
        }
        let pane_capsule = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        pane_capsule.add_css_class("goosic-glass-capsule");
        pane_capsule.add_css_class("goosic-pane-capsule");
        pane_capsule.set_halign(gtk::Align::End);
        pane_capsule.set_valign(gtk::Align::End);
        pane_capsule.set_margin_end(20);
        pane_capsule.set_margin_bottom(18);
        pane_capsule.append(&lyrics_toggle);
        pane_capsule.append(&queue_toggle);

        let root = gtk::Overlay::new();
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_visible(false);
        root.set_child(Some(&container));
        root.add_overlay(&pane_capsule);

        Self {
            root,
            lyrics,
            container,
            body,
            cover_column,
            cover_slot,
            cover_button,
            title,
            subtitle,
            like_button,
            elapsed,
            remaining,
            position,
            shuffle,
            previous,
            play_pause,
            next,
            repeat,
            volume,
            mute,
            lyrics_toggle,
            queue_toggle,
            cache,
            shown: RefCell::new(None),
            queue: RefCell::new(None),
            seeking,
            updating,
            paused_visual: Cell::new(false),
            base_cover_size: Cell::new(360),
        }
    }

    pub fn mount_queue(&self, panel: &gtk::Box) {
        if panel.parent().as_ref() != Some(self.body.upcast_ref()) {
            if let Some(parent) = panel.parent().and_downcast::<gtk::Box>() {
                parent.remove(panel);
            }
            self.body.append(panel);
        }
        *self.queue.borrow_mut() = Some(panel.clone());
    }

    pub fn set_panels(&self, lyrics: bool, queue: bool) {
        self.updating.set(true);
        self.lyrics_toggle.set_active(lyrics);
        self.queue_toggle.set_active(queue);
        self.updating.set(false);
    }

    pub fn layout(&self, width: i32, height: i32, lyrics: bool, queue: bool) {
        let auxiliary = lyrics || queue;
        let split = width >= 880 && auxiliary;
        self.body.set_orientation(if split {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        });
        self.body.set_spacing(if split { 48 } else { 14 });
        let h_margin = if width >= 900 { 44 } else { 20 };
        self.container.set_margin_start(h_margin);
        self.container.set_margin_end(h_margin);
        self.container.set_margin_top(18);
        self.container.set_margin_bottom(68);
        self.lyrics.root.set_visible(lyrics);

        let size = layout::cover_size(if split { width / 2 } else { width }, height);
        self.base_cover_size.set(size);
        self.cover_button.set_visible(!auxiliary || split);
        self.apply_cover_size();

        self.cover_column.set_valign(if auxiliary && !split {
            gtk::Align::End
        } else {
            gtk::Align::Center
        });
        let trailing = if queue {
            self.queue.borrow().clone()
        } else {
            Some(self.lyrics.root.clone())
        };
        self.body.reorder_child_after(
            &self.cover_column,
            if auxiliary && !split {
                trailing.as_ref()
            } else {
                None
            },
        );
        self.cover_column.set_vexpand(!auxiliary || split);
    }

    fn apply_cover_size(&self) {
        let base = self.base_cover_size.get().max(1);
        // Step the artwork back while paused, keeping the metadata and transport stationary.
        let visual = if self.paused_visual.get() {
            ((f64::from(base) * 0.80).round() as i32).max(1)
        } else {
            base
        };
        let pad = (base - visual) / 2;
        self.cover_slot.set_size_request(visual, visual);
        self.cover_slot.set_margin_top(pad);
        self.cover_slot.set_margin_bottom(pad);
        self.cover_slot.set_margin_start(pad);
        self.cover_slot.set_margin_end(pad);
        if let Some(overlay) = self.cover_slot.first_child().and_downcast::<gtk::Overlay>() {
            if let Some(picture) = overlay.last_child().and_downcast::<gtk::Picture>() {
                picture.set_size_request(visual, visual);
            }
        }
    }

    pub fn update(&self, player: &Player, loaded: bool) {
        self.updating.set(true);
        let track = player.current.as_ref();
        self.title.set_label(if player.advertisement {
            "Advertisement"
        } else {
            track.map_or("Nothing playing", |t| t.title.as_str())
        });
        let full_sub = match track {
            Some(track) if !track.artist.is_empty() && !track.album.is_empty() => {
                format!("{} — {}", track.artist, track.album)
            }
            _ => now_playing_subtitle(track),
        };
        self.subtitle.set_label(&full_sub);
        self.like_button.set_sensitive(track.is_some());

        let idle = player.transition() == PlaybackTransition::Idle;
        let playing = player.confirmed && !player.paused;
        let paused_state = player.confirmed && player.paused;
        if self.paused_visual.replace(paused_state) != paused_state {
            self.apply_cover_size();
        }
        if let Some(image) = self.play_pause.child().and_downcast::<gtk::Image>() {
            image.set_icon_name(Some(if playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            }));
        }
        self.play_pause
            .set_tooltip_text(Some(if playing { "Pause" } else { "Play" }));
        self.play_pause
            .set_sensitive(idle && (loaded || !player.queue.is_empty()));
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
            let total = player.duration.max(1.0);
            let adj = self.position.adjustment();
            adj.set_upper(total);
            adj.set_value(shown);
            self.elapsed.set_label(&time_text(shown));
            self.remaining.set_label(&if player.duration > 0.0 {
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
        self.mute.set_icon_name(if player.muted || player.volume <= 0.001 {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        });
        self.mute.set_sensitive(!player.advertisement);

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
        } else {
            self.repeat.add_css_class("goosic-control-active");
        }

        let remote = track.and_then(|track| track.thumbnail.as_deref());
        if self.shown.borrow().as_deref() != remote {
            *self.shown.borrow_mut() = remote.map(str::to_owned);
            while let Some(child) = self.cover_slot.first_child() {
                self.cover_slot.remove(&child);
            }
            let size = self.base_cover_size.get().max(1);
            let overlay = placeholder_cover(size);
            if let Some(picture) = overlay.last_child().and_downcast::<gtk::Picture>() {
                self.cache.show_picture(remote, &picture);
            }
            self.cover_slot.append(&overlay);
            self.apply_cover_size();
        }
        self.updating.set(false);
    }
}

fn placeholder_cover(size: i32) -> gtk::Overlay {
    let icon = gtk::Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(64)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    icon.add_css_class("dim-label");
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .width_request(size)
        .height_request(size)
        .build();
    let overlay = gtk::Overlay::builder().child(&icon).build();
    overlay.add_overlay(&picture);
    overlay
}

fn build_full_more(actions: &Rc<BarActions>) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    popover.add_css_class("goosic-popover");
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    list.set_margin_top(6);
    list.set_margin_bottom(6);
    list.set_margin_start(6);
    list.set_margin_end(6);
    for (label, action) in [
        ("Start radio", 0),
        ("Stop playback", 1),
        ("Sleep: 15 minutes", 15),
        ("Sleep: 30 minutes", 30),
        ("Sleep: End of this song", -1),
        ("Turn off sleep timer", -2),
    ] {
        let button = gtk::Button::with_label(label);
        let (actions, close) = (actions.clone(), popover.clone());
        button.connect_clicked(move |_| {
            close.popdown();
            match action {
                0 => (actions.radio)(),
                1 => (actions.stop)(),
                -2 => (actions.sleep)(0),
                minutes => (actions.sleep)(minutes),
            }
        });
        list.append(&button);
    }
    popover.set_child(Some(&list));
    gtk::MenuButton::builder()
        .icon_name("view-more-horizontal-symbolic")
        .tooltip_text("More playback actions")
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build()
}
