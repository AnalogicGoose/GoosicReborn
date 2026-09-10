//! The playback decisions a shell makes that Rust's authority does not.
//!
//! `goosic-core` decides who may play. What is here is everything a shell decides around that:
//! which queue position plays next, whether a requested seek or volume is meaningful, when a
//! pending seek has settled, whether a renderer's report is worth believing before it is
//! forwarded, when a finished track should advance, how a fresh page's volume is reconciled with
//! the user's, and how rapid preference changes are coalesced. In the Swift shell every one of
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
    if count == 0 {
        return None;
    }
    if repeat == RepeatMode::One {
        return Some(index);
    }
    if shuffle {
        if count == 1 {
            return (repeat == RepeatMode::All || wrapping).then_some(index);
        }
        if index >= count {
            return Some(pick(count) % count);
        }
        // Any position but the current one, so shuffle never repeats a track back to back. The
        // Swift shell drew until it missed; drawing from the other positions directly gives the
        // same distribution and cannot spin.
        let candidate = pick(count - 1) % (count - 1);
        return Some(if candidate >= index { candidate + 1 } else { candidate });
    }
    let next = index + 1;
    if next < count {
        return Some(next);
    }
    (repeat == RepeatMode::All || wrapping).then_some(0)
}

/// A position in `0..count` that differs from call to call. Not cryptographic, and it does not
/// need to be: it picks the next shuffled track.
pub fn random_index(count: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    if count == 0 {
        return 0;
    }
    // Each `RandomState` is keyed afresh, so hashing nothing still yields a new value per call.
    let value = std::collections::hash_map::RandomState::new().build_hasher().finish();
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

/// Whether a sample means the queue should move on.
///
/// Advertisements end too, and must never advance the queue; and a player reports `ended`
/// repeatedly, so only the first report for a video counts.
pub fn should_advance_after_end(
    state: &str,
    is_advertisement: bool,
    video_id: &str,
    last_ended_video_id: Option<&str>,
) -> bool {
    state == "ended" && !is_advertisement && last_ended_video_id != Some(video_id)
}

/// What to do with the volume a web player reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumeSync {
    /// Advertisements never touch the stored volume.
    Ignore,
    /// The stored preference has been applied to this load; follow what the player says.
    Follow { volume: f64, muted: bool },
    /// A fresh page started at its own volume. Push the stored preference once.
    PushStored,
    /// The page already matches the stored preference; nothing to push.
    AlreadyMatches,
}

/// Reconciles a fresh page's volume with the user's.
///
/// A new official page starts at whatever volume it likes. Until the stored preference has been
/// applied to this load, a difference means the page is wrong and the preference is pushed; after
/// that, the page is the truth, because the user may have changed the volume inside it.
pub fn volume_sync(
    applied_for_load: bool,
    is_advertisement: bool,
    reported_volume: f64,
    reported_muted: bool,
    stored_volume: f64,
    stored_muted: bool,
) -> VolumeSync {
    if is_advertisement {
        VolumeSync::Ignore
    } else if applied_for_load {
        VolumeSync::Follow { volume: reported_volume, muted: reported_muted }
    } else if (reported_volume - stored_volume).abs() > 0.01 || reported_muted != stored_muted {
        VolumeSync::PushStored
    } else {
        VolumeSync::AlreadyMatches
    }
}

/// Coalesces a preference change into one still waiting to be saved.
///
/// Every field is carried. The Swift shell's version copied six of the eight and dropped
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

    fn next(index: usize, count: usize, repeat: RepeatMode, shuffle: bool, wrapping: bool) -> Option<usize> {
        index_after(index, count, repeat, shuffle, wrapping, |n| n.saturating_sub(1))
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
        assert_eq!(next(1, 3, RepeatMode::One, false, true), Some(1));
    }

    #[test]
    fn shuffle_never_picks_the_track_it_is_already_on() {
        for draw in 0..3 {
            let chosen = index_after(2, 4, RepeatMode::Off, true, false, |_| draw);
            assert_ne!(chosen, Some(2), "draw {draw} landed on the current track");
            assert!(chosen.is_some_and(|index| index < 4));
        }
        for _ in 0..200 {
            assert_ne!(index_after(2, 4, RepeatMode::Off, true, false, random_index), Some(2));
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
        assert!(!is_seekable(200.0, Owner::OfficialWebView, true), "advertisements never seek");
        assert!(is_seekable(200.0, Owner::LocalDownloadedFile, false));
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

    #[test]
    fn a_pending_seek_shows_until_it_lands_or_the_window_passes() {
        let requested_at = Instant::now();
        let seek = PendingSeek { position: 90.0, requested_at };
        assert_eq!(displayed_position(10.0, Some(seek), requested_at), 90.0);
        assert_eq!(displayed_position(10.0, Some(seek), requested_at + SEEK_SETTLE_WINDOW), 10.0);
        assert_eq!(displayed_position(10.0, None, requested_at), 10.0);
        assert!(seek.is_settled_by(89.0, requested_at), "landed within tolerance");
        assert!(!seek.is_settled_by(10.0, requested_at), "still on the old position");
        assert!(seek.is_settled_by(10.0, requested_at + SEEK_SETTLE_WINDOW));
    }

    fn lease(owner: Owner, generation: u64) -> PlaybackState {
        PlaybackState { account_id: None, owner, generation, sample_sequence: 0 }
    }

    fn sample(generation: u64, video_id: &str, current_time: f64) -> ReportedSample<'_> {
        ReportedSample { generation, video_id, current_time, duration: 200.0 }
    }

    #[test]
    fn a_sample_is_believed_only_from_the_lease_holder_for_its_own_load() {
        let official = lease(Owner::OfficialWebView, 4);
        let host = Owner::OfficialWebView;
        assert!(believes_sample(host, &official, Some("v"), &sample(4, "v", 10.0)));
        assert!(!believes_sample(Owner::LocalDownloadedFile, &official, Some("v"), &sample(4, "v", 10.0)));
        assert!(!believes_sample(host, &official, Some("v"), &sample(3, "v", 10.0)), "stale generation");
        assert!(!believes_sample(host, &official, Some("w"), &sample(4, "v", 10.0)), "another load");
        assert!(!believes_sample(host, &official, None, &sample(4, "v", 10.0)), "nothing loaded");
        assert!(!believes_sample(host, &official, Some("v"), &sample(4, "v", f64::NAN)));
        assert!(!believes_sample(host, &official, Some("v"), &sample(4, "v", -1.0)));
        assert!(!believes_sample(Owner::None, &lease(Owner::None, 4), Some("v"), &sample(4, "v", 1.0)));
    }

    #[test]
    fn only_the_first_real_end_advances_the_queue() {
        assert!(should_advance_after_end("ended", false, "v", None));
        assert!(should_advance_after_end("ended", false, "v", Some("u")));
        assert!(!should_advance_after_end("ended", false, "v", Some("v")), "reported twice");
        assert!(!should_advance_after_end("ended", true, "v", None), "an advertisement ended");
        assert!(!should_advance_after_end("paused", false, "v", None));
    }

    #[test]
    fn a_fresh_page_gets_the_stored_volume_once_and_is_followed_after() {
        assert_eq!(volume_sync(false, false, 1.0, false, 0.4, false), VolumeSync::PushStored);
        assert_eq!(volume_sync(false, false, 0.4, true, 0.4, false), VolumeSync::PushStored);
        assert_eq!(volume_sync(false, false, 0.405, false, 0.4, false), VolumeSync::AlreadyMatches);
        assert_eq!(
            volume_sync(true, false, 0.7, false, 0.4, false),
            VolumeSync::Follow { volume: 0.7, muted: false }
        );
        assert_eq!(volume_sync(false, true, 1.0, false, 0.4, false), VolumeSync::Ignore);
    }

    /// The Swift shell's merge lost this change. It is the case this port exists to keep.
    #[test]
    fn a_shuffle_change_made_while_a_volume_save_is_pending_is_not_lost() {
        let pending = PreferencesPatch { volume: Some(0.25), ..Default::default() };
        let merged = merge_preferences(
            Some(pending),
            PreferencesPatch { shuffle: Some(true), ..Default::default() },
        );
        assert_eq!(merged.volume, Some(0.25));
        assert_eq!(merged.shuffle, Some(true));
        let merged = merge_preferences(
            Some(merged),
            PreferencesPatch { repeat_mode: Some("all".into()), volume: Some(0.5), ..Default::default() },
        );
        assert_eq!(merged.shuffle, Some(true));
        assert_eq!(merged.repeat_mode.as_deref(), Some("all"));
        assert_eq!(merged.volume, Some(0.5), "the newer value wins");
    }

    #[test]
    fn a_first_change_is_saved_as_it_is() {
        let update = PreferencesPatch { theme: Some("dark".into()), ..Default::default() };
        assert_eq!(merge_preferences(None, update.clone()), update);
    }

    #[test]
    fn the_now_playing_subtitle_falls_back_when_nothing_is_playing() {
        assert_eq!(now_playing_subtitle(None), "Choose a track to begin");
    }
}
