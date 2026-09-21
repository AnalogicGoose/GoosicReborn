//! What the operating system's media controls show, and which of their buttons may work.
//!
//! macOS publishes to Now Playing, Linux to MPRIS, and Windows will publish to its transport
//! controls. Each adapter talks to a different API, and none of them may decide anything: the
//! projection and the command availability are computed here from confirmed state, so a panel on
//! the desktop cannot offer a transition the app itself would refuse, and three adapters cannot
//! disagree about when Next is allowed.

use goosic_protocol::Owner;

use crate::catalog::Track;
use crate::navigation::PlaybackTransition;

/// The state the projection is computed from.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSnapshot {
    pub track: Option<Track>,
    pub current_time: f64,
    pub duration: f64,
    pub is_paused: bool,
    pub owner: Owner,
    pub is_advertisement: bool,
    pub has_queue: bool,
    pub transition: PlaybackTransition,
    pub volume: f64,
    pub is_muted: bool,
    /// True only after a validated event for this exact owner, generation and video.
    pub is_ready: bool,
}

impl MediaSnapshot {
    fn is_active(&self) -> bool {
        self.is_ready && self.owner != Owner::None && self.track.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPlaybackState {
    Stopped,
    Paused,
    Playing,
}

/// What the system shows as now playing.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub elapsed_time: f64,
    pub duration: f64,
    pub playback_rate: f64,
    pub playback_state: MediaPlaybackState,
    pub is_active: bool,
}

impl NowPlaying {
    pub fn from_snapshot(snapshot: &MediaSnapshot) -> NowPlaying {
        let active = snapshot.is_active();
        let duration = if snapshot.duration.is_finite() && snapshot.duration > 0.0 {
            snapshot.duration
        } else {
            0.0
        };
        let raw_elapsed = if snapshot.current_time.is_finite() {
            snapshot.current_time.max(0.0)
        } else {
            0.0
        };
        let elapsed_time = if duration > 0.0 { raw_elapsed.min(duration) } else { raw_elapsed };
        // Advertisement status affects which commands are available, not the confirmed media
        // state. A playing advertisement is still reported as playing.
        let playing = active && !snapshot.is_paused;
        let track = if active { snapshot.track.as_ref() } else { None };
        let non_empty = |value: &str| (!value.is_empty()).then(|| value.to_owned());

        NowPlaying {
            title: track.and_then(|t| non_empty(&t.title)),
            artist: track.and_then(|t| non_empty(&t.artist)),
            album: track.and_then(|t| non_empty(&t.album)),
            artwork_url: track
                .and_then(|t| t.thumbnail.as_deref())
                .filter(|thumbnail| url::Url::parse(thumbnail).is_ok())
                .map(str::to_owned),
            elapsed_time,
            duration,
            playback_rate: if playing { 1.0 } else { 0.0 },
            playback_state: match (active, playing) {
                (false, _) => MediaPlaybackState::Stopped,
                (true, true) => MediaPlaybackState::Playing,
                (true, false) => MediaPlaybackState::Paused,
            },
            is_active: active,
        }
    }
}

/// A button the system media controls can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaCommand {
    Play,
    Pause,
    TogglePlayPause,
    Next,
    Previous,
    ChangePosition,
    Stop,
    ChangeVolume,
}

/// Which buttons may work right now.
///
/// An adapter publishes this and also rechecks a command against it when the bus delivers one,
/// so a remote client cannot ask for a transition the app would refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandAvailability {
    pub play: bool,
    pub pause: bool,
    pub toggle_play_pause: bool,
    pub next: bool,
    pub previous: bool,
    pub change_position: bool,
    pub stop: bool,
    pub change_volume: bool,
}

impl CommandAvailability {
    pub fn from_snapshot(snapshot: &MediaSnapshot) -> CommandAvailability {
        let active = snapshot.is_active();
        let ready = active && snapshot.transition == PlaybackTransition::Idle;
        let playing = active && !snapshot.is_paused;
        let paused = active && snapshot.is_paused;
        let content_commands = ready && !snapshot.is_advertisement;

        CommandAvailability {
            play: ready && paused,
            pause: ready && playing,
            toggle_play_pause: ready,
            next: content_commands && snapshot.has_queue,
            previous: content_commands && snapshot.has_queue,
            change_position: content_commands
                && snapshot.duration.is_finite()
                && snapshot.duration > 0.0,
            stop: ready,
            change_volume: ready && !snapshot.is_advertisement,
        }
    }

    pub fn allows(&self, command: MediaCommand) -> bool {
        match command {
            MediaCommand::Play => self.play,
            MediaCommand::Pause => self.pause,
            MediaCommand::TogglePlayPause => self.toggle_play_pause,
            MediaCommand::Next => self.next,
            MediaCommand::Previous => self.previous,
            MediaCommand::ChangePosition => self.change_position,
            MediaCommand::Stop => self.stop,
            MediaCommand::ChangeVolume => self.change_volume,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: "track".into(),
            title: "Afterglow".into(),
            subtitle: "Song".into(),
            artist: "Signal Fires".into(),
            artist_id: None,
            album: "Night Windows".into(),
            album_id: None,
            duration: "3:42".into(),
            video_id: "video".into(),
            explicit: false,
            thumbnail: Some("https://example.test/art.jpg".into()),
        }
    }

    fn snapshot() -> MediaSnapshot {
        MediaSnapshot {
            track: Some(track()),
            current_time: 42.0,
            duration: 222.0,
            is_paused: false,
            owner: Owner::OfficialWebView,
            is_advertisement: false,
            has_queue: true,
            transition: PlaybackTransition::Idle,
            volume: 1.0,
            is_muted: false,
            is_ready: true,
        }
    }

    const EVERY_COMMAND: [MediaCommand; 8] = [
        MediaCommand::Play,
        MediaCommand::Pause,
        MediaCommand::TogglePlayPause,
        MediaCommand::Next,
        MediaCommand::Previous,
        MediaCommand::ChangePosition,
        MediaCommand::Stop,
        MediaCommand::ChangeVolume,
    ];

    #[test]
    fn the_projection_uses_the_confirmed_position_and_track_metadata() {
        let projection = NowPlaying::from_snapshot(&snapshot());
        assert_eq!(projection.title.as_deref(), Some("Afterglow"));
        assert_eq!(projection.artist.as_deref(), Some("Signal Fires"));
        assert_eq!(projection.album.as_deref(), Some("Night Windows"));
        assert_eq!(projection.artwork_url.as_deref(), Some("https://example.test/art.jpg"));
        assert_eq!(projection.elapsed_time, 42.0);
        assert_eq!(projection.duration, 222.0);
        assert_eq!(projection.playback_rate, 1.0);
        assert_eq!(projection.playback_state, MediaPlaybackState::Playing);
        assert!(projection.is_active);
    }

    #[test]
    fn the_projection_clamps_impossible_times_and_stops_when_released() {
        let clamped =
            NowPlaying::from_snapshot(&MediaSnapshot { current_time: 999.0, duration: 30.0, ..snapshot() });
        assert_eq!(clamped.elapsed_time, 30.0);
        assert_eq!(clamped.duration, 30.0);

        let released = NowPlaying::from_snapshot(&MediaSnapshot { owner: Owner::None, ..snapshot() });
        assert!(!released.is_active);
        assert_eq!(released.title, None);
        assert_eq!(released.playback_state, MediaPlaybackState::Stopped);
        assert_eq!(released.playback_rate, 0.0);
    }

    #[test]
    fn nothing_is_projected_or_offered_before_the_first_confirmed_sample() {
        let loading = MediaSnapshot { is_ready: false, ..snapshot() };
        let projection = NowPlaying::from_snapshot(&loading);
        assert!(!projection.is_active);
        assert_eq!(projection.title, None);
        assert_eq!(projection.playback_state, MediaPlaybackState::Stopped);
        let availability = CommandAvailability::from_snapshot(&loading);
        for command in EVERY_COMMAND {
            assert!(!availability.allows(command), "{command:?} offered before a confirmed sample");
        }
    }

    #[test]
    fn advertisements_stay_pauseable_but_disable_content_and_volume_commands() {
        let paused_ad = CommandAvailability::from_snapshot(&MediaSnapshot {
            is_paused: true,
            is_advertisement: true,
            ..snapshot()
        });
        assert!(paused_ad.play);
        assert!(!paused_ad.pause);
        assert!(paused_ad.toggle_play_pause);
        assert!(!paused_ad.next);
        assert!(!paused_ad.previous);
        assert!(!paused_ad.change_position);
        assert!(paused_ad.stop);
        assert!(!paused_ad.change_volume);

        let playing_ad =
            CommandAvailability::from_snapshot(&MediaSnapshot { is_advertisement: true, ..snapshot() });
        assert!(!playing_ad.play);
        assert!(playing_ad.pause);
        assert!(playing_ad.stop);
    }

    #[test]
    fn no_commands_are_available_during_a_playback_transition() {
        let availability = CommandAvailability::from_snapshot(&MediaSnapshot {
            transition: PlaybackTransition::Claiming,
            ..snapshot()
        });
        for command in EVERY_COMMAND {
            assert!(!availability.allows(command), "{command:?} offered mid-transition");
        }
    }
}
