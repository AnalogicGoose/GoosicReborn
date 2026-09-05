//! Decodes this machine's real downloaded files, if a previous Goosic install left any.
//!
//! `#[ignore]`d because it depends on the machine. It never modifies the legacy files.
//!
//! ```sh
//! cargo test -p goosic-downloads -- --ignored --nocapture
//! ```

use goosic_downloads::{decode, DownloadLibrary};

#[test]
#[ignore = "depends on a previous Goosic install having downloaded media"]
fn real_downloads_import_and_decode() {
    let Some(media) = DownloadLibrary::default_legacy_directory() else {
        println!("no legacy download directory on this machine");
        return;
    };
    println!("legacy downloads: {}", media.display());

    let directory = tempfile::tempdir().unwrap();
    let mut library = DownloadLibrary::open(
        directory.path().join("downloads.json"),
        directory.path().join("decoded"),
    )
    .unwrap();
    let added = match library.import_legacy(&media) {
        Ok(added) => added,
        Err(goosic_downloads::DownloadError::NoLegacyDownloads) => {
            println!("the legacy download directory exists but holds no media");
            return;
        }
        Err(error) => panic!("import failed: {error}"),
    };
    let tracks = library.tracks();
    println!("imported {added} tracks; index holds {}", tracks.len());
    for track in tracks.iter().take(3) {
        println!(
            "  {} — {} ({} bytes)",
            track.title, track.artist, track.bytes
        );
    }
    assert!(!tracks.is_empty());
    assert!(tracks.iter().all(|track| track.available));

    let first = tracks.first().unwrap();
    let started = std::time::Instant::now();
    let wav = library.prepare(&first.video_id).expect("the track decodes");
    let elapsed = started.elapsed();
    let bytes = std::fs::metadata(&wav).unwrap().len();
    let audio = decode::decode_opus_webm(&media.join(format!("{}.webm", first.video_id))).unwrap();
    println!(
        "decoded {} in {:.2}s -> {} ({} bytes, {:.1}s, {} ch)",
        first.title,
        elapsed.as_secs_f64(),
        wav.display(),
        bytes,
        audio.duration_seconds(),
        audio.channels
    );

    assert!(bytes > 44, "the wav has audio, not just a header");
    assert!(
        audio.duration_seconds() > 5.0,
        "a real track is longer than five seconds"
    );
    assert!(
        audio.samples.iter().any(|sample| sample.abs() > 0.01),
        "decoded audio must not be silence"
    );

    // A second prepare must reuse the cached decode rather than doing the work again.
    let reused = std::time::Instant::now();
    library.prepare(&first.video_id).unwrap();
    println!("second prepare took {:.3}s", reused.elapsed().as_secs_f64());
    assert!(reused.elapsed() < elapsed.max(std::time::Duration::from_millis(50)));
}
