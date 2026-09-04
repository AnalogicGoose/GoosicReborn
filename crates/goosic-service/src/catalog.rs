//! Catalog command handling and the NDJSON frame budget.
//!
//! A catalog page must survive one line of the service protocol. Upstream track lists can be
//! arbitrarily long, so pages are clamped here — visibly, via `truncated` — instead of being
//! silently dropped by a client-side frame limit.

use goosic_catalog::{Catalog, CatalogError};
use goosic_protocol::{
    CatalogPage, ErrorObject, RequestPayload, ResponseEnvelope, ResponsePayload,
};

/// Response budget for one catalog page, in serialized bytes. Kept below the shell's own
/// response frame limit so a clamped page always fits with room for the envelope.
pub const MAX_CATALOG_BYTES: usize = 192 * 1024;

/// Structural caps applied before the byte budget, so a huge page degrades predictably rather
/// than by whatever happens to serialize last.
const MAX_SHELVES: usize = 12;
const MAX_ITEMS_PER_SHELF: usize = 60;
const MAX_TRACKS: usize = 500;

/// Total rows a page carries across its shelves and track list.
fn row_count(page: &CatalogPage) -> usize {
    page.tracks.len()
        + page
            .shelves
            .iter()
            .map(|shelf| shelf.items.len())
            .sum::<usize>()
}

/// Drops the last `count` rows, taking from the track list first and then from the tail shelves.
fn drop_rows(page: &mut CatalogPage, count: usize) {
    let mut remaining = count;
    while remaining > 0 {
        if !page.tracks.is_empty() {
            let take = remaining.min(page.tracks.len());
            page.tracks.truncate(page.tracks.len() - take);
            remaining -= take;
            continue;
        }
        let Some(shelf) = page.shelves.last_mut() else {
            return;
        };
        if shelf.items.is_empty() {
            page.shelves.pop();
            continue;
        }
        let take = remaining.min(shelf.items.len());
        shelf.items.truncate(shelf.items.len() - take);
        remaining -= take;
    }
    page.shelves.retain(|shelf| !shelf.items.is_empty());
}

/// Clamps a page to the structural caps and then to `budget` serialized bytes.
///
/// Sets `truncated` whenever anything was removed, so the shell can say so rather than
/// presenting a partial list as complete.
pub fn clamp(mut page: CatalogPage, budget: usize) -> CatalogPage {
    let before = row_count(&page);

    if page.shelves.len() > MAX_SHELVES {
        page.shelves.truncate(MAX_SHELVES);
    }
    for shelf in &mut page.shelves {
        if shelf.items.len() > MAX_ITEMS_PER_SHELF {
            shelf.items.truncate(MAX_ITEMS_PER_SHELF);
        }
    }
    if page.tracks.len() > MAX_TRACKS {
        page.tracks.truncate(MAX_TRACKS);
    }

    // Shrink geometrically: serializing after every single removal is quadratic on long lists.
    loop {
        let size = serde_json::to_vec(&page)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        if size <= budget {
            break;
        }
        let rows = row_count(&page);
        if rows == 0 {
            break;
        }
        drop_rows(&mut page, (rows / 10).max(1));
    }

    page.truncated = row_count(&page) < before;
    page
}

fn failure(request_id: String, code: &str, message: String) -> ResponseEnvelope {
    ResponseEnvelope::failure(
        request_id,
        ErrorObject {
            code: code.to_owned(),
            message,
        },
    )
}

fn respond(request_id: String, result: Result<CatalogPage, CatalogError>) -> ResponseEnvelope {
    match result {
        Ok(page) => ResponseEnvelope::success(
            request_id,
            ResponsePayload {
                catalog: Some(clamp(page, MAX_CATALOG_BYTES)),
                ..Default::default()
            },
        ),
        Err(error) => failure(request_id, error.code(), error.to_string()),
    }
}

fn catalog_id(payload: &RequestPayload, request_id: &str) -> Result<String, ResponseEnvelope> {
    match payload.catalog_id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => Ok(id.to_owned()),
        _ => Err(failure(
            request_id.to_owned(),
            "invalidRequest",
            "catalog command requires catalogId".into(),
        )),
    }
}

/// Handles every `catalog.*` command. Returns `None` when the command is not a catalog command.
pub fn handle(
    catalog: &Catalog,
    command: &str,
    request_id: &str,
    payload: &RequestPayload,
) -> Option<ResponseEnvelope> {
    let id = request_id.to_owned();
    Some(match command {
        "catalog.search" => {
            let query = payload.query.as_deref().unwrap_or_default();
            let filter = payload.filter.as_deref().unwrap_or("all");
            respond(id, catalog.search(query, filter))
        }
        "catalog.browse" => {
            let route = match catalog_id(payload, request_id) {
                Ok(route) => route,
                Err(response) => return Some(response),
            };
            let title = payload.query.clone().unwrap_or_else(|| route.clone());
            respond(id, catalog.browse_route(&route, &title))
        }
        "catalog.album" => match catalog_id(payload, request_id) {
            Ok(browse_id) => respond(id, catalog.album(&browse_id)),
            Err(response) => response,
        },
        "catalog.playlist" => match catalog_id(payload, request_id) {
            Ok(playlist_id) => respond(id, catalog.playlist(&playlist_id)),
            Err(response) => response,
        },
        "catalog.artist" => match catalog_id(payload, request_id) {
            Ok(browse_id) => respond(id, catalog.artist(&browse_id)),
            Err(response) => response,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::{CatalogItem, CatalogItemKind, CatalogShelf};

    fn track(index: usize) -> CatalogItem {
        CatalogItem {
            kind: CatalogItemKind::Song,
            id: format!("video-{index:07}"),
            title: format!("A reasonably long track title number {index}"),
            subtitle: "Song • Some Artist • Some Album • 3:42".into(),
            artist: Some("Some Artist".into()),
            artist_id: Some(format!("UC{index:020}")),
            album: Some("Some Album".into()),
            album_id: Some(format!("MPREb_{index:011}")),
            duration: Some("3:42".into()),
            thumbnail: Some(format!("https://example.test/{index}/maxresdefault.jpg")),
            video_id: Some(format!("video-{index:05}")),
            explicit: false,
        }
    }

    fn page_with_tracks(count: usize) -> CatalogPage {
        CatalogPage {
            id: "MPREbtest".into(),
            title: "A long playlist".into(),
            tracks: (0..count).map(track).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_small_page_is_returned_unchanged_and_not_marked_truncated() {
        let page = clamp(page_with_tracks(12), MAX_CATALOG_BYTES);
        assert_eq!(page.tracks.len(), 12);
        assert!(!page.truncated);
    }

    #[test]
    fn an_oversized_page_is_clamped_under_budget_and_marked_truncated() {
        let page = clamp(page_with_tracks(4_000), MAX_CATALOG_BYTES);
        let size = serde_json::to_vec(&page).unwrap().len();
        assert!(size <= MAX_CATALOG_BYTES, "clamped page is {size} bytes");
        assert!(page.truncated);
        assert!(!page.tracks.is_empty(), "clamping must not empty the page");
    }

    #[test]
    fn structural_caps_apply_to_shelves_before_the_byte_budget() {
        let page = CatalogPage {
            id: "search:x".into(),
            title: "x".into(),
            shelves: (0..20)
                .map(|shelf| CatalogShelf {
                    id: format!("shelf-{shelf}"),
                    title: format!("Shelf {shelf}"),
                    items: (0..90).map(track).collect(),
                })
                .collect(),
            ..Default::default()
        };
        let clamped = clamp(page, MAX_CATALOG_BYTES);
        assert!(clamped.shelves.len() <= MAX_SHELVES);
        assert!(clamped
            .shelves
            .iter()
            .all(|shelf| shelf.items.len() <= MAX_ITEMS_PER_SHELF));
        assert!(clamped.truncated);
    }

    #[test]
    fn clamping_drops_tracks_before_shelves() {
        let page = CatalogPage {
            id: "artist".into(),
            title: "Artist".into(),
            shelves: vec![CatalogShelf {
                id: "shelf-0".into(),
                title: "Albums".into(),
                items: (0..5).map(track).collect(),
            }],
            tracks: (0..3_000).map(track).collect(),
            ..Default::default()
        };
        let clamped = clamp(page, MAX_CATALOG_BYTES);
        assert_eq!(clamped.shelves.len(), 1, "shelves survive the track clamp");
        assert_eq!(clamped.shelves[0].items.len(), 5);
    }

    #[test]
    fn non_catalog_commands_are_not_claimed() {
        let catalog = Catalog::new();
        assert!(handle(&catalog, "playback.claim", "1", &RequestPayload::default()).is_none());
        assert!(handle(&catalog, "state.get", "1", &RequestPayload::default()).is_none());
    }

    #[test]
    fn catalog_commands_requiring_an_id_reject_a_missing_one_without_network_access() {
        let catalog = Catalog::new();
        for command in [
            "catalog.album",
            "catalog.playlist",
            "catalog.artist",
            "catalog.browse",
        ] {
            let response = handle(&catalog, command, "1", &RequestPayload::default())
                .unwrap_or_else(|| panic!("{command} is a catalog command"));
            assert!(!response.ok, "{command} accepted a missing catalogId");
            assert_eq!(response.error.unwrap().code, "invalidRequest");
        }
    }

    #[test]
    fn search_rejects_an_empty_query_without_network_access() {
        let catalog = Catalog::new();
        let response = handle(&catalog, "catalog.search", "1", &RequestPayload::default()).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "invalidRequest");
    }
}
