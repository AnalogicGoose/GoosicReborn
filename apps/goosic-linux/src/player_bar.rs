//! The bar along the bottom of the window: what is playing, and the controls for it.
//!
//! It shows only what the player confirmed. A play request does not turn the button into Pause;
//! a validated report from the renderer does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use goosic_protocol::Owner;
use goosic_shell_support::navigation::{PlaybackTransition, RepeatMode};
use goosic_shell_support::playback::{
    displayed_position, is_seekable, now_playing_subtitle, time_text,
};
use gtk::glib;
use gtk::prelude::*;

use crate::playback::Player;

/// A drag on the position slider becomes one seek once the pointer rests this long, rather than a
/// seek per pixel.
const SEEK_SETTLE: Duration = Duration::from_millis(200);

/// What the bar can ask the shell to do.
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
}

pub struct PlayerBar {
    pub root: gtk::Box,
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
    shuffle: gtk::ToggleButton,
    repeat: gtk::Button,
    autoplay: gtk::ToggleButton,
    radio: gtk::Button,
    stop: gtk::Button,
    /// True while a drag on the position slider has not settled, so reports do not pull it back.
    seeking: Rc<Cell<bool>>,
    /// True while `update` is writing to the toggles, so their signals are not mistaken for clicks.
    updating: Rc<Cell<bool>>,
}

impl PlayerBar {
    pub fn new(actions: BarActions) -> PlayerBar {
        let actions = Rc::new(actions);
        let seeking = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));

        let title = single_line("Nothing playing");
        title.set_markup("<b>Nothing playing</b>");
        let subtitle = single_line("Choose a track to begin");
        subtitle.add_css_class("dim-label");
        let status = single_line("");
        status.add_css_class("dim-label");

        let previous = icon_button("media-skip-backward-symbolic", "Previous");
        let play_pause = icon_button("media-playback-start-symbolic", "Play");
        let next = icon_button("media-skip-forward-symbolic", "Next");
        let mute = icon_button("audio-volume-high-symbolic", "Mute");
        let repeat = icon_button("media-playlist-repeat-symbolic", "Repeat off");
        let stop = icon_button("media-playback-stop-symbolic", "Stop and release playback");
        let radio = gtk::Button::with_label("Radio");
        radio.set_tooltip_text(Some("Start a radio from what is playing"));
        let shuffle = gtk::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .tooltip_text("Shuffle")
            .build();
        let autoplay = gtk::ToggleButton::with_label("Autoplay");
        autoplay.set_tooltip_text(Some("Keep playing when the queue runs out"));

        connect(&previous, &actions, |a| (a.previous)());
        connect(&play_pause, &actions, |a| (a.toggle_pause)());
        connect(&next, &actions, |a| (a.next)());
        connect(&mute, &actions, |a| (a.mute)());
        connect(&repeat, &actions, |a| (a.repeat)());
        connect(&stop, &actions, |a| (a.stop)());
        connect(&radio, &actions, |a| (a.radio)());
        connect_toggle(&shuffle, &actions, &updating, |a| (a.shuffle)());
        connect_toggle(&autoplay, &actions, &updating, |a| (a.autoplay)());

        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        position.set_draw_value(false);
        position.set_hexpand(true);
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

        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_width_request(110);
        {
            let actions = actions.clone();
            volume.connect_change_value(move |_, _, value| {
                (actions.volume)(value);
                glib::Propagation::Proceed
            });
        }

        let elapsed = gtk::Label::new(Some("0:00"));
        elapsed.add_css_class("dim-label");
        let total = gtk::Label::new(Some("--:--"));
        total.add_css_class("dim-label");

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.append(&title);
        text.append(&subtitle);
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        top.append(&text);
        for widget in [&previous, &play_pause, &next] {
            top.append(widget);
        }
        top.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        top.append(&mute);
        top.append(&volume);

        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bottom.append(&elapsed);
        bottom.append(&position);
        bottom.append(&total);
        bottom.append(&shuffle);
        bottom.append(&repeat);
        bottom.append(&autoplay);
        bottom.append(&radio);
        bottom.append(&stop);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.set_margin_top(10);
        root.set_margin_bottom(10);
        root.set_margin_start(24);
        root.set_margin_end(24);
        root.append(&top);
        root.append(&bottom);
        root.append(&status);

        PlayerBar {
            root,
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
            shuffle,
            repeat,
            autoplay,
            radio,
            stop,
            seeking,
            updating,
        }
    }

    /// Redraws the bar from the player's state.
    pub fn update(&self, player: &Player, official_loaded: bool) {
        self.updating.set(true);
        let track = player.current.as_ref();
        self.title.set_markup(&format!(
            "<b>{}</b>",
            glib::markup_escape_text(track.map_or("Nothing playing", |t| t.title.as_str()))
        ));
        self.subtitle.set_label(&now_playing_subtitle(track));
        self.status.set_label(&player.status);

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
            let adjustment = self.position.adjustment();
            adjustment.set_upper(player.duration.max(1.0));
            adjustment.set_value(shown);
            self.elapsed.set_label(&time_text(shown));
        }
        self.total.set_label(&if player.duration > 0.0 {
            time_text(player.duration)
        } else {
            "--:--".to_owned()
        });

        self.volume
            .set_value(if player.muted { 0.0 } else { player.volume });
        self.volume.set_sensitive(!player.advertisement);
        self.mute.set_icon_name(if player.muted {
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
            self.repeat.add_css_class("dim-label");
        } else {
            self.repeat.remove_css_class("dim-label");
        }
        self.autoplay.set_active(player.autoplay);
        self.radio
            .set_sensitive(idle && player.current.is_some() && !player.radio_in_flight);
        self.stop
            .set_sensitive(idle && player.lease.owner != Owner::None);
        self.updating.set(false);
    }
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
