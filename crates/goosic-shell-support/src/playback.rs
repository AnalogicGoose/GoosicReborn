//! The playback decisions a shell makes that Rust's authority does not.
//!
//! `goosic-core` decides who may play. What is here is everything a shell decides around that:
//! which queue position plays next, whether a requested seek or volume is meaningful, when a
//! pending seek has settled, whether a renderer's report is worth believing before it is
//! forwarded, when a finished track should advance, when the page has stopped delivering a song
//! the listener wanted, how a fresh page's volume is reconciled with the user's, and how rapid
//! preference changes are coalesced. In the Swift shell every one of
//! these lived inside `GoosicAppModel`, reachable only by constructing the whole model.

use std::time::{Duration, Instant};

use goosic_protocol::{Owner, PlaybackState, PreferencesPatch};

use crate::catalog::Track;
use crate::navigation::RepeatMode;

/// How long the scrubber shows a requested seek before trusting the player's position again.
pub const SEEK_SETTLE_WINDOW: Duration = Duration::from_secs(1);

/// How close a confirmed position must be to a requested seek for the seek to count as landed.
pub const SEEK_LANDED_TOLERANCE_SECONDS: f64 = 1.5;

/// How long rapid preference changes are coalesced before one save is sent.
pub const PREFERENCE_SAVE_DELAY: Duration = Duration::from_secs(1);

/// The queue position to play after `index`, honouring shuffle and repeat.
///
/// `wrapping` is true for a deliberate Next, which should move rather than do nothing at the end
/// of the queue, and false for a track ending on its own, which stops so radio can take over.
/// `pick(n)` must return a position in `0..n`; the shell supplies randomness, which keeps this
/// testable and keeps a randomness dependency out of the crate. [`random_index`] is a fine
/// default.
pub fn index_after(
    index: usize,
    count: usize,
    repeat: RepeatMode,
    shuffle: bool,
    wrapping: bool,
    pick: impl FnOnce(usize) -> usize,
) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    if repeat == RepeatMode::One && !wrapping {
        return Some(index);
    }
    if shuffle {
        if count == 1 {
            return (repeat == RepeatMode::All || wrapping).then_some(index);
        }
        // Any position but the current one, so shuffle never repeats a track back to back. The
        // Swift shell drew until it missed; drawing from the other positions directly gives the
        // same distribution and cannot spin.
        let candidate = pick(count - 1) % (count - 1);
        return Some(if candidate >= index {
            candidate + 1
        } else {
            candidate
        });
    }
    let next = index + 1;
    if next < count {
        return Some(next);
    }
    (repeat == RepeatMode::All || wrapping).then_some(0)
}

/// Previous restarts the first track unless repeat-all explicitly permits wrapping.
/// A missing current entry has no neighbours.
pub fn index_before(index: usize, count: usize, repeat: RepeatMode) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    Some(if index > 0 {
        index - 1
    } else if repeat == RepeatMode::All {
        count - 1
    } else {
        0
    })
}

/// An ad's final media state can outlive its DOM marker. Suppress end detection until content
/// plays again, and request play once if the handoff leaves content paused. A listener's pause
/// always wins. This state is reset for each load and never seeks or skips an advertisement.
#[derive(Debug, Default, Clone, Copy)]
pub struct AdPlaybackRecovery {
    pub awaiting_content: bool,
    resume_requested: bool,
}

impl AdPlaybackRecovery {
    pub fn observe(&mut self, advertisement: bool, state: &str, listener_paused: bool) -> bool {
        if advertisement {
            self.awaiting_content = true;
            self.resume_requested = false;
            return false;
        }
        if state == "playing" {
            self.awaiting_content = false;
            self.resume_requested = false;
            return false;
        }
        if self.awaiting_content && state == "paused" && !listener_paused && !self.resume_requested
        {
            self.resume_requested = true;
            return true;
        }
        false
    }
}

/// How long the page's clock may stand still, while it says it is playing, before Goosic says the
/// song has stopped arriving. Ordinary buffering recovers well inside this.
pub const STALL_AFTER: Duration = Duration::from_secs(12);

/// How long a pause nobody asked for must last before it counts as the page stopping on its own.
/// A fresh page loads paused and is nudged once, which takes well under this.
pub const UNREQUESTED_PAUSE_AFTER: Duration = Duration::from_secs(5);

/// Playback the listener wanted that the official page stopped delivering.
///
/// YouTube can stop sending a song partway through: its stream switches to a status that carries
/// no media, the page's clock stops, and soon after the page empties its player and calls itself
/// paused. Nothing in the page reports that as an error, so without this a shell shows "Paused"
/// for a song the listener never paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackInterruption {
    /// The page says it is playing, but its clock has not moved for [`STALL_AFTER`].
    Stalled,
    /// The page stopped, or emptied itself, without the listener pausing it.
    StoppedByPage,
}

impl PlaybackInterruption {
    /// What the listener is told, short enough for a player bar's status line. The cause is
    /// YouTube's, and the way back is choosing the song again, because a page that has emptied its
    /// player does not answer play. An advertisement is named as what stopped, and recovering
    /// never skips it.
    pub fn message(self, is_advertisement: bool) -> &'static str {
        match (self, is_advertisement) {
            (_, true) => "The ad stalled. Choose the song again.",
            (Self::Stalled, false) => "YouTube stalled. Choose the song again.",
            (Self::StoppedByPage, false) => "YouTube stopped. Choose the song again.",
        }
    }
}

/// A sample as [`PlaybackWatch`] needs to see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatchSample<'a> {
    pub state: &'a str,
    pub position: f64,
    pub duration: f64,
    pub is_advertisement: bool,
    /// The listener paused, through Goosic or its system media controls.
    pub listener_paused: bool,
    /// Goosic has asked the page to pause and is waiting for it to confirm.
    pub pause_requested: bool,
}

/// Notices when the official page stops delivering playback the listener wanted.
///
/// Feed it every believed sample with [`PlaybackWatch::observe`], and [`PlaybackWatch::reset`] it
/// for each new load. It decides nothing about authority and requests nothing from the page: it
/// only tells the shell when to say that playback stopped, and when that is no longer true.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlaybackWatch {
    last_position: Option<f64>,
    progress_at: Option<Instant>,
    unrequested_pause_since: Option<Instant>,
    /// Whether this load's clock has been seen running. A page still loading can call itself
    /// playing at zero and then sit paused there for as long as loading takes, and that is not the
    /// page stopping.
    played: bool,
    current: Option<PlaybackInterruption>,
}

/// What one sample changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchChange {
    Unchanged,
    /// Playback has just been found interrupted; say so once.
    Interrupted(PlaybackInterruption),
    /// An interruption is over: the clock moved again, or the listener took over.
    Recovered,
}

impl PlaybackWatch {
    /// Forgets everything, for a new load.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The interruption in force, if any.
    pub fn interruption(&self) -> Option<PlaybackInterruption> {
        self.current
    }

    pub fn observe(&mut self, sample: &WatchSample, now: Instant) -> WatchChange {
        let finished = has_finished(
            sample.state,
            sample.position,
            sample.duration,
            sample.listener_paused,
        );
        let moved = match self.last_position {
            Some(last) => (sample.position - last).abs() > 0.2,
            None => true,
        };
        if moved {
            self.last_position = Some(sample.position);
            self.progress_at = Some(now);
        }
        if sample.state != "playing" || sample.listener_paused || sample.pause_requested {
            // The clock is not expected to move, so a stall can only be timed from when it is.
            self.progress_at = Some(now);
        }

        if sample.state == "playing" && sample.position > 0.5 {
            self.played = true;
        }

        let detected = if sample.listener_paused || sample.pause_requested || finished {
            // The listener's own pause, or a song that played through, is never an interruption.
            self.unrequested_pause_since = None;
            None
        } else if sample.state == "playing" {
            self.unrequested_pause_since = None;
            let still_since = self.progress_at.unwrap_or(now);
            (now.duration_since(still_since) >= STALL_AFTER)
                .then_some(PlaybackInterruption::Stalled)
        } else if sample.is_advertisement || !self.played {
            // A paused advertisement is AdPlaybackRecovery's to resume, and a page that has not
            // started yet is still loading; neither is the page stopping on its own.
            self.unrequested_pause_since = None;
            None
        } else {
            let since = *self.unrequested_pause_since.get_or_insert(now);
            (now.duration_since(since) >= UNREQUESTED_PAUSE_AFTER)
                .then_some(PlaybackInterruption::StoppedByPage)
        };

        match (self.current, detected) {
            (None, Some(kind)) => {
                self.current = Some(kind);
                WatchChange::Interrupted(kind)
            }
            // A stall the page then gives up on is still the same interruption to the listener.
            (Some(_), Some(_)) => WatchChange::Unchanged,
            // Only a clock moving while playing is a recovery. A page that empties itself jumps its
            // position back to zero, and that jump is the interruption getting worse.
            (Some(_), None)
                if (moved && sample.state == "playing") || sample.listener_paused || finished =>
            {
                self.current = None;
                WatchChange::Recovered
            }
            _ => WatchChange::Unchanged,
        }
    }
}

/// A position in `0..count` that differs from call to call. Not cryptographic, and it does not
/// need to be: it picks the next shuffled track.
pub fn random_index(count: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    if count == 0 {
        return 0;
    }
    // Each `RandomState` is keyed afresh, so hashing nothing still yields a new value per call.
    let value = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    (value % count as u64) as usize
}

/// `seconds` as `m:ss`, or `h:mm:ss` past an hour. Nonsense renders as `0:00`.
pub fn time_text(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".to_owned();
    }
    let total = seconds.floor() as u64;
    let (hours, minutes, secs) = (total / 3_600, (total % 3_600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// Whether the scrubber may move at all. Zero-length media would make an empty range, and
/// advertisements are never seekable.
pub fn is_seekable(duration: f64, owner: Owner, is_advertisement: bool) -> bool {
    duration > 0.0 && owner != Owner::None && !is_advertisement
}

/// The position a seek request should actually ask for, or `None` when the request is not
/// meaningful.
///
/// The Swift shell clamped with `min(max(…))`, which passes a NaN straight through to the player.
/// This refuses it instead, and refuses a seek into media with no known length.
pub fn clamp_seek(position: f64, duration: f64) -> Option<f64> {
    if !position.is_finite() || !duration.is_finite() || duration <= 0.0 {
        return None;
    }
    Some(position.clamp(0.0, duration))
}

/// The volume a request should actually set, or `None` when the request is not a number.
pub fn clamp_volume(volume: f64) -> Option<f64> {
    volume.is_finite().then(|| volume.clamp(0.0, 1.0))
}

/// The gain, 0 to 1, a volume slider at `position` (0 to 1) plays at.
///
/// Loudness is heard logarithmically, so a slider that moved the gain in a straight line spent
/// almost all of its travel between "loud" and "louder". Cubing the position is the usual
/// approximation of an audio-taper potentiometer -- half way is about -18 dB and a tenth of the
/// way about -60 dB -- and unlike a pure decibel curve it still reaches true silence at zero. The
/// player and the saved preference hold the gain; only the slider and its percentage work in
/// positions, so a saved volume opens at the same slider position on every platform.
pub fn volume_gain(position: f64) -> f64 {
    let position = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    position * position * position
}

/// The slider position, 0 to 1, that plays at `gain`; the inverse of [`volume_gain`].
pub fn volume_position(gain: f64) -> f64 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0).cbrt()
    } else {
        0.0
    }
}

/// A seek the shell has asked for and not yet seen the player confirm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingSeek {
    pub position: f64,
    pub requested_at: Instant,
}

impl PendingSeek {
    /// Whether a confirmed position means this seek is over — it landed, or it has had long
    /// enough that the player's own report is the truth again.
    pub fn is_settled_by(&self, confirmed_time: f64, now: Instant) -> bool {
        (confirmed_time - self.position).abs() < SEEK_LANDED_TOLERANCE_SECONDS
            || now.saturating_duration_since(self.requested_at) >= SEEK_SETTLE_WINDOW
    }
}

/// Where the scrubber should sit: the pending seek while it is still settling, otherwise the
/// position the player last confirmed.
pub fn displayed_position(current_time: f64, pending: Option<PendingSeek>, now: Instant) -> f64 {
    match pending {
        Some(seek) if now.saturating_duration_since(seek.requested_at) < SEEK_SETTLE_WINDOW => {
            seek.position
        }
        _ => current_time,
    }
}

/// What a renderer reported, reduced to the parts the shell checks before believing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReportedSample<'a> {
    pub generation: u64,
    pub video_id: &'a str,
    pub current_time: f64,
    pub duration: f64,
}

/// Whether the shell should act on a sample from `host`.
///
/// This is deliberately the cheaper of two layers. The web host has already applied the full
/// [`crate::bridge::rejection_reason`] — bridge version, document token, sequence, volume range —
/// before forwarding anything, so this does not repeat it. It asks only what the shell itself
/// knows: that `host` holds the lease, at this generation, for the video it loaded, with a
/// position that is a real number. Deleting either layer would remove one that is doing work.
pub fn believes_sample(
    host: Owner,
    lease: &PlaybackState,
    loaded_video_id: Option<&str>,
    sample: &ReportedSample<'_>,
) -> bool {
    host != Owner::None
        && lease.owner == host
        && sample.generation == lease.generation
        && loaded_video_id == Some(sample.video_id)
        && sample.current_time.is_finite()
        && sample.current_time >= 0.0
        && sample.duration.is_finite()
        && sample.duration >= 0.0
}

/// How close to its length a stopped track has to be for the stop to count as its end.
pub const END_TOLERANCE_SECONDS: f64 = 1.5;

/// Whether a report says the track has played through.
///
/// A player normally says `ended`. With YouTube Music's Automix off -- see
/// [`crate::bridge::AUTOMIX_OFF_SCRIPT`] -- the official page instead stops a fraction of a second
/// short of the length and says `paused`, and a shell that waited for `ended` sat silent after
/// every song. A pause that close to the end is the end, unless the listener paused it.
pub fn has_finished(state: &str, position: f64, duration: f64, listener_paused: bool) -> bool {
    state == "ended"
        || (state == "paused"
            && !listener_paused
            && duration.is_finite()
            && duration > 0.0
            && position.is_finite()
            && position >= duration - END_TOLERANCE_SECONDS)
}

/// A sample as [`should_advance_after_end`] needs to see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndSample<'a> {
    pub state: &'a str,
    pub is_advertisement: bool,
    pub video_id: &'a str,
    pub position: f64,
    pub duration: f64,
}

/// Whether a sample means the queue should move on.
///
/// Advertisements end too, and must never advance the queue; a player reports its end
/// repeatedly, so only the first report for a video counts; and a pause the listener asked for is
/// not an end even at the last second -- see [`has_finished`].
pub fn should_advance_after_end(
    sample: &EndSample<'_>,
    last_ended_video_id: Option<&str>,
    listener_paused: bool,
) -> bool {
    !sample.is_advertisement
        && last_ended_video_id != Some(sample.video_id)
        && has_finished(
            sample.state,
            sample.position,
            sample.duration,
            listener_paused,
        )
}

/// What to do with the volume a web player reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumeSync {
    /// Advertisements never touch the stored volume.
    Ignore,
    /// Kept for source compatibility with existing shells; a hidden official renderer never
    /// supplies a user volume choice, so `volume_sync` no longer returns this variant.
    Follow { volume: f64, muted: bool },
    /// The page differs from the chosen volume or mute state. Reapply that choice.
    PushStored,
    /// The page matches the chosen volume and mute state.
    AlreadyMatches,
}

/// Reconciles a hidden official page's volume with the user's.
///
/// Its renderer cannot receive a user's volume input. A different value, even after the first
/// preference push, means the page reset or replaced its media element. Keep the chosen value
/// authoritative so a background page cannot silently turn itself up to full volume.
pub fn volume_sync(
    _applied_for_load: bool,
    is_advertisement: bool,
    reported_volume: f64,
    reported_muted: bool,
    stored_volume: f64,
    stored_muted: bool,
) -> VolumeSync {
    if is_advertisement {
        VolumeSync::Ignore
    } else if (reported_volume - stored_volume).abs() > 0.01 || reported_muted != stored_muted {
        VolumeSync::PushStored
    } else {
        VolumeSync::AlreadyMatches
    }
}

/// Coalesces a preference change into one still waiting to be saved.
///
/// Every field is carried. The Swift shell's earlier version copied six of the eight fields it
/// had at the time and dropped
/// `shuffle` and `repeatMode`, so toggling shuffle within a second of a volume drag was never
/// saved. A newer value wins; a field the update does not set keeps the pending one.
pub fn merge_preferences(
    pending: Option<PreferencesPatch>,
    update: PreferencesPatch,
) -> PreferencesPatch {
    let Some(pending) = pending else {
        return update;
    };
    PreferencesPatch {
        theme: update.theme.or(pending.theme),
        volume: update.volume.or(pending.volume),
        muted: update.muted.or(pending.muted),
        autoplay: update.autoplay.or(pending.autoplay),
        last_route: update.last_route.or(pending.last_route),
        queue_visible: update.queue_visible.or(pending.queue_visible),
        shuffle: update.shuffle.or(pending.shuffle),
        repeat_mode: update.repeat_mode.or(pending.repeat_mode),
        artwork_background: update.artwork_background.or(pending.artwork_background),
        hide_explicit: update.hide_explicit.or(pending.hide_explicit),
        start_page: update.start_page.or(pending.start_page),
        reduce_motion: update.reduce_motion.or(pending.reduce_motion),
    }
}

/// What the now-playing bar shows under the title, including when there is nothing to say.
pub fn now_playing_subtitle(track: Option<&Track>) -> String {
    match track {
        None => "Choose a track to begin".to_owned(),
        Some(track) => {
            let text = track.secondary_text();
            if text.is_empty() {
                track.duration.clone()
            } else {
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch_sample(state: &str, position: f64) -> WatchSample<'_> {
        WatchSample {
            state,
            position,
            duration: 226.0,
            is_advertisement: false,
            listener_paused: false,
            pause_requested: false,
        }
    }

    /// Feeds `sample` every half second from `from` for `seconds`, as the bridge reports, and
    /// returns every change that was not `Unchanged`.
    fn feed(
        watch: &mut PlaybackWatch,
        sample: WatchSample,
        from: Instant,
        seconds: f64,
        advancing: bool,
    ) -> Vec<WatchChange> {
        let mut changes = Vec::new();
        let steps = (seconds * 2.0) as u32;
        for step in 0..=steps {
            let elapsed = f64::from(step) * 0.5;
            let mut current = sample;
            if advancing {
                current.position += elapsed;
            }
            let change = watch.observe(&current, from + Duration::from_secs_f64(elapsed));
            if change != WatchChange::Unchanged {
                changes.push(change);
            }
        }
        changes
    }

    #[test]
    fn steady_playback_is_never_an_interruption() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        assert!(feed(&mut watch, watch_sample("playing", 0.0), t0, 120.0, true).is_empty());
        assert_eq!(watch.interruption(), None);
    }

    #[test]
    fn a_clock_standing_still_while_playing_is_a_stall_reported_once() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 0.0), t0, 40.0, true);
        let t1 = t0 + Duration::from_secs(40);
        let short = feed(&mut watch, watch_sample("playing", 40.0), t1, 11.0, false);
        assert!(
            short.is_empty(),
            "ordinary buffering is not a stall: {short:?}"
        );
        let changes = feed(&mut watch, watch_sample("playing", 40.0), t1, 30.0, false);
        assert_eq!(
            changes,
            vec![WatchChange::Interrupted(PlaybackInterruption::Stalled)]
        );
        assert_eq!(watch.interruption(), Some(PlaybackInterruption::Stalled));
    }

    #[test]
    fn a_stall_ends_when_the_clock_moves_again() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 40.0), t0, 13.0, false);
        assert_eq!(watch.interruption(), Some(PlaybackInterruption::Stalled));
        let t1 = t0 + Duration::from_secs(14);
        let changes = feed(&mut watch, watch_sample("playing", 40.0), t1, 5.0, true);
        assert_eq!(changes, vec![WatchChange::Recovered]);
        assert_eq!(watch.interruption(), None);
    }

    #[test]
    fn a_page_that_empties_itself_after_a_stall_stays_one_interruption() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 37.0), t0, 13.0, false);
        let mut emptied = watch_sample("paused", 0.0);
        emptied.duration = 0.0;
        let t1 = t0 + Duration::from_secs(14);
        let changes = feed(&mut watch, emptied, t1, 60.0, false);
        assert!(
            changes.is_empty(),
            "no recovery and no second report: {changes:?}"
        );
        assert_eq!(watch.interruption(), Some(PlaybackInterruption::Stalled));
    }

    #[test]
    fn a_pause_nobody_asked_for_is_the_page_stopping_on_its_own() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 0.0), t0, 30.0, true);
        let t1 = t0 + Duration::from_secs(31);
        let mut emptied = watch_sample("paused", 0.0);
        emptied.duration = 0.0;
        let early = feed(&mut watch, emptied, t1, 4.0, false);
        assert!(
            early.is_empty(),
            "a brief unrequested pause is not reported: {early:?}"
        );
        let changes = feed(&mut watch, emptied, t1, 10.0, false);
        assert_eq!(
            changes,
            vec![WatchChange::Interrupted(
                PlaybackInterruption::StoppedByPage
            )]
        );
    }

    #[test]
    fn a_fresh_page_that_loads_paused_and_is_nudged_is_not_reported() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        let mut announced = watch_sample("playing", 0.0);
        announced.duration = 0.0;
        assert_eq!(watch.observe(&announced, t0), WatchChange::Unchanged);
        let mut loading = watch_sample("paused", 0.0);
        loading.duration = 0.0;
        let slow = feed(&mut watch, loading, t0, 20.0, false);
        assert!(
            slow.is_empty(),
            "a slow load is not the page stopping: {slow:?}"
        );
        let t1 = t0 + Duration::from_secs(20);
        assert!(feed(&mut watch, watch_sample("playing", 0.0), t1, 60.0, true).is_empty());
    }

    #[test]
    fn the_listeners_own_pause_is_never_an_interruption_and_ends_one() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        let mut paused = watch_sample("paused", 80.0);
        paused.listener_paused = true;
        assert!(feed(&mut watch, paused, t0, 120.0, false).is_empty());

        let mut requested = watch_sample("playing", 80.0);
        requested.pause_requested = true;
        let t1 = t0 + Duration::from_secs(121);
        assert!(feed(&mut watch, requested, t1, 30.0, false).is_empty());

        let t2 = t1 + Duration::from_secs_f64(30.5);
        let resumed = feed(&mut watch, watch_sample("playing", 80.0), t2, 11.0, false);
        assert!(
            resumed.is_empty(),
            "the stall clock starts at the resume: {resumed:?}"
        );
        let t3 = t2 + Duration::from_secs(11);
        feed(&mut watch, watch_sample("playing", 80.0), t3, 2.0, false);
        assert_eq!(watch.interruption(), Some(PlaybackInterruption::Stalled));
        let t4 = t3 + Duration::from_secs(3);
        assert_eq!(watch.observe(&paused, t4), WatchChange::Recovered);
    }

    #[test]
    fn a_song_that_played_through_is_not_an_interruption() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 200.0), t0, 25.5, true);
        let t1 = t0 + Duration::from_secs(26);
        assert!(feed(&mut watch, watch_sample("paused", 225.6), t1, 30.0, false).is_empty());
        let t2 = t1 + Duration::from_secs(31);
        assert!(feed(&mut watch, watch_sample("ended", 226.0), t2, 30.0, false).is_empty());
    }

    #[test]
    fn a_paused_advertisement_is_left_to_ad_recovery_but_a_stalled_one_is_reported() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        let mut ad = watch_sample("paused", 5.0);
        ad.is_advertisement = true;
        assert!(feed(&mut watch, ad, t0, 30.0, false).is_empty());
        ad.state = "playing";
        let t1 = t0 + Duration::from_secs(31);
        let changes = feed(&mut watch, ad, t1, 13.0, false);
        assert_eq!(
            changes,
            vec![WatchChange::Interrupted(PlaybackInterruption::Stalled)]
        );
        let message = PlaybackInterruption::Stalled.message(true);
        assert!(message.contains("ad stalled"), "{message}");
    }

    #[test]
    fn reset_forgets_an_interruption_for_the_next_load() {
        let mut watch = PlaybackWatch::default();
        let t0 = Instant::now();
        feed(&mut watch, watch_sample("playing", 40.0), t0, 13.0, false);
        assert!(watch.interruption().is_some());
        watch.reset();
        assert_eq!(watch.interruption(), None);
        let t1 = t0 + Duration::from_secs(14);
        assert!(feed(&mut watch, watch_sample("playing", 0.0), t1, 5.0, true).is_empty());
    }

    #[test]
    fn interruption_messages_name_the_way_back_and_no_internals() {
        for kind in [
            PlaybackInterruption::Stalled,
            PlaybackInterruption::StoppedByPage,
        ] {
            for ad in [false, true] {
                let message = kind.message(ad);
                assert!(message.contains("again"), "{message}");
                assert!(
                    message.chars().count() <= 40,
                    "fits a status line: {message}"
                );
                assert!(!message.to_lowercase().contains("ump"), "{message}");
                assert!(!message.contains("attestation"), "{message}");
            }
        }
    }

    fn next(
        index: usize,
        count: usize,
        repeat: RepeatMode,
        shuffle: bool,
        wrapping: bool,
    ) -> Option<usize> {
        index_after(index, count, repeat, shuffle, wrapping, |n| {
            n.saturating_sub(1)
        })
    }

    #[test]
    fn an_empty_queue_has_nowhere_to_go() {
        assert_eq!(next(0, 0, RepeatMode::Off, false, true), None);
    }

    #[test]
    fn in_order_playback_walks_forward() {
        assert_eq!(next(0, 3, RepeatMode::Off, false, false), Some(1));
        assert_eq!(next(1, 3, RepeatMode::Off, false, false), Some(2));
    }

    #[test]
    fn the_end_of_a_queue_stops_so_radio_can_take_over() {
        assert_eq!(next(2, 3, RepeatMode::Off, false, false), None);
    }

    #[test]
    fn pressing_next_at_the_end_wraps_even_with_repeat_off() {
        assert_eq!(next(2, 3, RepeatMode::Off, false, true), Some(0));
    }

    #[test]
    fn repeat_all_wraps_at_the_natural_end() {
        assert_eq!(next(2, 3, RepeatMode::All, false, false), Some(0));
    }

    #[test]
    fn repeat_one_stays_on_the_same_track() {
        assert_eq!(next(1, 3, RepeatMode::One, false, false), Some(1));
        assert_eq!(next(1, 3, RepeatMode::One, false, true), Some(2));
    }

    #[test]
    fn previous_does_not_invent_a_neighbour() {
        assert_eq!(index_before(0, 0, RepeatMode::Off), None);
        assert_eq!(index_before(3, 3, RepeatMode::All), None);
        assert_eq!(index_before(0, 3, RepeatMode::Off), Some(0));
        assert_eq!(index_before(0, 3, RepeatMode::One), Some(0));
        assert_eq!(index_before(0, 3, RepeatMode::All), Some(2));
        assert_eq!(index_before(2, 3, RepeatMode::Off), Some(1));
        assert_eq!(next(3, 3, RepeatMode::One, true, true), None);
    }

    #[test]
    fn ad_handoff_waits_for_content_and_resumes_only_once() {
        let mut recovery = AdPlaybackRecovery::default();
        assert!(!recovery.observe(true, "playing", false));
        assert!(!recovery.observe(true, "ended", false));
        assert!(!recovery.observe(false, "ended", false));
        assert!(recovery.awaiting_content);
        assert!(recovery.observe(false, "paused", false));
        assert!(!recovery.observe(false, "paused", false));
        assert!(!recovery.observe(false, "playing", false));
        assert!(!recovery.awaiting_content);
        assert!(!recovery.observe(false, "paused", false));
        assert!(!recovery.observe(true, "paused", true));
        assert!(!recovery.observe(false, "paused", true));
        assert!(recovery.awaiting_content);
    }

    #[test]
    fn shuffle_never_picks_the_track_it_is_already_on() {
        for draw in 0..3 {
            let chosen = index_after(2, 4, RepeatMode::Off, true, false, |_| draw);
            assert_ne!(chosen, Some(2), "draw {draw} landed on the current track");
            assert!(chosen.is_some_and(|index| index < 4));
        }
        for _ in 0..200 {
            assert_ne!(
                index_after(2, 4, RepeatMode::Off, true, false, random_index),
                Some(2)
            );
        }
    }

    #[test]
    fn shuffle_reaches_every_other_track() {
        let reached: std::collections::HashSet<usize> = (0..3)
            .filter_map(|draw| index_after(1, 4, RepeatMode::Off, true, false, |_| draw))
            .collect();
        assert_eq!(reached, [0, 2, 3].into_iter().collect());
    }

    #[test]
    fn shuffle_over_a_single_track_stops_at_the_end_rather_than_looping() {
        assert_eq!(next(0, 1, RepeatMode::Off, true, false), None);
        assert_eq!(next(0, 1, RepeatMode::Off, true, true), Some(0));
    }

    #[test]
    fn time_text_covers_minutes_and_hours() {
        assert_eq!(time_text(0.0), "0:00");
        assert_eq!(time_text(9.0), "0:09");
        assert_eq!(time_text(222.0), "3:42");
        assert_eq!(time_text(3_723.0), "1:02:03");
    }

    #[test]
    fn time_text_refuses_to_render_nonsense() {
        assert_eq!(time_text(-1.0), "0:00");
        assert_eq!(time_text(f64::NAN), "0:00");
        assert_eq!(time_text(f64::INFINITY), "0:00");
    }

    #[test]
    fn nothing_is_seekable_before_the_host_reports_a_duration() {
        assert!(!is_seekable(0.0, Owner::OfficialWebView, false));
        assert!(!is_seekable(200.0, Owner::None, false));
        assert!(
            !is_seekable(200.0, Owner::OfficialWebView, true),
            "advertisements never seek"
        );
        assert!(is_seekable(200.0, Owner::OfficialWebView, false));
    }

    #[test]
    fn requests_are_clamped_and_nonsense_is_refused() {
        assert_eq!(clamp_seek(-5.0, 100.0), Some(0.0));
        assert_eq!(clamp_seek(500.0, 100.0), Some(100.0));
        assert_eq!(clamp_seek(f64::NAN, 100.0), None);
        assert_eq!(clamp_seek(10.0, 0.0), None);
        assert_eq!(clamp_volume(1.5), Some(1.0));
        assert_eq!(clamp_volume(-0.1), Some(0.0));
        assert_eq!(clamp_volume(f64::NAN), None);
    }

    /// The same cases as `VolumeTaperTests.swift` and the Windows `SliderFollowsAnAudioTaper`.
    #[test]
    fn the_volume_slider_follows_an_audio_taper() {
        for (position, gain) in [(0.0, 0.0), (0.5, 0.125), (1.0, 1.0), (1.5, 1.0)] {
            assert!(
                (volume_gain(position) - gain).abs() < 1e-10,
                "gain at {position}"
            );
            assert!(
                (volume_position(gain) - position.clamp(0.0, 1.0)).abs() < 1e-10,
                "position for {gain}"
            );
        }
        assert_eq!(volume_gain(f64::NAN), 0.0);
        assert_eq!(volume_position(-0.5), 0.0);
    }

    #[test]
    fn a_pending_seek_shows_until_it_lands_or_the_window_passes() {
        let requested_at = Instant::now();
        let seek = PendingSeek {
            position: 90.0,
            requested_at,
        };
        assert_eq!(displayed_position(10.0, Some(seek), requested_at), 90.0);
        assert_eq!(
            displayed_position(10.0, Some(seek), requested_at + SEEK_SETTLE_WINDOW),
            10.0
        );
        assert_eq!(displayed_position(10.0, None, requested_at), 10.0);
        assert!(
            seek.is_settled_by(89.0, requested_at),
            "landed within tolerance"
        );
        assert!(
            !seek.is_settled_by(10.0, requested_at),
            "still on the old position"
        );
        assert!(seek.is_settled_by(10.0, requested_at + SEEK_SETTLE_WINDOW));
    }

    fn lease(owner: Owner, generation: u64) -> PlaybackState {
        PlaybackState {
            account_id: None,
            owner,
            generation,
            sample_sequence: 0,
        }
    }

    fn sample(generation: u64, video_id: &str, current_time: f64) -> ReportedSample<'_> {
        ReportedSample {
            generation,
            video_id,
            current_time,
            duration: 200.0,
        }
    }

    #[test]
    fn a_sample_is_believed_only_from_the_lease_holder_for_its_own_load() {
        let official = lease(Owner::OfficialWebView, 4);
        let host = Owner::OfficialWebView;
        assert!(believes_sample(
            host,
            &official,
            Some("v"),
            &sample(4, "v", 10.0)
        ));
        assert!(!believes_sample(
            Owner::None,
            &official,
            Some("v"),
            &sample(4, "v", 10.0)
        ));
        assert!(
            !believes_sample(host, &official, Some("v"), &sample(3, "v", 10.0)),
            "stale generation"
        );
        assert!(
            !believes_sample(host, &official, Some("w"), &sample(4, "v", 10.0)),
            "another load"
        );
        assert!(
            !believes_sample(host, &official, None, &sample(4, "v", 10.0)),
            "nothing loaded"
        );
        assert!(!believes_sample(
            host,
            &official,
            Some("v"),
            &sample(4, "v", f64::NAN)
        ));
        assert!(!believes_sample(
            host,
            &official,
            Some("v"),
            &sample(4, "v", -1.0)
        ));
        assert!(!believes_sample(
            Owner::None,
            &lease(Owner::None, 4),
            Some("v"),
            &sample(4, "v", 1.0)
        ));
    }

    #[test]
    fn only_the_first_real_end_advances_the_queue() {
        let end = |state, is_advertisement, position| EndSample {
            state,
            is_advertisement,
            video_id: "v",
            position,
            duration: 229.0,
        };
        assert!(should_advance_after_end(
            &end("ended", false, 10.0),
            None,
            false
        ));
        assert!(should_advance_after_end(
            &end("ended", false, 10.0),
            Some("u"),
            false
        ));
        assert!(
            !should_advance_after_end(&end("ended", false, 10.0), Some("v"), false),
            "reported twice"
        );
        assert!(
            !should_advance_after_end(&end("ended", true, 10.0), None, false),
            "an advertisement ended"
        );
        assert!(!should_advance_after_end(
            &end("paused", false, 10.0),
            None,
            false
        ));
        assert!(
            should_advance_after_end(&end("paused", false, 228.8), None, false),
            "the official page stops just short of the end with Automix off"
        );
        assert!(
            !should_advance_after_end(&end("paused", false, 228.8), None, true),
            "the listener paused at the last second"
        );
    }

    #[test]
    fn a_pause_at_the_last_moment_is_the_end() {
        assert!(has_finished("ended", 10.0, 229.0, false));
        assert!(has_finished("paused", 228.8, 229.0, false));
        assert!(!has_finished("paused", 228.8, 229.0, true));
        assert!(!has_finished("paused", 120.0, 229.0, false));
        assert!(!has_finished("paused", 0.0, 0.0, false));
        assert!(!has_finished("playing", 229.0, 229.0, false));
        assert!(!has_finished("paused", f64::NAN, 229.0, false));
    }

    #[test]
    fn a_hidden_page_reapplies_the_chosen_volume_after_any_reset() {
        assert_eq!(
            volume_sync(false, false, 1.0, false, 0.4, false),
            VolumeSync::PushStored
        );
        assert_eq!(
            volume_sync(false, false, 0.4, true, 0.4, false),
            VolumeSync::PushStored
        );
        assert_eq!(
            volume_sync(false, false, 0.405, false, 0.4, false),
            VolumeSync::AlreadyMatches
        );
        assert_eq!(
            volume_sync(true, false, 1.0, false, 0.4, false),
            VolumeSync::PushStored
        );
        assert_eq!(
            volume_sync(true, false, 0.4, true, 0.4, false),
            VolumeSync::PushStored
        );
        assert_eq!(
            volume_sync(true, false, 0.4, false, 0.4, false),
            VolumeSync::AlreadyMatches
        );
        assert_eq!(
            volume_sync(false, true, 1.0, false, 0.4, false),
            VolumeSync::Ignore
        );
    }

    /// The Swift shell's merge lost this change. It is the case this port exists to keep.
    #[test]
    fn a_shuffle_change_made_while_a_volume_save_is_pending_is_not_lost() {
        let pending = PreferencesPatch {
            volume: Some(0.25),
            ..Default::default()
        };
        let merged = merge_preferences(
            Some(pending),
            PreferencesPatch {
                shuffle: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(merged.volume, Some(0.25));
        assert_eq!(merged.shuffle, Some(true));
        let merged = merge_preferences(
            Some(merged),
            PreferencesPatch {
                repeat_mode: Some("all".into()),
                volume: Some(0.5),
                ..Default::default()
            },
        );
        assert_eq!(merged.shuffle, Some(true));
        assert_eq!(merged.repeat_mode.as_deref(), Some("all"));
        assert_eq!(merged.volume, Some(0.5), "the newer value wins");

        let merged = merge_preferences(
            Some(merged),
            PreferencesPatch {
                artwork_background: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(merged.shuffle, Some(true));
        assert_eq!(merged.artwork_background, Some(false));
    }

    #[test]
    fn a_first_change_is_saved_as_it_is() {
        let update = PreferencesPatch {
            theme: Some("dark".into()),
            ..Default::default()
        };
        assert_eq!(merge_preferences(None, update.clone()), update);
    }

    #[test]
    fn the_now_playing_subtitle_falls_back_when_nothing_is_playing() {
        assert_eq!(now_playing_subtitle(None), "Choose a track to begin");
    }
}
