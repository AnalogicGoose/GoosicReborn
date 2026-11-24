//! Compact Mini Player window, matching `NativeMacMiniPlayer`: the Full Player's visual language
//! (animated 6x6 mesh backdrop, frosted glass capsules, cover step-back on pause, audio-taper
//! volume) in a 320x470 window that closes back into hiding without interrupting playback.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use goosic_shell_support::lyrics_motion::PAUSED_COVER_SCALE;
use goosic_shell_support::navigation::PlaybackTransition;
use goosic_shell_support::playback::{
    displayed_position, is_seekable, now_playing_subtitle, time_text, volume_gain, volume_position,
};
use gtk::glib;
use gtk::prelude::*;

use crate::artwork::ArtworkCache;
use crate::glass::{GlassGroup, GlassKind};
use crate::mesh::MeshBackdrop;
use crate::playback::Player;
use crate::player_bar::BarActions;

const SEEK_SETTLE: Duration = Duration::from_millis(200);
const BASE_COVER: i32 = 168;

pub struct MiniPlayer {
    pub window: gtk::Window,
    mesh: MeshBackdrop,
    glass: GlassGroup,
    stack: gtk::Stack,
    cover_slot: gtk::Box,
    title: gtk::Label,
    subtitle: gtk::Label,
    like_button: gtk::Button,
    elapsed: gtk::Label,
    remaining: gtk::Label,
    position: gtk::Scale,
    previous: gtk::Button,
    play_pause: gtk::Button,
    next: gtk::Button,
    volume: gtk::Scale,
    mute: gtk::Button,
    cache: Rc<ArtworkCache>,
    shown: RefCell<Option<String>>,
    seeking: Rc<Cell<bool>>,
    updating: Rc<Cell<bool>>,
    paused_visual: Cell<bool>,
}

impl MiniPlayer {
    pub fn new(
        app: &gtk::Application,
        actions: Rc<BarActions>,
        cache: Rc<ArtworkCache>,
        return_to_main: impl Fn() + 'static,
    ) -> Self {
        let seeking = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));
        let mesh = MeshBackdrop::new(cache.clone());
        let glass = GlassGroup::new();
        {
            let glass = glass.clone();
            mesh.widget.set_on_tick(Rc::new(move || glass.invalidate()));
        }
        let underlay: gtk::Widget = mesh.widget.clone().upcast();

        let window = gtk::Window::builder()
            .application(app)
            .title("Mini Player")
            .default_width(320)
            .default_height(470)
            .hide_on_close(true)
            .build();
        window.set_size_request(260, 380);
        window.add_css_class("goosic-shell");
        window.add_css_class("goosic-immersive");
        window.add_css_class("goosic-mini-player");

        // Top controls: Back to Goosic + Full-Screen Player in a frosted capsule.
        let top_capsule_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        top_capsule_box.add_css_class("goosic-glass-capsule");
        let back_btn = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Goosic")
            .build();
        {
            let win = window.downgrade();
            back_btn.connect_clicked(move |_| {
                if let Some(w) = win.upgrade() {
                    w.set_visible(false);
                }
                return_to_main();
            });
        }
        let full_btn = gtk::Button::builder()
            .icon_name("view-fullscreen-symbolic")
            .tooltip_text("Full-Screen Player")
            .build();
        {
            let actions = actions.clone();
            let win = window.downgrade();
            full_btn.connect_clicked(move |_| {
                if let Some(w) = win.upgrade() {
                    w.set_visible(false);
                }
                (actions.expand)();
            });
        }
        top_capsule_box.append(&back_btn);
        top_capsule_box.append(&full_btn);
        let top_capsule = glass.wrap(&top_capsule_box, GlassKind::Capsule, Some(&underlay));
        top_capsule.set_halign(gtk::Align::Start);

        let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top_row.append(&top_capsule);

        // Cover art with paused step-back.
        let cover_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cover_slot.set_halign(gtk::Align::Center);
        cover_slot.set_valign(gtk::Align::Center);
        cover_slot.set_size_request(BASE_COVER, BASE_COVER);
        cover_slot.set_overflow(gtk::Overflow::Hidden);
        cover_slot.add_css_class("goosic-full-cover");
        cover_slot.append(&placeholder_cover(BASE_COVER));

        // Track metadata + frosted circular Like & More buttons.
        let title = gtk::Label::builder()
            .label("Nothing playing")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        title.add_css_class("goosic-mini-title");

        let subtitle = gtk::Label::builder()
            .label("Choose a song in Goosic")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("goosic-mini-subtitle");

        let text_col = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_col.set_hexpand(true);
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
        let like_glass = glass.wrap(&like_button, GlassKind::Circle, Some(&underlay));
        like_glass.set_valign(gtk::Align::Center);

        let meta_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        meta_row.append(&text_col);
        meta_row.append(&like_glass);

        // Timeline.
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
        let times = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        times.append(&elapsed);
        times.append(&remaining);
        let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        progress_box.append(&position);
        progress_box.append(&times);

        // Monochrome transport.
        let previous = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-skip-backward-symbolic")
                    .pixel_size(18)
                    .build(),
            )
            .tooltip_text("Previous")
            .build();
        let play_pause = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-playback-start-symbolic")
                    .pixel_size(24)
                    .build(),
            )
            .tooltip_text("Play")
            .build();
        play_pause.add_css_class("goosic-full-play-btn");
        let next = gtk::Button::builder()
            .child(
                &gtk::Image::builder()
                    .icon_name("media-skip-forward-symbolic")
                    .pixel_size(18)
                    .build(),
            )
            .tooltip_text("Next")
            .build();
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
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        transport.set_homogeneous(true);
        transport.add_css_class("goosic-full-transport");
        transport.append(&previous);
        transport.append(&play_pause);
        transport.append(&next);

        // Frosted audio-taper volume capsule.
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_hexpand(true);
        volume.add_css_class("goosic-volume");
        {
            let (actions, updating) = (actions.clone(), updating.clone());
            volume.connect_change_value(move |_, _, value| {
                if !updating.get() {
                    (actions.volume)(volume_gain(value.clamp(0.0, 1.0)));
                }
                glib::Propagation::Proceed
            });
        }
        let mute = gtk::Button::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Mute")
            .build();
        {
            let actions = actions.clone();
            mute.connect_clicked(move |_| (actions.mute)());
        }
        let vol_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        vol_box.add_css_class("goosic-glass-capsule");
        vol_box.add_css_class("goosic-full-volume-capsule");
        vol_box.append(&volume);
        vol_box.append(&mute);
        let vol_glass = glass.wrap(&vol_box, GlassKind::Capsule, Some(&underlay));

        let playing_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
        playing_box.add_css_class("goosic-full-player");
        playing_box.set_margin_top(12);
        playing_box.set_margin_bottom(14);
        playing_box.set_margin_start(16);
        playing_box.set_margin_end(16);
        playing_box.append(&top_row);
        playing_box.append(&cover_slot);
        playing_box.append(&meta_row);
        playing_box.append(&progress_box);
        playing_box.append(&transport);
        playing_box.append(&vol_glass);

        // Empty state when nothing is playing.
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 10);
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        empty.set_margin_start(24);
        empty.set_margin_end(24);
        let empty_icon = gtk::Image::builder()
            .icon_name("audio-x-generic-symbolic")
            .pixel_size(44)
            .build();
        empty_icon.add_css_class("dim-label");
        let empty_title = gtk::Label::builder().label("Nothing playing").build();
        empty_title.add_css_class("goosic-mini-title");
        let empty_desc = gtk::Label::builder()
            .label("Choose a song in Goosic to see it here.")
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        empty_desc.add_css_class("dim-label");
        empty.append(&empty_icon);
        empty.append(&empty_title);
        empty.append(&empty_desc);

        let stack = gtk::Stack::new();
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&playing_box, Some("playing"));
        stack.set_visible_child_name("empty");

        let root = gtk::Overlay::new();
        root.set_child(Some(&mesh.widget));
        root.add_overlay(&stack);
        window.set_child(Some(&root));

        Self {
            window,
            mesh,
            glass,
            stack,
            cover_slot,
            title,
            subtitle,
            like_button,
            elapsed,
            remaining,
            position,
            previous,
            play_pause,
            next,
            volume,
            mute,
            cache,
            shown: RefCell::new(None),
            seeking,
            updating,
            paused_visual: Cell::new(false),
        }
    }

    pub fn present(&self) {
        self.window.present();
        self.glass.invalidate();
    }

    pub fn set_appearance(&self, reduce_motion: bool, high_contrast: bool) {
        self.mesh.set_options(reduce_motion, high_contrast);
        self.glass.set_appearance(false, high_contrast);
        if high_contrast {
            self.window.add_css_class("goosic-high-contrast");
        } else {
            self.window.remove_css_class("goosic-high-contrast");
        }
    }

    pub fn update(&self, player: &Player, loaded: bool) {
        self.updating.set(true);
        let track = player.current.as_ref();
        self.stack
            .set_visible_child_name(if track.is_some() { "playing" } else { "empty" });

        self.title.set_label(if player.advertisement {
            "Advertisement"
        } else {
            track.map_or("Nothing playing", |t| t.title.as_str())
        });
        self.subtitle.set_label(&now_playing_subtitle(track));
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

        let remote = track.and_then(|t| t.thumbnail.as_deref());
        self.mesh.update(remote);
        if self.shown.borrow().as_deref() != remote {
            *self.shown.borrow_mut() = remote.map(str::to_owned);
            while let Some(child) = self.cover_slot.first_child() {
                self.cover_slot.remove(&child);
            }
            let overlay = placeholder_cover(BASE_COVER);
            if let Some(picture) = overlay.last_child().and_downcast::<gtk::Picture>() {
                self.cache.show_high_res_picture(remote, &picture);
            }
            self.cover_slot.append(&overlay);
            self.apply_cover_size();
        }
        self.updating.set(false);
    }

    fn apply_cover_size(&self) {
        let visual = if self.paused_visual.get() {
            ((f64::from(BASE_COVER) * PAUSED_COVER_SCALE).round() as i32).max(1)
        } else {
            BASE_COVER
        };
        let pad = (BASE_COVER - visual) / 2;
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
}

fn placeholder_cover(size: i32) -> gtk::Overlay {
    let icon = gtk::Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(48)
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
