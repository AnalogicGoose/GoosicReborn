//! Goosic on MPRIS, the D-Bus interface Linux desktops use for media keys, panel widgets and
//! `playerctl`.
//!
//! It decides nothing. What it shows is `goosic-shell-support`'s projection of confirmed state, and
//! every command that arrives is checked against the command availability recomputed from a
//! snapshot taken at that moment before it reaches the shell: the bus is another caller, not
//! another authority, and a panel cannot ask for a transition the app itself would refuse.
//!
//! Two rules follow from MPRIS being a bus interface rather than a system API. A method is declared
//! only when Goosic can honour it, because a declared method that refuses at runtime is a dead
//! button in every panel on the desktop. And `Position` is left out of `PropertiesChanged`, as the
//! specification asks: a player that announced every tick would wake every panel several times a
//! second. A jump in position is announced with `Seeked` instead.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use goosic_shell_support::media::{
    CommandAvailability, MediaCommand, MediaPlaybackState, MediaSnapshot, NowPlaying,
};
use gtk::prelude::*;
use gtk::{gio, glib};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.goosic";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// How far the position may drift from where steady playback would put it before the change is
/// announced as a seek. Samples arrive a quarter second apart and not on a metronome.
const SEEK_TOLERANCE: f64 = 1.5;

const INTROSPECTION: &str = r#"<node>
  <interface name='org.mpris.MediaPlayer2'>
    <method name='Raise'/>
    <method name='Quit'/>
    <property name='CanQuit' type='b' access='read'/>
    <property name='CanRaise' type='b' access='read'/>
    <property name='HasTrackList' type='b' access='read'/>
    <property name='Identity' type='s' access='read'/>
    <property name='DesktopEntry' type='s' access='read'/>
    <property name='SupportedUriSchemes' type='as' access='read'/>
    <property name='SupportedMimeTypes' type='as' access='read'/>
  </interface>
  <interface name='org.mpris.MediaPlayer2.Player'>
    <method name='Play'/>
    <method name='Pause'/>
    <method name='PlayPause'/>
    <method name='Stop'/>
    <method name='Next'/>
    <method name='Previous'/>
    <method name='Seek'>
      <arg type='x' name='Offset' direction='in'/>
    </method>
    <method name='SetPosition'>
      <arg type='o' name='TrackId' direction='in'/>
      <arg type='x' name='Position' direction='in'/>
    </method>
    <signal name='Seeked'>
      <arg type='x' name='Position'/>
    </signal>
    <property name='PlaybackStatus' type='s' access='read'/>
    <property name='Metadata' type='a{sv}' access='read'/>
    <property name='Position' type='x' access='read'/>
    <property name='Volume' type='d' access='readwrite'/>
    <property name='Rate' type='d' access='read'/>
    <property name='MinimumRate' type='d' access='read'/>
    <property name='MaximumRate' type='d' access='read'/>
    <property name='CanGoNext' type='b' access='read'/>
    <property name='CanGoPrevious' type='b' access='read'/>
    <property name='CanPlay' type='b' access='read'/>
    <property name='CanPause' type='b' access='read'/>
    <property name='CanSeek' type='b' access='read'/>
    <property name='CanControl' type='b' access='read'/>
  </interface>
</node>"#;

/// What the bus may ask of the shell, and where it reads the state it rechecks against.
pub struct MprisHandlers {
    pub snapshot: Box<dyn Fn() -> MediaSnapshot>,
    pub toggle_pause: Box<dyn Fn()>,
    pub next: Box<dyn Fn()>,
    pub previous: Box<dyn Fn()>,
    pub stop: Box<dyn Fn()>,
    /// An absolute position, in seconds.
    pub seek: Box<dyn Fn(f64)>,
    pub set_volume: Box<dyn Fn(f64)>,
    pub raise: Box<dyn Fn()>,
    pub quit: Box<dyn Fn()>,
}

/// What a client can see, computed from one snapshot.
#[derive(Debug, Clone, PartialEq)]
struct Published {
    now: NowPlaying,
    availability: CommandAvailability,
    volume: f64,
    track_path: String,
}

impl Published {
    fn from_snapshot(snapshot: &MediaSnapshot) -> Published {
        let now = NowPlaying::from_snapshot(snapshot);
        let track_path = track_path(
            snapshot
                .track
                .as_ref()
                .filter(|_| now.is_active)
                .map(|track| track.video_id.as_str()),
        );
        Published {
            availability: CommandAvailability::from_snapshot(snapshot),
            volume: if snapshot.is_muted {
                0.0
            } else {
                snapshot.volume
            },
            track_path,
            now,
        }
    }

    /// Equal in everything a client redraws on, which is everything but the position.
    fn looks_like(&self, other: &Published) -> bool {
        let mut same = other.clone();
        same.now.elapsed_time = self.now.elapsed_time;
        *self == same
    }
}

pub struct Mpris {
    handlers: MprisHandlers,
    published: RefCell<Option<Published>>,
    /// The last position published, when, and whether it was advancing.
    clock: Cell<Option<(f64, Instant, bool)>>,
    connection: RefCell<Option<gio::DBusConnection>>,
    registrations: RefCell<Vec<gio::RegistrationId>>,
    owner: RefCell<Option<gio::OwnerId>>,
}

impl Mpris {
    pub fn start(handlers: MprisHandlers) -> Rc<Mpris> {
        let mpris = Rc::new(Mpris {
            handlers,
            published: RefCell::new(None),
            clock: Cell::new(None),
            connection: RefCell::new(None),
            registrations: RefCell::new(Vec::new()),
            owner: RefCell::new(None),
        });
        mpris.own(BUS_NAME.to_owned(), true);
        mpris
    }

    /// Asks the session bus for `name`. When another player already holds the plain name — a
    /// second Goosic, most often — the specification's instance form is used instead, so both
    /// show up in the panel rather than one silently missing.
    fn own(self: &Rc<Self>, name: String, first: bool) {
        let (acquired, lost) = (Rc::downgrade(self), Rc::downgrade(self));
        let owner = gio::bus_own_name(
            gio::BusType::Session,
            &name,
            gio::BusNameOwnerFlags::DO_NOT_QUEUE,
            move |connection, _| {
                if let Some(mpris) = acquired.upgrade() {
                    mpris.register(connection);
                }
            },
            |_, _| {},
            move |connection, _| {
                // Without a session bus Goosic keeps playing; only the desktop integration is
                // missing.
                let Some(mpris) = lost.upgrade() else {
                    return;
                };
                if first && connection.is_some() {
                    let instance = format!("{BUS_NAME}.instance{}", std::process::id());
                    let mpris = mpris.clone();
                    glib::idle_add_local_once(move || mpris.own(instance, false));
                }
            },
        );
        let previous = self.owner.borrow_mut().replace(owner);
        if let Some(previous) = previous {
            gio::bus_unown_name(previous);
        }
    }

    fn register(self: &Rc<Self>, connection: gio::DBusConnection) {
        // A bus that drops and comes back acquires the name again; registering the same path twice
        // would leave the first pair orphaned and answering.
        self.unregister();
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let mut registrations = Vec::new();
        for name in [ROOT_INTERFACE, PLAYER_INTERFACE] {
            let Some(info) = node.lookup_interface(name) else {
                continue;
            };
            let (call, get, set) = (
                Rc::downgrade(self),
                Rc::downgrade(self),
                Rc::downgrade(self),
            );
            let registered = connection
                .register_object(OBJECT_PATH, &info)
                .method_call(move |_, _, _, interface, method, parameters, invocation| {
                    if let Some(mpris) = call.upgrade() {
                        mpris.invoke(interface.unwrap_or_default(), method, &parameters);
                    }
                    // These are requests, answered at once: what actually happened arrives
                    // later as a confirmed sample.
                    invocation.return_value(None);
                })
                .property(move |_, _, _, interface, property| match get.upgrade() {
                    Some(mpris) => mpris.property(interface, property),
                    None => false.to_variant(),
                })
                .set_property(move |_, _, _, _, property, value| {
                    property == "Volume"
                        && set.upgrade().is_some_and(|mpris| mpris.set_volume(&value))
                })
                .build();
            if let Ok(id) = registered {
                registrations.push(id);
            }
        }
        *self.registrations.borrow_mut() = registrations;
        *self.connection.borrow_mut() = Some(connection);
    }

    fn unregister(&self) {
        let connection = self.connection.borrow_mut().take();
        let registrations: Vec<_> = self.registrations.borrow_mut().drain(..).collect();
        if let Some(connection) = connection {
            for id in registrations {
                let _ = connection.unregister_object(id);
            }
        }
    }

    /// Republishes what the desktop shows. Called on every refresh, it signals only when something
    /// a client draws has changed, and announces a jump in position as a seek.
    pub fn update(&self, snapshot: &MediaSnapshot) {
        let next = Published::from_snapshot(snapshot);
        let seeked = self.position_jump(&next.now);
        let changed = self
            .published
            .borrow()
            .as_ref()
            .is_none_or(|published| !published.looks_like(&next));
        *self.published.borrow_mut() = Some(next.clone());
        if changed {
            self.emit_properties_changed(&next);
        }
        if let Some(position) = seeked {
            self.emit_seeked(position);
        }
    }

    fn position_jump(&self, now: &NowPlaying) -> Option<f64> {
        let at = Instant::now();
        let playing = now.playback_state == MediaPlaybackState::Playing;
        let previous = self.clock.replace(Some((now.elapsed_time, at, playing)));
        let (elapsed, then, was_playing) = previous?;
        let jumped = is_seek(
            elapsed,
            at.duration_since(then).as_secs_f64(),
            was_playing,
            now.elapsed_time,
        );
        (now.is_active && jumped).then_some(now.elapsed_time)
    }

    fn property(&self, interface: &str, name: &str) -> glib::Variant {
        if interface == ROOT_INTERFACE {
            return match name {
                "CanQuit" | "CanRaise" => true.to_variant(),
                "HasTrackList" => false.to_variant(),
                "Identity" => "Goosic".to_variant(),
                "DesktopEntry" => crate::APP_ID.to_variant(),
                // Goosic opens nothing handed to it from outside; the queue is its own.
                _ => Vec::<String>::new().to_variant(),
            };
        }
        // Read live, so a client asking for the position gets the current one.
        let published = Published::from_snapshot(&(self.handlers.snapshot)());
        let availability = published.availability;
        match name {
            "PlaybackStatus" => playback_status(published.now.playback_state).to_variant(),
            "Metadata" => metadata(&published),
            "Position" => microseconds(published.now.elapsed_time).to_variant(),
            "Volume" => published.volume.to_variant(),
            // Goosic never varies the rate, and saying so keeps clients from offering a slider.
            "Rate" | "MinimumRate" | "MaximumRate" => 1.0_f64.to_variant(),
            "CanGoNext" => availability.next.to_variant(),
            "CanGoPrevious" => availability.previous.to_variant(),
            "CanPlay" => availability.play.to_variant(),
            "CanPause" => availability.pause.to_variant(),
            "CanSeek" => availability.change_position.to_variant(),
            _ => true.to_variant(), // CanControl
        }
    }

    fn set_volume(&self, value: &glib::Variant) -> bool {
        let Some(volume) = value.get::<f64>().filter(|volume| volume.is_finite()) else {
            return false;
        };
        let allowed = CommandAvailability::from_snapshot(&(self.handlers.snapshot)());
        if !allowed.allows(MediaCommand::ChangeVolume) {
            return false;
        }
        (self.handlers.set_volume)(volume.clamp(0.0, 1.0));
        true
    }

    fn invoke(&self, interface: &str, method: &str, parameters: &glib::Variant) {
        if interface == ROOT_INTERFACE {
            match method {
                "Raise" => (self.handlers.raise)(),
                "Quit" => (self.handlers.quit)(),
                _ => {}
            }
            return;
        }
        // Rechecked against a snapshot taken now rather than the one last published: the panel may
        // have drawn the button before playback moved.
        let snapshot = (self.handlers.snapshot)();
        let Some(command) = command_for(method) else {
            return;
        };
        if !CommandAvailability::from_snapshot(&snapshot).allows(command) {
            return;
        }
        let argument = |index| {
            parameters
                .try_child_value(index)
                .and_then(|value| value.get::<i64>())
        };
        match method {
            "Play" | "Pause" | "PlayPause" => (self.handlers.toggle_pause)(),
            "Stop" => (self.handlers.stop)(),
            "Next" => (self.handlers.next)(),
            "Previous" => (self.handlers.previous)(),
            "Seek" => {
                if let Some(offset) = argument(0) {
                    let now = NowPlaying::from_snapshot(&snapshot);
                    (self.handlers.seek)((now.elapsed_time + seconds(offset)).max(0.0));
                }
            }
            "SetPosition" => {
                if let Some(position) = argument(1) {
                    (self.handlers.seek)(seconds(position).max(0.0));
                }
            }
            _ => {}
        }
    }

    fn emit_properties_changed(&self, published: &Published) {
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let availability = published.availability;
        let changed = glib::VariantDict::new(None);
        changed.insert_value(
            "PlaybackStatus",
            &playback_status(published.now.playback_state).to_variant(),
        );
        changed.insert_value("Metadata", &metadata(published));
        changed.insert_value("Volume", &published.volume.to_variant());
        for (name, value) in [
            ("CanGoNext", availability.next),
            ("CanGoPrevious", availability.previous),
            ("CanPlay", availability.play),
            ("CanPause", availability.pause),
            ("CanSeek", availability.change_position),
        ] {
            changed.insert_value(name, &value.to_variant());
        }
        let parameters = glib::Variant::tuple_from_iter([
            PLAYER_INTERFACE.to_variant(),
            changed.end(),
            Vec::<String>::new().to_variant(),
        ]);
        let _ = connection.emit_signal(
            None,
            OBJECT_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&parameters),
        );
    }

    fn emit_seeked(&self, position: f64) {
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let _ = connection.emit_signal(
            None,
            OBJECT_PATH,
            PLAYER_INTERFACE,
            "Seeked",
            Some(&(microseconds(position),).to_variant()),
        );
    }
}

impl Drop for Mpris {
    fn drop(&mut self) {
        self.unregister();
        if let Some(owner) = self.owner.borrow_mut().take() {
            gio::bus_unown_name(owner);
        }
    }
}

fn playback_status(state: MediaPlaybackState) -> &'static str {
    match state {
        MediaPlaybackState::Playing => "Playing",
        MediaPlaybackState::Paused => "Paused",
        MediaPlaybackState::Stopped => "Stopped",
    }
}

fn metadata(published: &Published) -> glib::Variant {
    let dict = glib::VariantDict::new(None);
    let path = glib::variant::ObjectPath::try_from(published.track_path.clone())
        .or_else(|_| glib::variant::ObjectPath::try_from(NO_TRACK.to_owned()))
        .expect("the no-track path is a valid object path");
    dict.insert_value("mpris:trackid", &path.to_variant());
    let now = &published.now;
    if now.is_active {
        if now.duration > 0.0 {
            dict.insert_value("mpris:length", &microseconds(now.duration).to_variant());
        }
        if let Some(title) = &now.title {
            dict.insert_value("xesam:title", &title.to_variant());
        }
        if let Some(artist) = &now.artist {
            dict.insert_value("xesam:artist", &vec![artist.clone()].to_variant());
        }
        if let Some(album) = &now.album {
            dict.insert_value("xesam:album", &album.to_variant());
        }
        if let Some(artwork) = &now.artwork_url {
            dict.insert_value("mpris:artUrl", &artwork.to_variant());
        }
    }
    dict.end()
}

/// A track id must be a valid object path, and a video id is not always one.
fn track_path(video_id: Option<&str>) -> String {
    match video_id.filter(|id| !id.is_empty()) {
        None => NO_TRACK.to_owned(),
        Some(id) => {
            let safe: String = id
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            format!("/org/goosic/track/{safe}")
        }
    }
}

fn command_for(method: &str) -> Option<MediaCommand> {
    Some(match method {
        "Play" => MediaCommand::Play,
        "Pause" => MediaCommand::Pause,
        "PlayPause" => MediaCommand::TogglePlayPause,
        "Stop" => MediaCommand::Stop,
        "Next" => MediaCommand::Next,
        "Previous" => MediaCommand::Previous,
        "Seek" | "SetPosition" => MediaCommand::ChangePosition,
        _ => return None,
    })
}

/// Whether a new position is a jump rather than steady playback from the last one.
fn is_seek(last: f64, seconds_since: f64, was_playing: bool, now: f64) -> bool {
    let expected = if was_playing {
        last + seconds_since
    } else {
        last
    };
    (now - expected).abs() > SEEK_TOLERANCE
}

fn microseconds(seconds: f64) -> i64 {
    (seconds * 1_000_000.0) as i64
}

fn seconds(microseconds: i64) -> f64 {
    microseconds as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_id_is_always_a_valid_object_path() {
        assert_eq!(track_path(None), NO_TRACK);
        assert_eq!(track_path(Some("")), NO_TRACK);
        assert_eq!(
            track_path(Some("dQw4w9WgXcQ")),
            "/org/goosic/track/dQw4w9WgXcQ"
        );
        let odd = track_path(Some("a-b_c"));
        assert_eq!(odd, "/org/goosic/track/a_b_c");
        assert!(glib::variant::ObjectPath::try_from(odd).is_ok());
    }

    #[test]
    fn every_declared_player_method_maps_to_a_command_the_rules_can_refuse() {
        for method in [
            "Play",
            "Pause",
            "PlayPause",
            "Stop",
            "Next",
            "Previous",
            "Seek",
            "SetPosition",
        ] {
            assert!(
                command_for(method).is_some(),
                "{method} is declared and must be checked"
            );
        }
        assert_eq!(
            command_for("OpenUri"),
            None,
            "not declared, so not honoured"
        );
    }

    #[test]
    fn steady_playback_is_not_a_seek_and_a_jump_is() {
        assert!(!is_seek(10.0, 0.25, true, 10.25));
        assert!(!is_seek(10.0, 5.0, false, 10.0), "paused time stands still");
        assert!(is_seek(10.0, 0.25, true, 60.0));
        assert!(is_seek(60.0, 0.25, true, 5.0), "backwards counts too");
    }

    #[test]
    fn the_position_alone_does_not_make_a_visible_change() {
        let snapshot = MediaSnapshot {
            track: None,
            current_time: 1.0,
            duration: 0.0,
            is_paused: true,
            owner: goosic_protocol::Owner::None,
            is_advertisement: false,
            has_queue: false,
            transition: goosic_shell_support::navigation::PlaybackTransition::Idle,
            volume: 0.5,
            is_muted: false,
            is_ready: false,
        };
        let first = Published::from_snapshot(&snapshot);
        let later = Published::from_snapshot(&MediaSnapshot {
            current_time: 9.0,
            ..snapshot.clone()
        });
        assert!(first.looks_like(&later));
        let louder = Published::from_snapshot(&MediaSnapshot {
            volume: 0.9,
            ..snapshot
        });
        assert!(!first.looks_like(&louder));
    }
}
