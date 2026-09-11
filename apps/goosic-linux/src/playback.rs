//! What the shell remembers about playback between the service's answers.
//!
//! Rust decides who may play. This is the other half: the queue, what the renderer last confirmed,
//! which lease transition is in flight, and the preferences that shape what plays next. Nothing
//! here touches GTK, WebKit or the service, so every transition is testable on its own; the rules
//! it applies come from `goosic-shell-support`.

use goosic_protocol::{Owner, PlaybackState, SettingsSnapshot};
use goosic_shell_support::catalog::Track;
use goosic_shell_support::media::MediaSnapshot;
use goosic_shell_support::navigation::{PlaybackTransition, RepeatMode};
use goosic_shell_support::playback::{clamp_volume, index_after, random_index, PendingSeek};

/// The shell's playback state.
#[derive(Debug)]
pub struct Player {
    /// Rust's lease, as the service last reported it.
    pub lease: PlaybackState,
    pub queue: Vec<Track>,
    pub index: usize,
    pub current: Option<Track>,
    transition: PlaybackTransition,
    transition_token: u64,
    pub paused: bool,
    pub current_time: f64,
    pub duration: f64,
    pub volume: f64,
    pub muted: bool,
    pub advertisement: bool,
    /// Set only by a validated sample for this owner, generation and video. The media controls stay
    /// empty until it is.
    pub confirmed: bool,
    pub pending_seek: Option<PendingSeek>,
    /// The video whose end has already advanced the queue, so repeated `ended` reports cannot skip
    /// several tracks at once.
    pub ended_video_id: Option<String>,
    /// Whether the stored volume has been pushed to this load's page yet.
    pub volume_applied_for_load: bool,
    pub autoplay: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub radio_in_flight: bool,
    /// The track the current radio grew from, so a radio that runs out does not reseed from the
    /// same song and loop.
    pub radio_seed: Option<String>,
    /// What the shell tells the user about playback.
    pub status: String,
    sample_base: u64,
    last_sample_sent: u64,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Self {
            lease: PlaybackState {
                account_id: None,
                owner: Owner::None,
                generation: 0,
                sample_sequence: 0,
            },
            queue: Vec::new(),
            index: 0,
            current: None,
            transition: PlaybackTransition::Idle,
            transition_token: 0,
            paused: true,
            current_time: 0.0,
            duration: 0.0,
            volume: 1.0,
            muted: false,
            advertisement: false,
            confirmed: false,
            pending_seek: None,
            ended_video_id: None,
            volume_applied_for_load: false,
            autoplay: true,
            shuffle: false,
            repeat: RepeatMode::Off,
            radio_in_flight: false,
            radio_seed: None,
            status: "Choose a track to begin.".to_owned(),
            sample_base: 0,
            last_sample_sent: 0,
        }
    }

    pub fn transition(&self) -> PlaybackTransition {
        self.transition
    }

    /// Starts a lease transition and returns the token its answer must present.
    pub fn begin_transition(&mut self, kind: PlaybackTransition) -> u64 {
        self.transition_token += 1;
        self.transition = kind;
        self.transition_token
    }

    /// Whether an answer still belongs to the transition in flight. A late answer to one the user
    /// has since replaced must not act.
    pub fn is_current(&self, token: u64, kind: PlaybackTransition) -> bool {
        self.transition_token == token && self.transition == kind
    }

    pub fn finish_transition(&mut self, token: u64) {
        if self.transition_token == token {
            self.transition = PlaybackTransition::Idle;
        }
    }

    /// Forgets everything the previous track confirmed, so no position, end marker or volume
    /// decision is carried into the next one.
    pub fn begin_track(&mut self) {
        self.paused = true;
        self.current_time = 0.0;
        self.duration = 0.0;
        self.pending_seek = None;
        self.ended_video_id = None;
        self.advertisement = false;
        self.confirmed = false;
        self.volume_applied_for_load = false;
    }

    /// Makes `tracks` the queue, positioned on `chosen`. A deliberately chosen list is a fresh
    /// listening context, so it may start its own radio when it runs out.
    pub fn set_queue(&mut self, tracks: Vec<Track>, chosen: &Track) {
        self.index = tracks
            .iter()
            .position(|track| track.id == chosen.id)
            .unwrap_or(0);
        self.queue = tracks;
        self.radio_seed = None;
    }

    /// Moves the queue to `track`, putting it at the front when it is not already in it.
    pub fn select(&mut self, track: &Track) {
        match self.queue.iter().position(|queued| queued.id == track.id) {
            Some(index) => self.index = index,
            None => {
                self.queue.insert(0, track.clone());
                self.index = 0;
            }
        }
    }

    /// Records the lease the service reported. A new owner, generation or account means nothing the
    /// renderer confirmed before still applies, and Rust has started counting samples from zero.
    pub fn apply_lease(&mut self, lease: PlaybackState) {
        let changed = lease.owner != self.lease.owner
            || lease.generation != self.lease.generation
            || lease.account_id != self.lease.account_id;
        if changed {
            self.sample_base = 0;
            self.last_sample_sent = 0;
        }
        if changed || lease.owner == Owner::None {
            self.confirmed = false;
        }
        self.lease = lease;
    }

    /// Called when a new page starts loading under the lease already held.
    ///
    /// The page numbers its samples from one again, but Rust has already seen this generation
    /// count higher. Continuing from the last number sent keeps every sample acceptable, where
    /// restarting would have Rust refuse the new track's samples until their count caught up.
    pub fn begin_load(&mut self) {
        self.sample_base = self.last_sample_sent;
    }

    /// The sequence to report to Rust for a sample the page numbered `page_sequence`.
    pub fn sample_sequence(&mut self, page_sequence: u64) -> u64 {
        let sequence = self.sample_base + page_sequence;
        self.last_sample_sent = self.last_sample_sent.max(sequence);
        sequence
    }

    /// The queue position after the current one. `wrapping` is true for a deliberate Next.
    pub fn next_index(&self, wrapping: bool) -> Option<usize> {
        index_after(
            self.index,
            self.queue.len(),
            self.repeat,
            self.shuffle,
            wrapping,
            random_index,
        )
    }

    /// The position before the current one, wrapping to the end from the first track.
    pub fn previous_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }
        Some(if self.index > 0 {
            self.index - 1
        } else {
            self.queue.len() - 1
        })
    }

    pub fn queued(&self) -> Option<&Track> {
        self.queue.get(self.index)
    }

    /// Adopts stored preferences. Where the shell opens is navigation's business, not this.
    pub fn apply_settings(&mut self, settings: &SettingsSnapshot) {
        self.volume = clamp_volume(settings.volume).unwrap_or(1.0);
        self.muted = settings.muted;
        self.autoplay = settings.autoplay;
        self.shuffle = settings.shuffle;
        self.repeat = RepeatMode::from_raw(&settings.repeat_mode).unwrap_or(RepeatMode::Off);
    }

    /// The state the system media controls are projected from.
    pub fn snapshot(&self) -> MediaSnapshot {
        MediaSnapshot {
            track: self.current.clone(),
            current_time: self.current_time,
            duration: self.duration,
            is_paused: self.paused,
            owner: self.lease.owner,
            is_advertisement: self.advertisement,
            has_queue: !self.queue.is_empty(),
            transition: self.transition,
            volume: self.volume,
            is_muted: self.muted,
            is_ready: self.confirmed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: format!("Track {id}"),
            subtitle: String::new(),
            artist: String::new(),
            artist_id: None,
            album: String::new(),
            album_id: None,
            duration: String::new(),
            video_id: id.into(),
            explicit: false,
            thumbnail: None,
        }
    }

    fn lease(owner: Owner, generation: u64) -> PlaybackState {
        PlaybackState {
            account_id: None,
            owner,
            generation,
            sample_sequence: 0,
        }
    }

    #[test]
    fn a_late_answer_cannot_finish_a_transition_it_does_not_own() {
        let mut player = Player::new();
        let first = player.begin_transition(PlaybackTransition::Claiming);
        let second = player.begin_transition(PlaybackTransition::Releasing);
        assert!(!player.is_current(first, PlaybackTransition::Claiming));
        player.finish_transition(first);
        assert_eq!(player.transition(), PlaybackTransition::Releasing);
        player.finish_transition(second);
        assert_eq!(player.transition(), PlaybackTransition::Idle);
    }

    #[test]
    fn a_chosen_list_becomes_the_queue_positioned_on_the_choice() {
        let mut player = Player::new();
        player.radio_seed = Some("old".into());
        player.set_queue(vec![track("a"), track("b"), track("c")], &track("b"));
        assert_eq!(player.index, 1);
        assert_eq!(
            player.radio_seed, None,
            "a new list may start its own radio"
        );
    }

    #[test]
    fn a_track_outside_the_queue_goes_to_its_front() {
        let mut player = Player::new();
        player.set_queue(vec![track("a"), track("b")], &track("b"));
        player.select(&track("z"));
        assert_eq!(player.queued().map(|t| t.id.as_str()), Some("z"));
        assert_eq!(player.queue.len(), 3);
        player.select(&track("b"));
        assert_eq!(player.index, 2);
    }

    #[test]
    fn a_new_track_forgets_what_the_last_one_confirmed() {
        let mut player = Player::new();
        player.confirmed = true;
        player.paused = false;
        player.current_time = 42.0;
        player.ended_video_id = Some("a".into());
        player.volume_applied_for_load = true;
        player.begin_track();
        assert!(!player.confirmed);
        assert!(player.paused);
        assert_eq!(player.current_time, 0.0);
        assert_eq!(player.ended_video_id, None);
        assert!(!player.volume_applied_for_load);
    }

    #[test]
    fn a_new_generation_clears_confirmation_but_an_acknowledgement_does_not() {
        let mut player = Player::new();
        player.apply_lease(lease(Owner::OfficialWebView, 1));
        player.confirmed = true;
        player.apply_lease(lease(Owner::OfficialWebView, 1));
        assert!(
            player.confirmed,
            "a sample acknowledgement repeats the lease"
        );
        player.apply_lease(lease(Owner::OfficialWebView, 2));
        assert!(!player.confirmed);
    }

    #[test]
    fn samples_keep_counting_up_across_tracks_under_one_lease() {
        let mut player = Player::new();
        player.apply_lease(lease(Owner::OfficialWebView, 1));
        assert_eq!(player.sample_sequence(1), 1);
        assert_eq!(player.sample_sequence(7), 7);
        // The next track's page numbers from one again; Rust must still see an increase.
        player.begin_load();
        assert_eq!(player.sample_sequence(1), 8);
        assert_eq!(player.sample_sequence(2), 9);
        // A new lease is a new count, from Rust's side as well as ours.
        player.apply_lease(lease(Owner::OfficialWebView, 2));
        player.begin_load();
        assert_eq!(player.sample_sequence(1), 1);
    }

    #[test]
    fn previous_wraps_from_the_first_track_to_the_last() {
        let mut player = Player::new();
        assert_eq!(player.previous_index(), None);
        player.set_queue(vec![track("a"), track("b"), track("c")], &track("a"));
        assert_eq!(player.previous_index(), Some(2));
        player.index = 2;
        assert_eq!(player.previous_index(), Some(1));
    }

    #[test]
    fn stored_preferences_are_adopted_and_nonsense_is_corrected() {
        let mut player = Player::new();
        player.apply_settings(&SettingsSnapshot {
            theme: "dark".into(),
            volume: f64::NAN,
            muted: true,
            autoplay: false,
            last_route: "charts".into(),
            queue_visible: false,
            shuffle: true,
            repeat_mode: "one".into(),
            imported_from_legacy: false,
            legacy_available: false,
        });
        assert_eq!(player.volume, 1.0);
        assert!(player.muted);
        assert!(!player.autoplay);
        assert!(player.shuffle);
        assert_eq!(player.repeat, RepeatMode::One);
    }
}
