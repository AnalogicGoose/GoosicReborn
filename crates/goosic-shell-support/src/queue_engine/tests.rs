use super::*;

fn track(id: &str) -> QueueTrack {
    QueueTrack::new(id, format!("Song {id}"))
}

fn tracks(ids: &[&str]) -> Vec<QueueTrack> {
    ids.iter().map(|id| track(id)).collect()
}

fn engine() -> QueueEngine {
    QueueEngine::with_seed(42)
}

fn ordered(engine: &mut QueueEngine, ids: &[&str], start: Option<usize>) -> Outcome {
    engine.handle(Command::Launch {
        tracks: tracks(ids),
        start,
        kind: LaunchKind::Ordered,
        source_title: Some("Album".into()),
        source_key: Some("album:1".into()),
        shuffle: None,
    })
}

fn video_ids(engine: &QueueEngine) -> Vec<String> {
    engine
        .snapshot()
        .entries
        .iter()
        .map(|entry| engine.track(*entry).unwrap().video_id.clone())
        .collect()
}

fn playing(engine: &QueueEngine) -> Option<String> {
    let snapshot = engine.snapshot();
    snapshot.current.map(|index| {
        engine
            .track(snapshot.entries[index])
            .unwrap()
            .video_id
            .clone()
    })
}

fn played_video(engine: &QueueEngine, outcome: &Outcome) -> Option<String> {
    outcome.effects.iter().find_map(|effect| match effect {
        Effect::Play { entry } => Some(engine.track(*entry).unwrap().video_id.clone()),
        _ => None,
    })
}

fn radio_request(outcome: &Outcome) -> Option<(u64, String, Option<String>, Option<String>)> {
    outcome.effects.iter().find_map(|effect| match effect {
        Effect::FetchRadio {
            request,
            seed_video_id,
            continuation,
            account_id,
        } => Some((
            *request,
            seed_video_id.clone(),
            continuation.clone(),
            account_id.clone(),
        )),
        _ => None,
    })
}

fn notices(outcome: &Outcome) -> Vec<String> {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notice { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn surfaces_decide_between_an_ordered_list_and_a_station() {
    for surface in [
        LaunchSurface::Album,
        LaunchSurface::Playlist,
        LaunchSurface::LikedMusic,
        LaunchSurface::Artist,
        LaunchSurface::Queue,
    ] {
        assert_eq!(launch_kind_for(surface), LaunchKind::Ordered, "{surface:?}");
    }
    assert_eq!(launch_kind_for(LaunchSurface::Search), LaunchKind::Station);
    assert_eq!(launch_kind_for(LaunchSurface::Shelf), LaunchKind::Station);
}

#[test]
fn an_ordered_launch_queues_the_list_and_plays_the_chosen_row() {
    let mut engine = engine();
    let outcome = ordered(&mut engine, &["a", "b", "c"], Some(1));
    assert_eq!(video_ids(&engine), ["a", "b", "c"]);
    assert_eq!(played_video(&engine, &outcome).as_deref(), Some("b"));
    assert_eq!(outcome.added.len(), 3);
    assert_eq!(outcome.snapshot.up_next_text, "From Album · 1 song");
    assert_eq!(outcome.snapshot.source_key.as_deref(), Some("album:1"));
    assert!(
        radio_request(&outcome).is_none(),
        "an album does not start a radio"
    );
}

#[test]
fn a_search_result_starts_a_station_from_that_song_alone() {
    let mut engine = engine();
    let outcome = engine.handle(Command::Launch {
        tracks: tracks(&["dumbai", "dubai", "other"]),
        start: Some(0),
        kind: launch_kind_for(LaunchSurface::Search),
        source_title: Some("Songs".into()),
        source_key: Some("search:dumbai".into()),
        shuffle: None,
    });
    assert_eq!(video_ids(&engine), ["dumbai"]);
    assert_eq!(
        outcome.added,
        [Added {
            input: 0,
            entry: outcome.snapshot.entries[0]
        }]
    );
    assert_eq!(
        outcome.snapshot.source_title.as_deref(),
        Some("Song dumbai radio")
    );
    assert_eq!(outcome.snapshot.source_key, None);
    let (_, seed, continuation, _) =
        radio_request(&outcome).expect("recommendations are asked for");
    assert_eq!(seed, "dumbai");
    assert_eq!(continuation, None);
    assert_eq!(
        outcome.snapshot.up_next_text,
        "From Song dumbai radio · Finding songs like this…"
    );
}

#[test]
fn recommendations_extend_the_station_without_repeats() {
    let mut engine = engine();
    let outcome = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&outcome).unwrap();
    let loaded = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["seed", "x", "y", "x", "z", ""]),
        next_cursor: Some("next".into()),
    });
    assert_eq!(video_ids(&engine), ["seed", "x", "y", "z"]);
    assert_eq!(
        loaded
            .added
            .iter()
            .map(|added| added.input)
            .collect::<Vec<_>>(),
        [1, 2, 4]
    );
    assert_eq!(loaded.snapshot.up_next_count, 3);
    assert!(!loaded.snapshot.radio_loading);
}

#[test]
fn a_late_page_for_a_replaced_queue_is_dropped() {
    let mut engine = engine();
    let outcome = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&outcome).unwrap();
    ordered(&mut engine, &["a", "b"], None);
    let late = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["x"]),
        next_cursor: None,
    });
    assert!(late.added.is_empty());
    assert_eq!(video_ids(&engine), ["a", "b"]);
}

#[test]
fn the_radio_keeps_a_few_songs_ahead() {
    let mut engine = engine();
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["a", "b", "c", "d"]),
        next_cursor: Some("p2".into()),
    });
    // Four ahead: nothing to fetch yet.
    assert!(radio_request(&engine.handle(Command::Next)).is_none());
    // Three ahead after the first Next, still enough; two ahead asks for the next page.
    let outcome = engine.handle(Command::Next);
    let (_, seed, continuation, _) = radio_request(&outcome).expect("topped up");
    assert_eq!(seed, "seed");
    assert_eq!(continuation.as_deref(), Some("p2"));
    // A second move while the page is on its way does not ask twice.
    assert!(radio_request(&engine.handle(Command::Next)).is_none());
}

#[test]
fn a_repeated_cursor_ends_the_station() {
    let mut engine = engine();
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    // A short page asks again straight away, with the cursor it carried.
    let loaded = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["a"]),
        next_cursor: Some("same".into()),
    });
    let (request, _, continuation, _) = radio_request(&loaded).expect("topped up");
    assert_eq!(continuation.as_deref(), Some("same"));
    let repeated = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["b"]),
        next_cursor: Some("same".into()),
    });
    assert!(
        radio_request(&repeated).is_none(),
        "an exhausted station is not asked again"
    );
    assert_eq!(video_ids(&engine), ["seed", "a", "b"]);
    engine.handle(Command::Next);
    let outcome = engine.handle(Command::Next);
    assert!(
        radio_request(&outcome).is_none(),
        "an exhausted station is not re-seeded"
    );
    let end = engine.handle(Command::Next);
    assert_eq!(notices(&end), ["The queue has finished."]);
}

#[test]
fn the_end_of_an_ordered_list_continues_with_radio_when_autoplay_is_on() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b"], Some(1));
    let ended = engine.handle(Command::Ended);
    let (request, seed, ..) = radio_request(&ended).expect("autoplay starts a radio");
    assert_eq!(seed, "b");
    assert!(
        played_video(&engine, &ended).is_none(),
        "it waits for the recommendations"
    );
    let loaded = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["a", "r1", "r2"]),
        next_cursor: None,
    });
    assert_eq!(played_video(&engine, &loaded).as_deref(), Some("r1"));
    assert_eq!(video_ids(&engine), ["a", "b", "r1", "r2"]);
}

#[test]
fn something_chosen_while_recommendations_load_stands() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b"], Some(1));
    let (request, ..) = radio_request(&engine.handle(Command::Ended)).unwrap();
    let jump = engine.handle(Command::Jump {
        entry: engine.snapshot().entries[0],
    });
    assert_eq!(played_video(&engine, &jump).as_deref(), Some("a"));
    let loaded = engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["r1"]),
        next_cursor: None,
    });
    assert!(played_video(&engine, &loaded).is_none());
    assert_eq!(playing(&engine).as_deref(), Some("a"));
}

#[test]
fn autoplay_off_plays_the_list_through_and_then_stops() {
    let mut engine = engine();
    engine.handle(Command::SetAutoplay { enabled: false });
    ordered(&mut engine, &["a", "b"], None);
    let ended = engine.handle(Command::Ended);
    assert_eq!(
        played_video(&engine, &ended).as_deref(),
        Some("b"),
        "autoplay governs radio, not the list the listener chose"
    );
    let ended = engine.handle(Command::Ended);
    assert!(radio_request(&ended).is_none());
    assert_eq!(notices(&ended), ["Track finished. Autoplay is off."]);
}

#[test]
fn next_on_the_last_song_continues_a_running_radio_even_with_autoplay_off() {
    let mut engine = engine();
    engine.handle(Command::SetAutoplay { enabled: false });
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: vec![],
        next_cursor: Some("p2".into()),
    });
    let next = engine.handle(Command::Next);
    assert!(
        radio_request(&next).is_some(),
        "a skip into a running radio continues it"
    );
}

#[test]
fn next_on_the_last_song_does_not_wrap_to_the_top() {
    let mut engine = engine();
    engine.handle(Command::SetAutoplay { enabled: false });
    ordered(&mut engine, &["a", "b"], Some(1));
    let next = engine.handle(Command::Next);
    assert!(played_video(&engine, &next).is_none());
    assert_eq!(notices(&next), ["The queue has finished."]);
}

#[test]
fn repeat_all_wraps_both_ways_and_repeat_one_only_governs_natural_ends() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c"], Some(2));
    engine.handle(Command::CycleRepeat);
    assert_eq!(engine.snapshot().repeat, RepeatMode::All);
    let next = engine.handle(Command::Next);
    assert_eq!(played_video(&engine, &next).as_deref(), Some("a"));
    let previous = engine.handle(Command::Previous { position: 0.0 });
    assert_eq!(played_video(&engine, &previous).as_deref(), Some("c"));
    engine.handle(Command::CycleRepeat);
    assert_eq!(engine.snapshot().repeat, RepeatMode::One);
    let ended = engine.handle(Command::Ended);
    assert_eq!(
        played_video(&engine, &ended).as_deref(),
        Some("c"),
        "reloaded"
    );
    let next = engine.handle(Command::Next);
    assert_eq!(
        played_video(&engine, &next).as_deref(),
        Some("a"),
        "Next still moves"
    );
}

#[test]
fn previous_restarts_a_song_past_its_first_seconds_and_the_first_song() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b"], Some(1));
    let late = engine.handle(Command::Previous { position: 12.0 });
    assert_eq!(late.effects, [Effect::SeekToStart]);
    let early = engine.handle(Command::Previous { position: 1.0 });
    assert_eq!(played_video(&engine, &early).as_deref(), Some("a"));
    let first = engine.handle(Command::Previous { position: 0.0 });
    assert_eq!(
        first.effects,
        [Effect::SeekToStart],
        "never wraps without repeat-all"
    );
}

#[test]
fn one_end_is_acted_on_once() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c"], None);
    engine.handle(Command::Ended);
    let repeated = engine.handle(Command::Ended);
    assert_eq!(
        played_video(&engine, &repeated).as_deref(),
        Some("c"),
        "a new play arms a new end"
    );
    // But the same play's second report is ignored.
    let mut engine = super::QueueEngine::with_seed(1);
    ordered(&mut engine, &["a", "b", "c"], None);
    engine.handle(Command::Ended);
    let snapshot_before = engine.snapshot();
    engine.end_handled = Some(engine.play_sequence);
    let ignored = engine.handle(Command::Ended);
    assert!(ignored.effects.is_empty());
    assert_eq!(engine.snapshot().current, snapshot_before.current);
}

#[test]
fn the_sleep_timer_stops_at_the_end_of_the_song() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b"], None);
    engine.handle(Command::SetSleepAtEnd { enabled: true });
    let ended = engine.handle(Command::Ended);
    assert!(ended.effects.contains(&Effect::StopForSleep));
    assert!(played_video(&engine, &ended).is_none());
    assert!(!ended.snapshot.sleep_at_end, "it fires once");
}

#[test]
fn refused_songs_are_left_out_of_lists_unless_chosen_and_out_of_radio() {
    let mut engine = engine();
    engine.handle(Command::Refused {
        video_id: "bad".into(),
    });
    ordered(&mut engine, &["a", "bad", "c"], Some(0));
    assert_eq!(video_ids(&engine), ["a", "c"]);
    ordered(&mut engine, &["a", "bad", "c"], Some(1));
    assert_eq!(
        video_ids(&engine),
        ["a", "bad", "c"],
        "the listener asked for it"
    );

    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["bad", "ok"]),
        next_cursor: None,
    });
    assert_eq!(video_ids(&engine), ["seed", "ok"]);
}

#[test]
fn recently_played_songs_are_kept_out_of_recommendations() {
    let mut engine = engine();
    ordered(&mut engine, &["old"], None);
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["old", "new"]),
        next_cursor: None,
    });
    assert_eq!(video_ids(&engine), ["seed", "new"]);
}

#[test]
fn a_launch_with_nothing_playable_says_so() {
    let mut engine = engine();
    let outcome = ordered(&mut engine, &["", ""], None);
    assert_eq!(notices(&outcome), ["Nothing here can be played."]);
    assert!(outcome.snapshot.entries.is_empty());
}

#[test]
fn shuffle_reorders_what_is_left_and_restores_it() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c", "d", "e", "f"], Some(1));
    let toggled = engine.handle(Command::ToggleShuffle);
    assert_eq!(
        toggled.effects,
        [Effect::SavePreferences {
            shuffle: true,
            repeat: RepeatMode::Off
        }]
    );
    let shuffled = video_ids(&engine);
    assert_eq!(
        &shuffled[..2],
        ["a", "b"],
        "history and the playing song stay put"
    );
    let mut rest = shuffled[2..].to_vec();
    rest.sort();
    assert_eq!(rest, ["c", "d", "e", "f"]);
    assert_eq!(playing(&engine).as_deref(), Some("b"));

    engine.handle(Command::ToggleShuffle);
    assert_eq!(video_ids(&engine), ["a", "b", "c", "d", "e", "f"]);
    assert_eq!(playing(&engine).as_deref(), Some("b"));
}

#[test]
fn a_shuffled_launch_leads_with_the_chosen_song() {
    let mut engine = engine();
    engine.handle(Command::Configure {
        shuffle: true,
        repeat: RepeatMode::Off,
        autoplay: true,
    });
    let outcome = ordered(&mut engine, &["a", "b", "c", "d"], Some(2));
    assert_eq!(played_video(&engine, &outcome).as_deref(), Some("c"));
    assert_eq!(
        outcome.snapshot.current,
        Some(0),
        "nothing plays before the chosen song"
    );
    assert_eq!(video_ids(&engine).len(), 4);
}

#[test]
fn a_shuffle_play_button_turns_shuffle_on_and_starts_anywhere() {
    let mut engine = engine();
    let outcome = engine.handle(Command::Launch {
        tracks: tracks(&["a", "b", "c"]),
        start: None,
        kind: LaunchKind::Ordered,
        source_title: None,
        source_key: None,
        shuffle: Some(true),
    });
    assert!(outcome.snapshot.shuffle);
    assert_eq!(outcome.snapshot.current, Some(0));
    assert!(played_video(&engine, &outcome).is_some());
    assert!(
        outcome.effects.contains(&Effect::SavePreferences {
            shuffle: true,
            repeat: RepeatMode::Off
        }),
        "the button changed the preference, so it is stored"
    );
}

#[test]
fn a_play_button_plays_in_order_and_a_row_keeps_the_preference() {
    let mut engine = engine();
    engine.handle(Command::Configure {
        shuffle: true,
        repeat: RepeatMode::Off,
        autoplay: true,
    });
    let launch = |shuffle| Command::Launch {
        tracks: tracks(&["a", "b", "c", "d"]),
        start: None,
        kind: LaunchKind::Ordered,
        source_title: None,
        source_key: None,
        shuffle,
    };
    let played = engine.handle(launch(Some(false)));
    assert!(!played.snapshot.shuffle);
    assert_eq!(video_ids(&engine), ["a", "b", "c", "d"]);
    assert_eq!(playing(&engine).as_deref(), Some("a"));
    assert!(played
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::SavePreferences { shuffle: false, .. })));

    engine.handle(Command::ToggleShuffle);
    let row = engine.handle(launch(None));
    assert!(row.snapshot.shuffle, "a row leaves the preference alone");
    assert!(!row
        .effects
        .iter()
        .any(|effect| matches!(effect, Effect::SavePreferences { .. })));
}

#[test]
fn play_next_and_add_to_queue_place_the_song_and_keep_the_unshuffled_order() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c"], Some(0));
    let next = engine.handle(Command::PlayNext { track: track("n") });
    assert_eq!(notices(&next), ["\u{201C}Song n\u{201D} plays next."]);
    engine.handle(Command::AddToQueue { track: track("z") });
    assert_eq!(video_ids(&engine), ["a", "n", "b", "c", "z"]);

    // With nothing playing, the song simply plays.
    let mut empty = super::QueueEngine::with_seed(3);
    let outcome = empty.handle(Command::AddToQueue {
        track: track("solo"),
    });
    assert_eq!(played_video(&empty, &outcome).as_deref(), Some("solo"));
}

#[test]
fn the_playing_song_cannot_be_removed() {
    let mut engine = engine();
    let outcome = ordered(&mut engine, &["a", "b", "c"], Some(1));
    let entries = outcome.snapshot.entries;
    let refused = engine.handle(Command::Remove { entry: entries[1] });
    assert_eq!(
        notices(&refused),
        ["The track that is playing stays in the queue."]
    );
    let removed = engine.handle(Command::Remove { entry: entries[0] });
    assert_eq!(removed.snapshot.current, Some(0));
    assert_eq!(video_ids(&engine), ["b", "c"]);
}

#[test]
fn reordering_keeps_the_playing_entry_playing() {
    let mut engine = engine();
    let outcome = ordered(&mut engine, &["a", "b", "c"], Some(1));
    let e = outcome.snapshot.entries;
    let moved = engine.handle(Command::Reorder {
        entries: vec![e[2], e[0], e[1]],
    });
    assert_eq!(moved.snapshot.current, Some(2));
    assert_eq!(playing(&engine).as_deref(), Some("b"));
    // Not a permutation of the queue: ignored.
    let bogus = engine.handle(Command::Reorder {
        entries: vec![e[0], e[0], e[1]],
    });
    assert_eq!(bogus.snapshot.revision, moved.snapshot.revision);
}

#[test]
fn clearing_keeps_history_and_can_be_undone_once_in_time() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c", "d"], Some(1));
    let cleared = engine.handle(Command::ClearUpcoming { now: 100.0 });
    assert_eq!(video_ids(&engine), ["a", "b"]);
    assert_eq!(cleared.snapshot.undo_deadline, Some(110.0));
    assert!(!cleared.snapshot.can_clear);

    let late = engine.handle(Command::UndoClear { now: 111.0 });
    assert!(late.effects.is_empty());
    let undone = engine.handle(Command::UndoClear { now: 105.0 });
    assert_eq!(notices(&undone), ["Restored Up Next"]);
    assert_eq!(video_ids(&engine), ["a", "b", "c", "d"]);
    assert_eq!(undone.snapshot.undo_deadline, None);
}

#[test]
fn any_other_change_ends_the_undo() {
    let mut engine = engine();
    ordered(&mut engine, &["a", "b", "c"], Some(0));
    engine.handle(Command::ClearUpcoming { now: 0.0 });
    let added = engine.handle(Command::AddToQueue { track: track("x") });
    assert_eq!(added.snapshot.undo_deadline, None);
    engine.handle(Command::UndoClear { now: 1.0 });
    assert_eq!(video_ids(&engine), ["a", "x"]);
}

#[test]
fn clearing_drops_a_pending_radio_page() {
    let mut engine = engine();
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: tracks(&["a"]),
        next_cursor: Some("p2".into()),
    });
    // One ahead, so the radio has asked for more.
    let pending = engine.snapshot().radio_loading;
    assert!(pending);
    let cleared = engine.handle(Command::ClearUpcoming { now: 0.0 });
    assert!(!cleared.snapshot.radio_loading);
    assert!(!cleared.snapshot.radio);
}

#[test]
fn a_personal_radio_falls_back_to_the_catalog_once() {
    let mut engine = engine();
    engine.handle(Command::SetRadioAccount {
        account_id: Some("acct".into()),
    });
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, _, _, account) = radio_request(&first).unwrap();
    assert_eq!(account.as_deref(), Some("acct"));
    let failed = engine.handle(Command::RadioFailed { request });
    let (request, seed, _, account) = radio_request(&failed).expect("retried anonymously");
    assert_eq!(seed, "seed");
    assert_eq!(account, None);
    let failed = engine.handle(Command::RadioFailed { request });
    assert_eq!(notices(&failed), ["Could not load recommendations."]);
}

#[test]
fn an_account_change_replaces_a_personal_station_at_the_end() {
    let mut engine = engine();
    engine.handle(Command::SetRadioAccount {
        account_id: Some("one".into()),
    });
    let first = engine.handle(Command::StartRadio {
        track: track("seed"),
    });
    let (request, ..) = radio_request(&first).unwrap();
    engine.handle(Command::RadioLoaded {
        request,
        tracks: vec![],
        next_cursor: Some("p".into()),
    });
    engine.handle(Command::SetRadioAccount {
        account_id: Some("two".into()),
    });
    let next = engine.handle(Command::Next);
    let (_, seed, continuation, account) = radio_request(&next).unwrap();
    assert_eq!(seed, "seed");
    assert_eq!(continuation, None, "another account's cursor is never sent");
    assert_eq!(account.as_deref(), Some("two"));
}

#[test]
fn reset_forgets_the_queue_but_not_the_preferences() {
    let mut engine = engine();
    engine.handle(Command::CycleRepeat);
    ordered(&mut engine, &["a"], None);
    let reset = engine.handle(Command::Reset);
    assert!(reset.snapshot.entries.is_empty());
    assert_eq!(reset.snapshot.current, None);
    assert_eq!(reset.snapshot.repeat, RepeatMode::All);
    assert_eq!(reset.snapshot.up_next_text, "Nothing after this");
}

#[test]
fn commands_and_outcomes_speak_camel_case_json() {
    let command: Command = serde_json::from_str(
        r#"{"type":"launch","tracks":[{"videoId":"a","title":"A"}],"start":0,"kind":"station","sourceTitle":"Songs"}"#,
    )
    .unwrap();
    assert!(matches!(
        command,
        Command::Launch {
            kind: LaunchKind::Station,
            ..
        }
    ));
    let command: Command =
        serde_json::from_str(r#"{"type":"radioLoaded","request":3,"tracks":[],"nextCursor":"c"}"#)
            .unwrap();
    assert!(matches!(command, Command::RadioLoaded { request: 3, .. }));
    let mut engine = engine();
    let outcome = engine.handle(command);
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["snapshot"]["repeat"], "off");
    assert_eq!(json["snapshot"]["upNextText"], "Nothing after this");
    let effect = serde_json::to_value(Effect::FetchRadio {
        request: 1,
        seed_video_id: "s".into(),
        continuation: None,
        account_id: None,
    })
    .unwrap();
    assert_eq!(effect["type"], "fetchRadio");
    assert_eq!(effect["seedVideoId"], "s");
}
