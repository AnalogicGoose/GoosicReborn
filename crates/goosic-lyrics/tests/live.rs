//! Live LRCLIB checks.
//!
//! `#[ignore]`d: they need network access and depend on a third-party database.
//!
//! ```sh
//! cargo test -p goosic-lyrics -- --ignored --nocapture
//! ```

use goosic_lyrics::LyricsClient;
use goosic_protocol::LyricsQuery;

#[test]
#[ignore = "requires network access to lrclib.net"]
fn a_well_known_song_returns_synced_lyrics() {
    let client = LyricsClient::new();
    let document = client
        .lookup(&LyricsQuery {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            album: String::new(),
            duration_seconds: None,
        })
        .expect("lyrics are found");

    println!(
        "source={} synced={} lines={} truncated={}",
        document.source,
        document.synced,
        document.lines.len(),
        document.truncated
    );
    for line in document.lines.iter().take(5) {
        println!("  {:>7}ms  {}", line.at_ms, line.text);
    }

    assert!(!document.lines.is_empty());
    if document.synced {
        assert!(
            document.lines.windows(2).all(|pair| pair[0].at_ms <= pair[1].at_ms),
            "synced lines must be ordered"
        );
        assert!(
            document.lines.iter().any(|line| line.at_ms > 0),
            "synced lines must carry real timings"
        );
    }
}

#[test]
#[ignore = "requires network access to lrclib.net"]
fn an_invented_track_reports_not_found_rather_than_guessing() {
    let client = LyricsClient::new();
    let error = client
        .lookup(&LyricsQuery {
            title: "Zzzq Nonexistent Track 47190".into(),
            artist: "Nobody At All 47190".into(),
            album: String::new(),
            duration_seconds: None,
        })
        .map(|_| ())
        .unwrap_err();
    println!("error: {error} ({})", error.code());
    assert_eq!(error.code(), "lyricsNotFound");
}

#[test]
#[ignore = "requires network access to lrclib.net"]
fn a_repeat_lookup_is_served_from_the_cache() {
    let client = LyricsClient::new();
    let query = LyricsQuery {
        title: "Creep".into(),
        artist: "Radiohead".into(),
        album: String::new(),
        duration_seconds: None,
    };
    let first = std::time::Instant::now();
    client.lookup(&query).expect("lyrics are found");
    let cold = first.elapsed();

    let second = std::time::Instant::now();
    client.lookup(&query).expect("lyrics are found");
    let warm = second.elapsed();

    println!("cold={cold:?} warm={warm:?}");
    assert!(warm < cold, "a repeat lookup must not hit the network again");
}
