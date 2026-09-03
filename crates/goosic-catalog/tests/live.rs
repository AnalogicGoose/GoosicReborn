//! Live upstream checks.
//!
//! These are `#[ignore]`d because they require network access and depend on a third-party
//! service that changes without notice. Run them deliberately:
//!
//! ```sh
//! cargo test -p goosic-catalog -- --ignored --nocapture
//! ```

use goosic_catalog::Catalog;
use goosic_protocol::CatalogItemKind;

#[test]
#[ignore = "requires network access to music.youtube.com"]
fn search_returns_playable_songs() {
    let catalog = Catalog::new();
    let page = catalog.search("daft punk", "songs").expect("search succeeds");
    println!("shelves: {}", page.shelves.len());
    let songs: Vec<_> = page
        .shelves
        .iter()
        .flat_map(|shelf| shelf.items.iter())
        .filter(|item| matches!(item.kind, CatalogItemKind::Song | CatalogItemKind::Video))
        .collect();
    for item in songs.iter().take(5) {
        println!(
            "  {:?} {} — {} [{}] {}",
            item.kind,
            item.title,
            item.artist.as_deref().unwrap_or("?"),
            item.duration.as_deref().unwrap_or("?"),
            item.video_id.as_deref().unwrap_or("?")
        );
    }
    assert!(!songs.is_empty(), "expected at least one playable song");
    for song in &songs {
        let video_id = song.video_id.as_deref().expect("song carries a video id");
        assert_eq!(video_id.len(), 11, "unexpected video id shape: {video_id}");
        assert!(!song.title.is_empty());
    }
}

#[test]
#[ignore = "requires network access to music.youtube.com"]
fn mixed_search_groups_multiple_kinds() {
    let catalog = Catalog::new();
    let page = catalog.search("radiohead", "all").expect("search succeeds");
    for shelf in &page.shelves {
        println!("shelf {} ({} items)", shelf.title, shelf.items.len());
        for item in shelf.items.iter().take(3) {
            println!("    {:?} {} :: {}", item.kind, item.title, item.id);
        }
    }
    assert!(!page.shelves.is_empty());
}

#[test]
#[ignore = "requires network access to music.youtube.com"]
fn home_browse_returns_carousels() {
    let catalog = Catalog::new();
    let page = catalog.browse_route("home", "Home").expect("home browses");
    println!("home shelves: {}", page.shelves.len());
    for shelf in page.shelves.iter().take(5) {
        println!("  {} ({} items)", shelf.title, shelf.items.len());
        for item in shelf.items.iter().take(2) {
            println!("      {:?} {} :: {}", item.kind, item.title, item.id);
        }
    }
    assert!(!page.shelves.is_empty());
}

#[test]
#[ignore = "requires network access to music.youtube.com"]
fn album_from_search_loads_its_tracks() {
    let catalog = Catalog::new();
    let results = catalog
        .search("random access memories", "albums")
        .expect("album search succeeds");
    let album = results
        .shelves
        .iter()
        .flat_map(|shelf| shelf.items.iter())
        .find(|item| item.kind == CatalogItemKind::Album)
        .expect("an album result");
    println!("album {} :: {}", album.title, album.id);
    let page = catalog.album(&album.id).expect("album page loads");
    println!("  {} — {} ({} tracks)", page.title, page.subtitle, page.tracks.len());
    for track in page.tracks.iter().take(5) {
        println!(
            "    {} [{}] {}",
            track.title,
            track.duration.as_deref().unwrap_or("?"),
            track.video_id.as_deref().unwrap_or("?")
        );
    }
    assert!(!page.tracks.is_empty());
}
