//! The listening queue as one state machine, shared by every shell.
//!
//! Each shell used to keep its own queue, and the copies drifted. A song chosen from a search
//! played the search list in order on macOS and started a radio on Windows; shuffle jumped to a
//! random position on one and reordered what was left on the other; Next on the last song wrapped
//! to the top on one and continued with recommendations on the other. None of those differences
//! had anything to do with an operating system. They were two hands writing the same rule.
//!
//! [`QueueEngine`] is that rule written once. A shell tells it what happened — a song was chosen,
//! Next was pressed, the track ended, a page of recommendations arrived — and the engine answers
//! with what to do ([`Effect`]) and what the queue now is ([`Snapshot`]). It holds no audio, no
//! WebView, no network and no clock: it asks the shell to fetch recommendations instead of
//! fetching them, because an account's recommendations are read inside that account's web
//! profile, where its cookies live, and nowhere else. Whether a track may actually play is still
//! `goosic-core`'s answer, reached through the playback lease; the engine only decides what to
//! ask for.
//!
//! Entries are identified by an id the engine assigns, not by position or by video id. A queue may
//! hold the same song twice, and the one asked for is the one with that id. A shell keeps its own
//! rich track objects keyed by those ids; the engine needs only each song's video id and title.

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::navigation::RepeatMode;
use crate::player_text;
use crate::queue::UNDO_WINDOW_SECONDS;

/// How many recommendations a running radio keeps ahead of the playing song, so Next never
/// waits on the network.
pub const RADIO_LOOKAHEAD: usize = 3;

/// How many recently played songs are kept out of recommendations.
pub const RECENTLY_PLAYED_LIMIT: usize = 200;

/// How far into a song Previous restarts it instead of going back, as every player does.
pub const RESTART_THRESHOLD_SECONDS: f64 = 3.0;

/// A song as the engine needs it: what to load, and what to call it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueTrack {
    pub video_id: String,
    #[serde(default)]
    pub title: String,
}

impl QueueTrack {
    pub fn new(video_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            video_id: video_id.into(),
            title: title.into(),
        }
    }
}

/// How a chosen song starts playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchKind {
    /// The list the song was chosen from, in its order: an album, a playlist, Liked Music, a
    /// collection's Play button.
    Ordered,
    /// The song, then YouTube Music's recommendations after it: a search result, a shelf card,
    /// a quick pick, Start Radio.
    Station,
}

/// Where a song was chosen, which decides how it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchSurface {
    Album,
    Playlist,
    LikedMusic,
    /// An artist page's songs, or its Play button.
    Artist,
    Search,
    Shelf,
    /// The Up Next list or history itself.
    Queue,
}

/// Whether choosing a song on `surface` plays that list in order or starts a station from it.
///
/// Albums, playlists, Liked Music and an artist's songs are lists someone means to hear in
/// order. A search result or a shelf card is one song chosen on its own; queuing its neighbours
/// is what turned a search for one song into a queue of unrelated ones.
pub fn launch_kind_for(surface: LaunchSurface) -> LaunchKind {
    match surface {
        LaunchSurface::Album
        | LaunchSurface::Playlist
        | LaunchSurface::LikedMusic
        | LaunchSurface::Artist
        | LaunchSurface::Queue => LaunchKind::Ordered,
        LaunchSurface::Search | LaunchSurface::Shelf => LaunchKind::Station,
    }
}

/// Something that happened, which the engine turns into effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Command {
    /// Restores saved preferences without treating them as a change to save.
    Configure {
        shuffle: bool,
        repeat: RepeatMode,
        autoplay: bool,
    },
    /// The account whose recommendations a radio uses, or none for the anonymous catalog's.
    SetRadioAccount {
        account_id: Option<String>,
    },
    /// A song chosen on a page. `start` is its position in `tracks`; `None` starts at the top,
    /// or anywhere when shuffled.
    ///
    /// `shuffle` is set by a page's Play and Shuffle buttons, which say how the list should play:
    /// Play in order, Shuffle shuffled, and the listener's preference follows. A row chosen from
    /// the list leaves it out and plays under the preference as it stands.
    Launch {
        tracks: Vec<QueueTrack>,
        start: Option<usize>,
        kind: LaunchKind,
        #[serde(default)]
        source_title: Option<String>,
        #[serde(default)]
        source_key: Option<String>,
        #[serde(default)]
        shuffle: Option<bool>,
    },
    /// Start Radio: the song, then its recommendations.
    StartRadio {
        track: QueueTrack,
    },
    Next,
    /// `position` is how far into the playing song the listener is, when it is seekable.
    Previous {
        #[serde(default)]
        position: f64,
    },
    /// The playing song finished by itself.
    Ended,
    /// An entry already in the queue was chosen.
    Jump {
        entry: u64,
    },
    PlayNext {
        track: QueueTrack,
    },
    AddToQueue {
        track: QueueTrack,
    },
    Remove {
        entry: u64,
    },
    /// The queue's entries in a new order. The playing entry stays the playing one.
    Reorder {
        entries: Vec<u64>,
    },
    ClearUpcoming {
        now: f64,
    },
    UndoClear {
        now: f64,
    },
    ToggleShuffle,
    CycleRepeat,
    SetAutoplay {
        enabled: bool,
    },
    SetSleepAtEnd {
        enabled: bool,
    },
    /// The official player refused this song; it is left out of lists and recommendations for
    /// the rest of the session.
    Refused {
        video_id: String,
    },
    /// A page of recommendations answered `request`.
    RadioLoaded {
        request: u64,
        tracks: Vec<QueueTrack>,
        #[serde(default)]
        next_cursor: Option<String>,
    },
    RadioFailed {
        request: u64,
    },
    /// Forgets the queue, as an account change does. Preferences and refusals stay.
    Reset,
    /// Asks for the current snapshot without changing anything.
    Snapshot,
}

/// What the shell should do now. Effects are listed in the order they apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Effect {
    /// Load and play this entry from its start.
    Play { entry: u64 },
    /// Seek the playing song back to its start; nothing else changes.
    SeekToStart,
    /// Fetch a page of recommendations and answer with `RadioLoaded` or `RadioFailed`.
    FetchRadio {
        request: u64,
        seed_video_id: String,
        continuation: Option<String>,
        account_id: Option<String>,
    },
    /// The sleep timer's "end of this song" has been reached: stop, and play nothing else.
    StopForSleep,
    /// Shuffle or repeat changed at the listener's request; store them.
    SavePreferences { shuffle: bool, repeat: RepeatMode },
    /// A sentence for the status line or an accessibility announcement.
    Notice { text: String },
}

/// An entry the engine created from one of the tracks a command carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    /// The track's position in the command's list.
    pub input: usize,
    pub entry: u64,
}

/// The queue as a shell draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Every entry, history included, in play order.
    pub entries: Vec<u64>,
    /// The playing entry's position in `entries`.
    pub current: Option<usize>,
    pub source_title: Option<String>,
    pub source_key: Option<String>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub autoplay: bool,
    pub sleep_at_end: bool,
    /// A radio is running behind the queue.
    pub radio: bool,
    /// Recommendations are on their way.
    pub radio_loading: bool,
    /// How many entries follow the playing one.
    pub up_next_count: usize,
    /// "From Liked Music · 99 songs".
    pub up_next_text: String,
    pub can_clear: bool,
    /// Until when the last Clear may be undone, on the clock the shell passed with it. `None`
    /// once anything else has changed the queue.
    pub undo_deadline: Option<f64>,
    /// Bumped by every change to the queue's contents or order.
    pub revision: u64,
}

/// What one command produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub effects: Vec<Effect>,
    pub added: Vec<Added>,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    id: u64,
    track: QueueTrack,
}

/// Where recommendations come from, so a continuation is only used with the reader that issued
/// it.
#[derive(Debug, Clone, PartialEq)]
struct Station {
    seed: String,
    account_id: Option<String>,
    continuation: Option<String>,
    loaded: bool,
}

impl Station {
    fn new(seed: impl Into<String>, account_id: Option<String>) -> Self {
        Self {
            seed: seed.into(),
            account_id,
            continuation: None,
            loaded: false,
        }
    }

    fn is_personal(&self) -> bool {
        self.account_id.is_some()
    }

    /// After the first page, a station with no cursor is exhausted. Re-seeding from the last
    /// recommendation is what made long queues wander into unrelated music.
    fn can_load_more(&self) -> bool {
        !self.loaded || self.continuation.is_some()
    }

    /// An account's cursor means nothing to another reader; the anonymous catalog's works for
    /// anyone.
    fn usable_with(&self, account_id: Option<&str>) -> bool {
        !self.is_personal() || self.account_id.as_deref() == account_id
    }

    /// A cursor that repeats the one just used would loop forever, so it ends the station.
    fn after(&mut self, next_cursor: Option<String>) {
        self.loaded = true;
        self.continuation = next_cursor.filter(|cursor| Some(cursor) != self.continuation.as_ref());
    }
}

#[derive(Debug, Clone)]
struct PendingRadio {
    request: u64,
    station_revision: u64,
}

#[derive(Debug, Clone)]
struct Cleared {
    removed: Vec<Entry>,
    unshuffled: Option<Vec<u64>>,
    station: Option<Station>,
    playing: u64,
    revision: u64,
    deadline: f64,
}

/// The shared queue. See the module documentation.
#[derive(Debug)]
pub struct QueueEngine {
    entries: Vec<Entry>,
    current: Option<usize>,
    /// Upcoming entries in the order they were queued, kept while shuffle is on.
    unshuffled: Option<Vec<u64>>,
    source_title: Option<String>,
    source_key: Option<String>,
    shuffle: bool,
    repeat: RepeatMode,
    autoplay: bool,
    sleep_at_end: bool,
    station: Option<Station>,
    /// Bumped whenever the station is replaced, so a late page for the old one is dropped.
    station_revision: u64,
    pending_radio: Option<PendingRadio>,
    /// A move ran out of songs and is waiting for recommendations. Holds the play it was made
    /// from: anything chosen in the meantime supersedes it.
    awaiting_more: Option<u64>,
    radio_account: Option<String>,
    recently_played: VecDeque<String>,
    refused: HashSet<String>,
    cleared: Option<Cleared>,
    revision: u64,
    next_entry: u64,
    next_request: u64,
    /// Bumped by every Play effect, so one end is acted on once.
    play_sequence: u64,
    end_handled: Option<u64>,
    rng: u64,
}

impl Default for QueueEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueEngine {
    pub fn new() -> Self {
        Self::with_seed(crate::playback::random_index(usize::MAX) as u64)
    }

    /// An engine whose shuffles are repeatable, for tests.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            entries: Vec::new(),
            current: None,
            unshuffled: None,
            source_title: None,
            source_key: None,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: true,
            sleep_at_end: false,
            station: None,
            station_revision: 0,
            pending_radio: None,
            awaiting_more: None,
            radio_account: None,
            recently_played: VecDeque::new(),
            refused: HashSet::new(),
            cleared: None,
            revision: 0,
            next_entry: 1,
            next_request: 1,
            play_sequence: 0,
            end_handled: None,
            // Zero would stick xorshift at zero forever.
            rng: seed | 1,
        }
    }

    /// Applies one command.
    pub fn handle(&mut self, command: Command) -> Outcome {
        let mut out = Output::default();
        match command {
            Command::Configure {
                shuffle,
                repeat,
                autoplay,
            } => {
                self.shuffle = shuffle;
                self.repeat = repeat;
                self.autoplay = autoplay;
            }
            Command::SetRadioAccount { account_id } => self.radio_account = account_id,
            Command::Launch {
                tracks,
                start,
                kind,
                source_title,
                source_key,
                shuffle,
            } => self.launch(
                &mut out,
                tracks,
                start,
                kind,
                source_title,
                source_key,
                shuffle,
            ),
            Command::StartRadio { track } => self.launch(
                &mut out,
                vec![track],
                Some(0),
                LaunchKind::Station,
                None,
                None,
                None,
            ),
            Command::Next => self.advance(&mut out, true, false),
            Command::Previous { position } => {
                if self.current.is_some()
                    && position.is_finite()
                    && position > RESTART_THRESHOLD_SECONDS
                {
                    out.effects.push(Effect::SeekToStart);
                } else {
                    self.advance(&mut out, false, false);
                }
            }
            Command::Ended => self.ended(&mut out),
            Command::Jump { entry } => {
                if let Some(index) = self.index_of(entry) {
                    self.point(&mut out, index);
                }
            }
            Command::PlayNext { track } => self.enqueue(&mut out, track, true),
            Command::AddToQueue { track } => self.enqueue(&mut out, track, false),
            Command::Remove { entry } => self.remove(&mut out, entry),
            Command::Reorder { entries } => self.reorder(entries),
            Command::ClearUpcoming { now } => self.clear_upcoming(&mut out, now),
            Command::UndoClear { now } => self.undo_clear(&mut out, now),
            Command::ToggleShuffle => self.toggle_shuffle(&mut out),
            Command::CycleRepeat => {
                self.repeat = self.repeat.next();
                out.effects.push(self.save_preferences());
            }
            Command::SetAutoplay { enabled } => self.autoplay = enabled,
            Command::SetSleepAtEnd { enabled } => self.sleep_at_end = enabled,
            Command::Refused { video_id } => {
                if !video_id.is_empty() {
                    self.refused.insert(video_id);
                }
            }
            Command::RadioLoaded {
                request,
                tracks,
                next_cursor,
            } => self.radio_loaded(&mut out, request, tracks, next_cursor),
            Command::RadioFailed { request } => self.radio_failed(&mut out, request),
            Command::Reset => self.reset(),
            Command::Snapshot => {}
        }
        Outcome {
            effects: out.effects,
            added: out.added,
            snapshot: self.snapshot(),
        }
    }

    /// The queue as it stands.
    pub fn snapshot(&self) -> Snapshot {
        let up_next_count = self.upcoming_count();
        let radio_loading = self.pending_radio.is_some();
        let playing = self.current_entry().map(|entry| entry.id);
        let undo_deadline = self.cleared.as_ref().and_then(|cleared| {
            (cleared.revision == self.revision && Some(cleared.playing) == playing)
                .then_some(cleared.deadline)
        });
        Snapshot {
            entries: self.entries.iter().map(|entry| entry.id).collect(),
            current: self.current,
            source_title: self.source_title.clone(),
            source_key: self.source_key.clone(),
            shuffle: self.shuffle,
            repeat: self.repeat,
            autoplay: self.autoplay,
            sleep_at_end: self.sleep_at_end,
            radio: self.station.is_some(),
            radio_loading,
            up_next_count,
            up_next_text: player_text::up_next(
                self.source_title.as_deref(),
                up_next_count,
                radio_loading,
            ),
            can_clear: playing.is_some() && up_next_count > 0,
            undo_deadline,
            revision: self.revision,
        }
    }

    /// The track behind an entry id.
    pub fn track(&self, entry: u64) -> Option<&QueueTrack> {
        self.entries
            .iter()
            .find(|candidate| candidate.id == entry)
            .map(|entry| &entry.track)
    }

    // ---- Launching ---------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn launch(
        &mut self,
        out: &mut Output,
        tracks: Vec<QueueTrack>,
        start: Option<usize>,
        kind: LaunchKind,
        source_title: Option<String>,
        source_key: Option<String>,
        shuffle: Option<bool>,
    ) {
        let start = start.filter(|index| *index < tracks.len());
        let tracks: Vec<QueueTrack> = match (kind, start) {
            // A station starts from the one song; its list was only where it was found.
            (LaunchKind::Station, Some(index)) => vec![tracks[index].clone()],
            (LaunchKind::Station, None) => tracks.into_iter().take(1).collect(),
            (LaunchKind::Ordered, _) => tracks,
        };
        let start = if kind == LaunchKind::Station {
            Some(0).filter(|_| !tracks.is_empty())
        } else {
            start
        };

        // Songs the player refused this session are left out, unless one is the song chosen:
        // asking for it again is the listener's call.
        let mut kept: Vec<(usize, QueueTrack)> = Vec::with_capacity(tracks.len());
        for (input, track) in tracks.into_iter().enumerate() {
            let chosen = Some(input) == start;
            if track.video_id.is_empty() || (!chosen && self.refused.contains(&track.video_id)) {
                continue;
            }
            kept.push((input, track));
        }
        if kept.is_empty() {
            out.notice("Nothing here can be played.");
            return;
        }

        self.station = None;
        self.station_revision += 1;
        self.pending_radio = None;
        self.awaiting_more = None;
        self.unshuffled = None;
        self.cleared = None;
        self.entries.clear();

        let mut first = None;
        for (input, track) in kept {
            let id = self.allocate();
            if Some(input) == start {
                first = Some(self.entries.len());
            }
            out.added.push(Added { input, entry: id });
            self.entries.push(Entry { id, track });
        }

        let preference_changed = shuffle.is_some_and(|shuffle| shuffle != self.shuffle);
        if let Some(shuffle) = shuffle {
            self.shuffle = shuffle;
        }
        if self.shuffle && first.is_none() {
            first = Some(self.random_below(self.entries.len()));
        }
        let mut first = first.unwrap_or(0);
        if self.shuffle && self.entries.len() > 1 {
            // The chosen song leads and the rest are shuffled after it: a song placed before it
            // would be one Previous reaches without it ever having played.
            let leading = self.entries.remove(first);
            self.unshuffled = Some(self.entries.iter().map(|entry| entry.id).collect());
            self.shuffle_slice_from(0);
            self.entries.insert(0, leading);
            first = 0;
        }
        self.revision += 1;
        if preference_changed {
            out.effects.push(self.save_preferences());
        }

        match kind {
            LaunchKind::Station => {
                let seed = &self.entries[first].track;
                self.source_title = Some(player_text::radio_source(&seed.title));
                self.source_key = None;
                self.station = Some(Station::new(
                    seed.video_id.clone(),
                    self.radio_account.clone(),
                ));
            }
            LaunchKind::Ordered => {
                self.source_title = source_title.filter(|title| !title.is_empty());
                self.source_key = source_key;
            }
        }
        self.point(out, first);
    }

    // ---- Moving ------------------------------------------------------------------------------

    /// Makes the entry at `index` the playing one and asks for it to be loaded.
    fn point(&mut self, out: &mut Output, index: usize) {
        self.current = Some(index);
        self.play_sequence += 1;
        self.awaiting_more = None;
        let video_id = self.entries[index].track.video_id.clone();
        self.remember_played(video_id);
        out.effects.push(Effect::Play {
            entry: self.entries[index].id,
        });
        self.top_up(out);
    }

    fn ended(&mut self, out: &mut Output) {
        if self.current.is_none() || self.end_handled == Some(self.play_sequence) {
            // The player reports the end more than once; only the first counts.
            return;
        }
        self.end_handled = Some(self.play_sequence);
        if self.sleep_at_end {
            // "End of this song" is honoured exactly where a natural end would otherwise advance.
            self.sleep_at_end = false;
            out.effects.push(Effect::StopForSleep);
            out.notice("Sleep timer stopped the music at the end of the song.");
            return;
        }
        self.advance(out, true, true);
    }

    /// Next, Previous or a natural end.
    fn advance(&mut self, out: &mut Output, forward: bool, natural: bool) {
        let Some(index) = self.current.filter(|index| *index < self.entries.len()) else {
            out.notice("Choose a track to begin.");
            return;
        };
        match decide_move(self.entries.len(), index, forward, natural, self.repeat) {
            Move::Play(target) => self.point(out, target),
            // Repeat-one at a natural end loads the song afresh: the page has already finished it.
            Move::Restart if natural => self.point(out, index),
            Move::Restart => out.effects.push(Effect::SeekToStart),
            Move::NeedMore => self.continue_with_radio(out, index, natural),
        }
    }

    /// The queue ran out going forward. A running station continues when skipped into, and on a
    /// natural end when autoplay is on; without one, autoplay starts one from the last song.
    fn continue_with_radio(&mut self, out: &mut Output, index: usize, natural: bool) {
        let may_continue = if self.station.is_some() {
            self.autoplay || !natural
        } else {
            self.autoplay
        };
        if !may_continue {
            out.notice(if natural && !self.autoplay {
                "Track finished. Autoplay is off."
            } else {
                "The queue has finished."
            });
            return;
        }
        let account = self.radio_account.clone();
        let usable = self
            .station
            .as_ref()
            .is_some_and(|station| station.usable_with(account.as_deref()));
        if !usable {
            let seed = self.entries[index].track.video_id.clone();
            self.station = Some(Station::new(seed, account));
            self.station_revision += 1;
            self.pending_radio = None;
        }
        if !self.station.as_ref().is_some_and(Station::can_load_more) {
            out.notice("The queue has finished.");
            return;
        }
        self.awaiting_more = Some(self.play_sequence);
        self.request_radio(out);
    }

    // ---- Radio -------------------------------------------------------------------------------

    /// Keeps a few recommendations ahead of the song playing, so Next never waits.
    fn top_up(&mut self, out: &mut Output) {
        if self.station.as_ref().is_some_and(Station::can_load_more)
            && self.upcoming_count() < RADIO_LOOKAHEAD
        {
            self.request_radio(out);
        }
    }

    /// Asks for the station's next page, unless one is already on its way.
    fn request_radio(&mut self, out: &mut Output) {
        if self.pending_radio.is_some() {
            return;
        }
        let Some(station) = self
            .station
            .as_ref()
            .filter(|station| station.can_load_more())
        else {
            return;
        };
        let request = self.next_request;
        self.next_request += 1;
        out.effects.push(Effect::FetchRadio {
            request,
            seed_video_id: station.seed.clone(),
            continuation: station.continuation.clone(),
            account_id: station.account_id.clone(),
        });
        self.pending_radio = Some(PendingRadio {
            request,
            station_revision: self.station_revision,
        });
    }

    /// Takes the pending request if `request` is it and still belongs to the current station.
    fn take_pending(&mut self, request: u64) -> Option<bool> {
        let pending = self.pending_radio.as_ref()?;
        if pending.request != request {
            return None;
        }
        let current = pending.station_revision == self.station_revision;
        self.pending_radio = None;
        Some(current)
    }

    fn radio_loaded(
        &mut self,
        out: &mut Output,
        request: u64,
        tracks: Vec<QueueTrack>,
        next_cursor: Option<String>,
    ) {
        if self.take_pending(request) != Some(true) {
            // A late page for a queue that has since been replaced or cleared.
            return;
        }
        let account = self.radio_account.clone();
        let Some(station) = self.station.as_mut() else {
            return;
        };
        if !station.usable_with(account.as_deref()) {
            return;
        }
        station.after(next_cursor);

        // Recommendations worth adding: none already queued, recently played or refused, and no
        // repeats within the page. A listener's own queue may hold a song twice; a radio may not,
        // because a repeated recommendation reads as a broken "up next".
        let mut seen: HashSet<String> = self
            .entries
            .iter()
            .map(|entry| entry.track.video_id.clone())
            .chain(self.recently_played.iter().cloned())
            .chain(self.refused.iter().cloned())
            .collect();
        let first_new = self.entries.len();
        for (input, track) in tracks.into_iter().enumerate() {
            if track.video_id.is_empty() || !seen.insert(track.video_id.clone()) {
                continue;
            }
            let id = self.allocate();
            out.added.push(Added { input, entry: id });
            if let Some(unshuffled) = self.unshuffled.as_mut() {
                unshuffled.push(id);
            }
            self.entries.push(Entry { id, track });
        }
        let grew = self.entries.len() > first_new;
        if grew {
            self.revision += 1;
        }

        if self.awaiting_more.take() == Some(self.play_sequence) {
            if grew {
                self.point(out, first_new);
            } else {
                out.notice("The queue has finished.");
            }
        } else if grew {
            // A short page leaves the lookahead short. Only a page that added something asks
            // again, so a station answering with nothing new cannot spin.
            self.top_up(out);
        }
    }

    fn radio_failed(&mut self, out: &mut Output, request: u64) {
        if self.take_pending(request) != Some(true) {
            return;
        }
        let fallback = self
            .station
            .as_ref()
            .filter(|station| station.is_personal() && !station.loaded)
            .map(|station| station.seed.clone());
        if let Some(seed) = fallback {
            // The account's reader could not answer; the anonymous catalog still can.
            self.station = Some(Station::new(seed, None));
            self.request_radio(out);
            return;
        }
        out.notice("Could not load recommendations.");
        if self.awaiting_more.take() == Some(self.play_sequence) {
            out.notice("The queue has finished.");
        }
    }

    // ---- Editing -----------------------------------------------------------------------------

    fn enqueue(&mut self, out: &mut Output, track: QueueTrack, next: bool) {
        if track.video_id.is_empty() {
            out.notice("That row does not carry a playable track.");
            return;
        }
        let Some(current) = self.current else {
            // With nothing playing there is nothing to queue behind, so the song simply plays.
            self.launch(
                out,
                vec![track],
                Some(0),
                LaunchKind::Ordered,
                None,
                None,
                None,
            );
            return;
        };
        let title = track.title.clone();
        let id = self.allocate();
        if next {
            self.entries.insert(current + 1, Entry { id, track });
        } else {
            self.entries.push(Entry { id, track });
        }
        // Kept in the same place in the unshuffled order, so turning shuffle off leaves it next.
        if let Some(unshuffled) = self.unshuffled.as_mut() {
            if next {
                unshuffled.insert(0, id);
            } else {
                unshuffled.push(id);
            }
        }
        out.added.push(Added {
            input: 0,
            entry: id,
        });
        self.revision += 1;
        out.notice(&if next {
            format!("\u{201C}{title}\u{201D} plays next.")
        } else {
            format!("\u{201C}{title}\u{201D} was added to the queue.")
        });
    }

    fn remove(&mut self, out: &mut Output, entry: u64) {
        let Some(index) = self.index_of(entry) else {
            return;
        };
        if Some(index) == self.current {
            // Removing the playing song would stop the music, which is what Pause is for.
            out.notice("The track that is playing stays in the queue.");
            return;
        }
        self.entries.remove(index);
        if let Some(current) = self.current.as_mut() {
            if index < *current {
                *current -= 1;
            }
        }
        if let Some(unshuffled) = self.unshuffled.as_mut() {
            unshuffled.retain(|id| *id != entry);
        }
        self.revision += 1;
    }

    fn reorder(&mut self, order: Vec<u64>) {
        let mut wanted = order.clone();
        wanted.sort_unstable();
        let mut have: Vec<u64> = self.entries.iter().map(|entry| entry.id).collect();
        have.sort_unstable();
        if wanted != have || order.iter().copied().eq(self.entries.iter().map(|e| e.id)) {
            return;
        }
        let playing = self.current_entry().map(|entry| entry.id);
        let mut entries = std::mem::take(&mut self.entries);
        let mut reordered = Vec::with_capacity(entries.len());
        for id in order {
            if let Some(position) = entries.iter().position(|entry| entry.id == id) {
                reordered.push(entries.swap_remove(position));
            }
        }
        self.entries = reordered;
        self.current = playing.and_then(|id| self.index_of(id));
        self.revision += 1;
    }

    /// Clears everything after the playing song, keeping it for Undo. Songs already played stay,
    /// so Previous still reaches them.
    fn clear_upcoming(&mut self, out: &mut Output, now: f64) {
        let Some(current) = self.current else {
            return;
        };
        if self.upcoming_count() == 0 {
            return;
        }
        let removed = self.entries.split_off(current + 1);
        let playing = self.entries[current].id;
        let previous_unshuffled = self.unshuffled.clone();
        self.unshuffled = self.shuffle.then(Vec::new);
        let station = self.station.take();
        // A late page or an autoplay continuation must not refill a deliberate clear.
        self.station_revision += 1;
        self.pending_radio = None;
        self.awaiting_more = None;
        self.revision += 1;
        self.cleared = Some(Cleared {
            removed,
            unshuffled: previous_unshuffled,
            station,
            playing,
            revision: self.revision,
            deadline: now + UNDO_WINDOW_SECONDS,
        });
        out.notice("Cleared Up Next");
    }

    fn undo_clear(&mut self, out: &mut Output, now: f64) {
        let playing = self.current_entry().map(|entry| entry.id);
        let Some(cleared) = self.cleared.take() else {
            return;
        };
        let valid = cleared.revision == self.revision
            && Some(cleared.playing) == playing
            && crate::queue::can_undo(cleared.revision, self.revision, cleared.deadline, now);
        if !valid {
            self.cleared = Some(cleared);
            return;
        }
        self.entries.extend(cleared.removed);
        self.unshuffled = cleared.unshuffled;
        self.station = cleared.station;
        self.station_revision += 1;
        self.revision += 1;
        out.notice("Restored Up Next");
    }

    fn toggle_shuffle(&mut self, out: &mut Output) {
        if self.shuffle {
            // Put the upcoming entries back in the order they were queued.
            if let (Some(order), Some(current)) = (self.unshuffled.take(), self.current) {
                let upcoming: Vec<Entry> = self.entries.split_off(current + 1);
                let mut restored: Vec<Entry> = order
                    .iter()
                    .filter_map(|id| upcoming.iter().find(|entry| entry.id == *id).cloned())
                    .collect();
                // Anything queued without being recorded keeps its shuffled place at the end.
                for entry in upcoming {
                    if !order.contains(&entry.id) {
                        restored.push(entry);
                    }
                }
                self.entries.extend(restored);
                self.revision += 1;
            }
            self.unshuffled = None;
            self.shuffle = false;
        } else {
            self.shuffle = true;
            // Only what is after the playing song is shuffled; what already played stays put.
            let from = self.current.map_or(0, |current| current + 1);
            self.unshuffled = Some(self.entries[from..].iter().map(|entry| entry.id).collect());
            if self.entries.len() > from + 1 {
                self.shuffle_slice_from(from);
                self.revision += 1;
            }
        }
        out.effects.push(self.save_preferences());
    }

    fn reset(&mut self) {
        self.entries.clear();
        self.current = None;
        self.unshuffled = None;
        self.source_title = None;
        self.source_key = None;
        self.station = None;
        self.station_revision += 1;
        self.pending_radio = None;
        self.awaiting_more = None;
        self.cleared = None;
        self.sleep_at_end = false;
        self.recently_played.clear();
        self.revision += 1;
    }

    // ---- Helpers -----------------------------------------------------------------------------

    fn current_entry(&self) -> Option<&Entry> {
        self.current.and_then(|index| self.entries.get(index))
    }

    fn upcoming_count(&self) -> usize {
        match self.current {
            Some(current) => self.entries.len().saturating_sub(current + 1),
            None => 0,
        }
    }

    fn index_of(&self, entry: u64) -> Option<usize> {
        self.entries
            .iter()
            .position(|candidate| candidate.id == entry)
    }

    fn allocate(&mut self) -> u64 {
        let id = self.next_entry;
        self.next_entry += 1;
        id
    }

    fn remember_played(&mut self, video_id: String) {
        if video_id.is_empty() {
            return;
        }
        self.recently_played.retain(|id| *id != video_id);
        self.recently_played.push_back(video_id);
        while self.recently_played.len() > RECENTLY_PLAYED_LIMIT {
            self.recently_played.pop_front();
        }
    }

    fn save_preferences(&self) -> Effect {
        Effect::SavePreferences {
            shuffle: self.shuffle,
            repeat: self.repeat,
        }
    }

    /// xorshift64*: not cryptographic, and it does not need to be. It orders a shuffle.
    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn random_below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_random() % bound as u64) as usize
        }
    }

    /// Fisher–Yates over `entries[from..]`.
    fn shuffle_slice_from(&mut self, from: usize) {
        let len = self.entries.len();
        if len <= from + 1 {
            return;
        }
        for i in (from + 1..len).rev() {
            let j = from + self.random_below(i - from + 1);
            self.entries.swap(i, j);
        }
    }
}

#[derive(Default)]
struct Output {
    effects: Vec<Effect>,
    added: Vec<Added>,
}

impl Output {
    fn notice(&mut self, text: &str) {
        self.effects.push(Effect::Notice {
            text: text.to_owned(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Play(usize),
    Restart,
    NeedMore,
}

/// Where Next, Previous or a natural end goes.
///
/// Repeat-one governs natural ends only; pressing Next still moves, and since the listener chose
/// to loop, a move past either end wraps as it does under repeat-all. Previous on the first song
/// otherwise starts it again, as every player does. Next on the last song asks for more instead
/// of starting over.
fn decide_move(
    count: usize,
    index: usize,
    forward: bool,
    natural: bool,
    repeat: RepeatMode,
) -> Move {
    if natural && repeat == RepeatMode::One {
        return Move::Restart;
    }
    if forward && index + 1 < count {
        return Move::Play(index + 1);
    }
    if !forward && index > 0 {
        return Move::Play(index - 1);
    }
    if repeat != RepeatMode::Off {
        return Move::Play(if forward { 0 } else { count - 1 });
    }
    if forward {
        Move::NeedMore
    } else {
        Move::Restart
    }
}

#[cfg(test)]
mod tests;
