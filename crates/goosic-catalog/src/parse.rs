//! Pure InnerTube JSON -> catalog normalization.
//!
//! Nothing here performs I/O, so every rule below is unit-testable against small fixtures.

use goosic_protocol::{
    ArtistHeader, CatalogItem, CatalogItemKind, CatalogPage, CatalogShelf, ShelfLayout,
};
use serde_json::Value;

use crate::json;

const THUMBNAIL_BUDGET: u64 = 400;

/// Maps YTM's `pageType` to the destination kind the shell can navigate to.
fn page_type_kind(page_type: &str) -> Option<CatalogItemKind> {
    match page_type {
        "MUSIC_PAGE_TYPE_ALBUM" => Some(CatalogItemKind::Album),
        "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => Some(CatalogItemKind::Artist),
        "MUSIC_PAGE_TYPE_PLAYLIST" => Some(CatalogItemKind::Playlist),
        _ => None,
    }
}

/// Maps the leading subtitle token ("Song • …") to a kind.
///
/// The client context pins `hl=en`, so these tokens are stable English.
fn token_kind(token: &str) -> Option<CatalogItemKind> {
    match token {
        "Song" => Some(CatalogItemKind::Song),
        // A podcast episode is a video in YouTube Music's player; filed as a song, it read as
        // one in the Songs shelf of an unfiltered search.
        "Video" | "Episode" => Some(CatalogItemKind::Video),
        "Album" | "Single" | "EP" => Some(CatalogItemKind::Album),
        "Artist" => Some(CatalogItemKind::Artist),
        "Playlist" | "Community playlist" => Some(CatalogItemKind::Playlist),
        _ => None,
    }
}

/// A browse destination read from any navigation endpoint.
struct Destination {
    browse_id: String,
    kind: Option<CatalogItemKind>,
}

fn destination(endpoint: &Value) -> Option<Destination> {
    let browse = json::first(endpoint, "browseEndpoint")?;
    let browse_id = browse.get("browseId").and_then(Value::as_str)?.to_owned();
    let page_type = json::path(
        browse,
        &[
            "browseEndpointContextSupportedConfigs",
            "browseEndpointContextMusicConfig",
            "pageType",
        ],
    )
    .and_then(Value::as_str)
    .unwrap_or_default();
    Some(Destination {
        browse_id,
        kind: page_type_kind(page_type),
    })
}

fn watch_video_id(node: &Value) -> Option<String> {
    if let Some(id) = json::path(node, &["playlistItemData", "videoId"]).and_then(Value::as_str) {
        return Some(id.to_owned());
    }
    json::first(node, "watchEndpoint")
        .and_then(|endpoint| endpoint.get("videoId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn playlist_id(node: &Value) -> Option<String> {
    json::first(node, "watchPlaylistEndpoint")
        .or_else(|| json::first(node, "watchEndpoint"))
        .and_then(|endpoint| endpoint.get("playlistId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn is_explicit(node: &Value) -> bool {
    json::collect(node, "musicInlineBadgeRenderer")
        .iter()
        .any(|badge| {
            json::path(badge, &["icon", "iconType"])
                .and_then(Value::as_str)
                .is_some_and(|icon| icon == "MUSIC_EXPLICIT_BADGE")
        })
}

/// Named references (artist, album) discovered among a row's subtitle runs.
#[derive(Default)]
struct References {
    artist: Option<(String, String)>,
    album: Option<(String, String)>,
}

fn references(nodes: &[&Value]) -> References {
    let mut refs = References::default();
    for node in nodes {
        for run in json::runs(node) {
            let Some(text) = run.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some(endpoint) = run.get("navigationEndpoint") else {
                continue;
            };
            let Some(destination) = destination(endpoint) else {
                continue;
            };
            match destination.kind {
                Some(CatalogItemKind::Artist) if refs.artist.is_none() => {
                    refs.artist = Some((text.to_owned(), destination.browse_id));
                }
                Some(CatalogItemKind::Album) if refs.album.is_none() => {
                    refs.album = Some((text.to_owned(), destination.browse_id));
                }
                _ => {}
            }
        }
    }
    refs
}

fn duration_from(nodes: &[&Value]) -> Option<String> {
    for node in nodes {
        for run in json::runs(node) {
            if let Some(text) = run.get("text").and_then(Value::as_str) {
                if json::is_duration(text) {
                    return Some(text.to_owned());
                }
            }
        }
        let text = json::runs_text(node);
        if json::is_duration(&text) {
            return Some(text);
        }
    }
    None
}

/// Normalizes one `musicResponsiveListItemRenderer` (search rows, album and playlist tracks).
pub fn responsive_item(node: &Value) -> Option<CatalogItem> {
    let flex: Vec<&Value> = node
        .get("flexColumns")
        .and_then(Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .filter_map(|column| {
                    json::path(
                        column,
                        &["musicResponsiveListItemFlexColumnRenderer", "text"],
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let fixed: Vec<&Value> = node
        .get("fixedColumns")
        .and_then(Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .filter_map(|column| {
                    json::path(
                        column,
                        &["musicResponsiveListItemFixedColumnRenderer", "text"],
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let title = flex.first().map(|node| json::runs_text(node))?;
    if title.is_empty() {
        return None;
    }
    let subtitle_nodes: Vec<&Value> = flex.iter().skip(1).copied().collect();
    let subtitle = subtitle_nodes
        .iter()
        .map(|node| json::runs_text(node))
        .find(|text| !text.is_empty())
        .unwrap_or_default();

    let leading_token = subtitle
        .split('•')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();

    let video_id = watch_video_id(node);
    // The title run's own endpoint is the row's canonical destination; endpoints found deeper in
    // the subtitle belong to the artist or album, not to the row itself.
    let title_destination = flex
        .first()
        .and_then(|node| json::runs(node).first())
        .and_then(|run| run.get("navigationEndpoint"))
        .and_then(destination)
        .or_else(|| node.get("navigationEndpoint").and_then(destination));

    let kind = if video_id.is_some() {
        token_kind(&leading_token)
            .filter(|kind| matches!(kind, CatalogItemKind::Song | CatalogItemKind::Video))
            .unwrap_or(CatalogItemKind::Song)
    } else {
        title_destination
            .as_ref()
            .and_then(|destination| destination.kind)
            .or_else(|| token_kind(&leading_token))?
    };

    let id = match kind {
        CatalogItemKind::Song | CatalogItemKind::Video => video_id.clone()?,
        CatalogItemKind::Playlist => title_destination
            .as_ref()
            .map(|destination| destination.browse_id.clone())
            .or_else(|| playlist_id(node))?,
        _ => title_destination
            .as_ref()
            .map(|destination| destination.browse_id.clone())?,
    };

    let refs = references(&subtitle_nodes);
    let mut duration_nodes = fixed.clone();
    duration_nodes.extend(subtitle_nodes.iter().copied());

    Some(CatalogItem {
        kind,
        id,
        title,
        subtitle,
        artist: refs.artist.as_ref().map(|(name, _)| name.clone()),
        artist_id: refs.artist.as_ref().map(|(_, id)| id.clone()),
        album: refs.album.as_ref().map(|(name, _)| name.clone()),
        album_id: refs.album.as_ref().map(|(_, id)| id.clone()),
        duration: duration_from(&duration_nodes),
        thumbnail: json::thumbnail(node, THUMBNAIL_BUDGET),
        video_id,
        explicit: is_explicit(node),
        color: None,
    })
}

/// Normalizes one `musicTwoRowItemRenderer` (the cards inside home and explore carousels).
pub fn two_row_item(node: &Value) -> Option<CatalogItem> {
    let title_node = node.get("title")?;
    let title = json::runs_text(title_node);
    if title.is_empty() {
        return None;
    }
    let subtitle = node
        .get("subtitle")
        .map(json::runs_text)
        .unwrap_or_default();
    let endpoint = node.get("navigationEndpoint");
    let video_id = endpoint.and_then(watch_video_id);
    let destination = endpoint.and_then(destination);

    let kind = match (&video_id, destination.as_ref().and_then(|d| d.kind)) {
        (Some(_), _) => CatalogItemKind::Song,
        (None, Some(kind)) => kind,
        (None, None) => {
            // A card with only a playlist watch endpoint is still a playable playlist.
            if endpoint.and_then(playlist_id).is_some() {
                CatalogItemKind::Playlist
            } else {
                return None;
            }
        }
    };

    let id = match kind {
        CatalogItemKind::Song | CatalogItemKind::Video => video_id.clone()?,
        _ => destination
            .as_ref()
            .map(|destination| destination.browse_id.clone())
            .or_else(|| endpoint.and_then(playlist_id))?,
    };

    let refs = references(&[node.get("subtitle").unwrap_or(&Value::Null)]);
    Some(CatalogItem {
        kind,
        id,
        title,
        subtitle,
        artist: refs.artist.as_ref().map(|(name, _)| name.clone()),
        artist_id: refs.artist.as_ref().map(|(_, id)| id.clone()),
        album: None,
        album_id: None,
        duration: None,
        thumbnail: json::thumbnail(node, THUMBNAIL_BUDGET),
        video_id,
        explicit: is_explicit(node),
        color: None,
    })
}

/// Normalizes one `playlistPanelVideoRenderer`, the row shape the watch queue uses.
///
/// This is a different renderer from the list rows elsewhere: its byline is one text node
/// rather than separate columns, so the artist is read from the byline's navigation runs.
pub fn queue_item(node: &Value) -> Option<CatalogItem> {
    let title = node.get("title").map(json::runs_text)?;
    if title.is_empty() {
        return None;
    }
    let video_id = node
        .get("videoId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| watch_video_id(node))?;

    let byline = node
        .get("longBylineText")
        .or_else(|| node.get("shortBylineText"));
    let subtitle = byline.map(json::runs_text).unwrap_or_default();
    let refs = references(&[byline.unwrap_or(&Value::Null)]);
    let duration = node
        .get("lengthText")
        .map(json::runs_text)
        .filter(|text| json::is_duration(text));

    Some(CatalogItem {
        kind: CatalogItemKind::Song,
        id: video_id.clone(),
        title,
        subtitle,
        artist: refs.artist.as_ref().map(|(name, _)| name.clone()),
        artist_id: refs.artist.as_ref().map(|(_, id)| id.clone()),
        album: refs.album.as_ref().map(|(name, _)| name.clone()),
        album_id: refs.album.as_ref().map(|(_, id)| id.clone()),
        duration,
        thumbnail: json::thumbnail(node, THUMBNAIL_BUDGET),
        video_id: Some(video_id),
        explicit: is_explicit(node),
        color: None,
    })
}

/// Builds the ordered queue that follows a track.
///
/// The first row is the track that was asked about, so it is dropped: the caller already has it
/// and re-queueing it would replay what just finished.
pub fn radio_page(seed_video_id: &str, response: &Value) -> CatalogPage {
    let mut tracks: Vec<CatalogItem> = json::collect(response, "playlistPanelVideoRenderer")
        .into_iter()
        .filter_map(queue_item)
        .collect();
    if tracks
        .first()
        .is_some_and(|first| first.id == seed_video_id)
    {
        tracks.remove(0);
    }
    // Upstream can repeat a row across continuations; a queue with duplicates plays the same
    // track twice in a row.
    let mut seen = std::collections::HashSet::new();
    tracks.retain(|track| seen.insert(track.id.clone()));

    // This cursor belongs to the watch queue itself. A whole-response continuation search can
    // accidentally pick related content or lyrics, which is indistinguishable from a queue that
    // starts recommending random music after its first page.
    let next_cursor = json::first(response, "playlistPanelRenderer")
        .or_else(|| json::first(response, "playlistPanelContinuation"))
        .and_then(|panel| panel.get("continuations"))
        .and_then(Value::as_array)
        .and_then(|continuations| {
            continuations.iter().find_map(|continuation| {
                continuation
                    .get("nextRadioContinuationData")
                    .or_else(|| continuation.get("nextContinuationData"))
                    .and_then(|data| data.get("continuation"))
                    .and_then(Value::as_str)
                    .filter(|token| !token.is_empty())
                    .map(str::to_owned)
            })
        });

    CatalogPage {
        id: format!("radio:{seed_video_id}"),
        title: "Radio".to_owned(),
        subtitle: String::new(),
        shelves: Vec::new(),
        tracks,
        thumbnail: None,
        next_cursor,
        truncated: false,
        all_tracks_id: None,
        artist: None,
    }
}

/// Reads the "Top result" card that leads an unfiltered search.
///
/// YouTube Music puts its best guess -- usually the artist the query names -- in a
/// `musicCardShelfRenderer` rather than among the rows, followed by a few of that result's
/// songs. Those songs omit the artist because the card above them already names it, so the card's
/// artist is written back into each one; otherwise they read as songs by nobody.
fn top_result(card: &Value) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let title_node = card.get("title");
    let title = title_node.map(json::runs_text).unwrap_or_default();
    let subtitle = card
        .get("subtitle")
        .map(json::runs_text)
        .unwrap_or_default();
    let title_destination = title_node
        .and_then(|node| json::runs(node).first())
        .and_then(|run| run.get("navigationEndpoint"))
        .and_then(destination)
        .or_else(|| card.get("onTap").and_then(destination));
    let tap_video = card
        .get("onTap")
        .and_then(|tap| tap.get("watchEndpoint"))
        .and_then(|endpoint| endpoint.get("videoId"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let leading_token = subtitle
        .split('•')
        .next()
        .map(str::trim)
        .unwrap_or_default();
    let thumbnail = card
        .get("thumbnail")
        .and_then(|node| json::thumbnail(node, THUMBNAIL_BUDGET));

    let featured = if let Some(video_id) = tap_video {
        let refs = references(&card.get("subtitle").into_iter().collect::<Vec<_>>());
        Some(CatalogItem {
            kind: token_kind(leading_token)
                .filter(|kind| *kind == CatalogItemKind::Video)
                .unwrap_or(CatalogItemKind::Song),
            id: video_id.clone(),
            title: title.clone(),
            subtitle: subtitle.clone(),
            artist: refs.artist.as_ref().map(|(name, _)| name.clone()),
            artist_id: refs.artist.as_ref().map(|(_, id)| id.clone()),
            album: refs.album.as_ref().map(|(name, _)| name.clone()),
            album_id: refs.album.as_ref().map(|(_, id)| id.clone()),
            duration: None,
            thumbnail,
            video_id: Some(video_id),
            explicit: is_explicit(card),
            color: None,
        })
    } else {
        title_destination.and_then(|destination| {
            let kind = destination.kind.or_else(|| token_kind(leading_token))?;
            Some(CatalogItem {
                kind,
                id: destination.browse_id,
                title: title.clone(),
                subtitle: subtitle.clone(),
                artist: None,
                artist_id: None,
                album: None,
                album_id: None,
                duration: None,
                thumbnail,
                video_id: None,
                explicit: false,
                color: None,
            })
        })
    };
    let Some(featured) = featured else {
        return items;
    };
    let artist = (featured.kind == CatalogItemKind::Artist)
        .then(|| (featured.title.clone(), featured.id.clone()));
    items.push(featured);

    for row in card
        .get("contents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| node.get("musicResponsiveListItemRenderer"))
        .filter_map(responsive_item)
    {
        let mut row = row;
        if let (None, Some((name, id))) = (&row.artist, &artist) {
            row.artist = Some(name.clone());
            row.artist_id = Some(id.clone());
            let mut parts: Vec<&str> = row.subtitle.split(" • ").collect();
            parts.insert(1.min(parts.len()), name);
            row.subtitle = parts.join(" • ");
        }
        items.push(row);
    }
    items
}

/// Groups search rows into the shelves the shell renders.
///
/// Explicit `musicShelfRenderer` titles are preferred; the modern flat "all" response has none,
/// so rows are then grouped by kind under stable titles. A top result, when there is one, comes
/// first, and the rows inside it are not repeated in the groups below.
///
/// A filtered tab ends its one shelf with a cursor for the rows after it, which is kept; whether
/// a tab is continued is the caller's decision, since only some tabs read as one growing list.
pub fn search_page(query: &str, response: &Value) -> CatalogPage {
    let mut shelves: Vec<CatalogShelf> = Vec::new();
    let cards = json::collect(response, "musicCardShelfRenderer");
    let top: Vec<CatalogItem> = cards
        .first()
        .map(|card| top_result(card))
        .unwrap_or_default();
    for shelf in json::collect(response, "musicShelfRenderer") {
        let title = shelf.get("title").map(json::runs_text).unwrap_or_default();
        let items: Vec<CatalogItem> = json::collect(shelf, "musicResponsiveListItemRenderer")
            .into_iter()
            .filter_map(responsive_item)
            .collect();
        if title.is_empty() || items.is_empty() {
            continue;
        }
        shelves.push(CatalogShelf {
            id: format!("shelf-{}", shelves.len()),
            title,
            items,
            layout: ShelfLayout::List,
        });
    }

    if shelves.is_empty() {
        let inside_cards: Vec<&Value> = cards
            .iter()
            .flat_map(|card| json::collect(card, "musicResponsiveListItemRenderer"))
            .collect();
        let rows = json::collect(response, "musicResponsiveListItemRenderer")
            .into_iter()
            .filter(|row| !inside_cards.iter().any(|inner| std::ptr::eq(*inner, *row)))
            .filter_map(responsive_item)
            .collect();
        shelves = group_by_kind(rows);
    }

    if !top.is_empty() {
        shelves.insert(
            0,
            CatalogShelf {
                id: "top-result".to_owned(),
                title: "Top result".to_owned(),
                items: top,
                layout: ShelfLayout::List,
            },
        );
    }

    CatalogPage {
        id: format!("search:{query}"),
        title: query.to_owned(),
        subtitle: String::new(),
        shelves,
        tracks: Vec::new(),
        thumbnail: None,
        next_cursor: continuation_token(response),
        truncated: false,
        all_tracks_id: None,
        artist: None,
    }
}

/// Builds the rows after a filtered search tab's first page.
///
/// Upstream answers with a bare `musicShelfContinuation`: rows and the next cursor, but no title,
/// because they belong to the shelf already shown. The caller says which shelf that is, so the
/// rows land under it rather than under a second heading of their own.
pub fn search_continuation_page(shelf_id: &str, title: &str, response: &Value) -> CatalogPage {
    let items: Vec<CatalogItem> = json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .collect();
    let shelves = if items.is_empty() {
        Vec::new()
    } else {
        vec![CatalogShelf {
            id: shelf_id.to_owned(),
            title: title.to_owned(),
            items,
            layout: ShelfLayout::List,
        }]
    };
    CatalogPage {
        id: "continuation".to_owned(),
        shelves,
        next_cursor: continuation_token(response),
        ..Default::default()
    }
}

const KIND_ORDER: [(CatalogItemKind, &str); 5] = [
    (CatalogItemKind::Song, "Songs"),
    (CatalogItemKind::Artist, "Artists"),
    (CatalogItemKind::Album, "Albums & singles"),
    (CatalogItemKind::Video, "Videos"),
    (CatalogItemKind::Playlist, "Community playlists"),
];

fn group_by_kind(items: Vec<CatalogItem>) -> Vec<CatalogShelf> {
    KIND_ORDER
        .iter()
        .filter_map(|(kind, title)| {
            let items: Vec<CatalogItem> = items
                .iter()
                .filter(|item| item.kind == *kind)
                .cloned()
                .collect();
            if items.is_empty() {
                return None;
            }
            Some(CatalogShelf {
                id: format!("all-{}", title.to_lowercase().replace(' ', "-")),
                title: (*title).to_owned(),
                items,
                // Songs and videos are rows in YouTube Music's search; the rest are cards.
                layout: if matches!(kind, CatalogItemKind::Song | CatalogItemKind::Video) {
                    ShelfLayout::List
                } else {
                    ShelfLayout::Cards
                },
            })
        })
        .collect()
}

/// Separates the browse id from its parameters inside a category's opaque id.
///
/// Neither half can contain it: browse ids are word characters and the parameters are URL-safe
/// base64.
pub const CATEGORY_SEPARATOR: char = '|';

/// Normalizes one `musicNavigationButtonRenderer`, a mood or genre button on Moods & genres.
///
/// Opening one needs its browse id and its parameters together, so both travel in the item's id
/// and only the service ever splits them again. The button's stripe colour is kept, so a shell
/// can draw the page as YouTube Music does rather than as a wall of grey tiles.
pub fn navigation_button_item(node: &Value) -> Option<CatalogItem> {
    let title = node.get("buttonText").map(json::runs_text)?;
    if title.is_empty() {
        return None;
    }
    let browse = json::first(node.get("clickCommand")?, "browseEndpoint")?;
    let browse_id = browse.get("browseId").and_then(Value::as_str)?;
    let params = browse
        .get("params")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let color = json::path(node, &["solid", "leftStripeColor"])
        .and_then(Value::as_u64)
        .map(|argb| format!("#{:06X}", argb & 0x00FF_FFFF));
    Some(CatalogItem {
        kind: CatalogItemKind::Category,
        id: format!("{browse_id}{CATEGORY_SEPARATOR}{params}"),
        title,
        subtitle: String::new(),
        artist: None,
        artist_id: None,
        album: None,
        album_id: None,
        duration: None,
        thumbnail: None,
        video_id: None,
        explicit: false,
        color,
    })
}

/// Splits a category id made by [`navigation_button_item`] into its browse id and parameters.
pub fn split_category_id(id: &str) -> Option<(&str, &str)> {
    let (browse_id, params) = id.split_once(CATEGORY_SEPARATOR)?;
    (!browse_id.trim().is_empty()).then_some((browse_id, params))
}

/// Reads every carousel of a browse response into shelves (home, explore, moods, charts).
pub fn browse_shelves(response: &Value) -> Vec<CatalogShelf> {
    let mut shelves = Vec::new();
    // One pass in document order. Each kind used to be collected separately, so every carousel
    // came before every list shelf and grid, whatever order the page itself had.
    let sections = json::collect_any(
        response,
        &[
            "musicCarouselShelfRenderer",
            "musicShelfRenderer",
            "gridRenderer",
        ],
    );
    for (kind, section) in sections {
        let untitled = || format!("Shelf {}", shelves.len() + 1);
        let shelf = match kind {
            "musicCarouselShelfRenderer" => {
                let title = section
                    .get("header")
                    .map(|header| {
                        json::first(header, "title")
                            .map(json::runs_text)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                let cards: Vec<CatalogItem> = json::collect(section, "musicTwoRowItemRenderer")
                    .into_iter()
                    .filter_map(two_row_item)
                    .collect();
                let rows: Vec<CatalogItem> =
                    json::collect(section, "musicResponsiveListItemRenderer")
                        .into_iter()
                        .filter_map(responsive_item)
                        .collect();
                // A carousel of song rows is "Quick picks": a grid of rows, not of cards.
                let layout = if cards.is_empty() {
                    ShelfLayout::List
                } else {
                    ShelfLayout::Cards
                };
                CatalogShelf {
                    id: format!("carousel-{}", shelves.len()),
                    title: if title.is_empty() { untitled() } else { title },
                    items: cards.into_iter().chain(rows).collect(),
                    layout,
                }
            }
            // Several recommendation modules use a plain music shelf of responsive song rows
            // rather than artwork cards. The old carousel-only parser silently discarded them.
            "musicShelfRenderer" => {
                let title = section
                    .get("title")
                    .map(json::runs_text)
                    .unwrap_or_default();
                CatalogShelf {
                    id: format!("shelf-{}", shelves.len()),
                    title: if title.is_empty() { untitled() } else { title },
                    items: json::collect(section, "musicResponsiveListItemRenderer")
                        .into_iter()
                        .filter_map(responsive_item)
                        .collect(),
                    layout: ShelfLayout::List,
                }
            }
            // Library and browse continuations sometimes return a grid directly under the
            // selected tab. Treat it as one shelf so those cards do not disappear just because
            // the wrapper differs from Home's.
            // Moods & genres is two grids of navigation buttons under their own headings.
            _ => {
                let title = json::path(section, &["header", "gridHeaderRenderer", "title"])
                    .map(json::runs_text)
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "More".to_owned());
                let cards = json::collect(section, "musicTwoRowItemRenderer")
                    .into_iter()
                    .filter_map(two_row_item);
                let buttons = json::collect(section, "musicNavigationButtonRenderer")
                    .into_iter()
                    .filter_map(navigation_button_item);
                CatalogShelf {
                    id: format!("grid-{}", shelves.len()),
                    title,
                    items: cards.chain(buttons).collect(),
                    layout: ShelfLayout::Cards,
                }
            }
        };
        if !shelf.items.is_empty() {
            shelves.push(shelf);
        }
    }
    shelves
}

/// Finds the next opaque browse cursor without interpreting or persisting it.
pub fn continuation_token(response: &Value) -> Option<String> {
    json::collect(response, "continuationItemRenderer")
        .into_iter()
        .find_map(|item| {
            json::path(
                item,
                &["continuationEndpoint", "continuationCommand", "token"],
            )
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
        })
        .or_else(|| {
            json::collect(response, "nextContinuationData")
                .into_iter()
                .find_map(|item| {
                    item.get("continuation")
                        .and_then(Value::as_str)
                        .filter(|token| !token.is_empty())
                        .map(str::to_owned)
                })
        })
}

/// Builds a browse page of shelves.
pub fn browse_page(id: &str, title: &str, response: &Value) -> CatalogPage {
    CatalogPage {
        id: id.to_owned(),
        title: title.to_owned(),
        subtitle: String::new(),
        shelves: browse_shelves(response),
        tracks: Vec::new(),
        thumbnail: None,
        next_cursor: continuation_token(response),
        truncated: false,
        all_tracks_id: None,
        artist: None,
    }
}

/// Builds the next chunk of a browse surface. Continuation responses contain the same shelf
/// renderers as an initial browse, wrapped in append actions rather than a selected tab.
pub fn browse_continuation_page(response: &Value) -> CatalogPage {
    CatalogPage {
        id: "continuation".to_owned(),
        shelves: browse_shelves(response),
        next_cursor: continuation_token(response),
        ..Default::default()
    }
}

/// Reads the title/subtitle of an album, playlist, or artist page from whichever header
/// renderer this response happens to use.
fn header_text(response: &Value) -> (String, String) {
    for key in [
        "musicResponsiveHeaderRenderer",
        "musicDetailHeaderRenderer",
        "musicImmersiveHeaderRenderer",
        "musicEditablePlaylistDetailHeaderRenderer",
    ] {
        let Some(header) = json::first(response, key) else {
            continue;
        };
        let title = header.get("title").map(json::runs_text).unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let subtitle = [
            "straplineTextOne",
            "subtitle",
            "description",
            "secondSubtitle",
        ]
        .iter()
        .filter_map(|field| header.get(*field))
        .map(json::runs_text)
        .find(|text| !text.is_empty())
        .unwrap_or_default();
        return (title, subtitle);
    }
    (String::new(), String::new())
}

/// The artist an album page's header names, read from its linked byline.
fn header_artist(response: &Value) -> Option<(String, String)> {
    ["musicResponsiveHeaderRenderer", "musicDetailHeaderRenderer"]
        .iter()
        .filter_map(|key| json::first(response, key))
        .find_map(|header| {
            let bylines: Vec<&Value> = ["straplineTextOne", "subtitle"]
                .iter()
                .filter_map(|field| header.get(*field))
                .collect();
            references(&bylines).artist
        })
}

/// Fills a row's missing artist or album from the page it was listed on.
///
/// An album's rows leave out what the header already says: the album itself always, and the
/// artist whenever every song is by the album's artist. A song taken from such a page into the
/// queue would otherwise have nowhere to go from the player's Go to Artist and Go to Album.
fn inherit(row: &mut CatalogItem, artist: Option<&(String, String)>, album: Option<(&str, &str)>) {
    if let (None, Some((name, id))) = (&row.artist_id, artist) {
        row.artist.get_or_insert_with(|| name.clone());
        row.artist_id = Some(id.clone());
    }
    if let (None, Some((name, id))) = (&row.album_id, album) {
        row.album.get_or_insert_with(|| name.to_owned());
        row.album_id = Some(id.to_owned());
    }
}

/// Builds an ordered track list page for an album or playlist.
pub fn track_list_page(id: &str, response: &Value) -> CatalogPage {
    let (title, subtitle) = header_text(response);
    // Only an album's header speaks for its rows; a playlist's byline is its owner, not the
    // artist of every song on it.
    let is_album = id.starts_with("MPRE");
    let artist = is_album.then(|| header_artist(response)).flatten();
    let album = (is_album && !title.is_empty()).then_some((title.as_str(), id));
    let tracks: Vec<CatalogItem> = json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .filter(|item| item.video_id.is_some())
        .map(|mut item| {
            inherit(&mut item, artist.as_ref(), album);
            item
        })
        .collect();
    CatalogPage {
        id: id.to_owned(),
        title: if title.is_empty() {
            id.to_owned()
        } else {
            title
        },
        subtitle,
        shelves: Vec::new(),
        tracks,
        thumbnail: json::thumbnail(response, THUMBNAIL_BUDGET),
        next_cursor: None,
        truncated: false,
        all_tracks_id: None,
        artist: None,
    }
}

/// Builds an artist page: its header, its top tracks, and its carousels.
pub fn artist_page(id: &str, response: &Value) -> CatalogPage {
    let (title, subtitle) = header_text(response);
    let artist = (!title.is_empty()).then(|| (title.clone(), id.to_owned()));
    let tracks: Vec<CatalogItem> = json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .filter(|item| item.video_id.is_some())
        .map(|mut item| {
            inherit(&mut item, artist.as_ref(), None);
            item
        })
        .collect();
    CatalogPage {
        id: id.to_owned(),
        title: if title.is_empty() {
            id.to_owned()
        } else {
            title
        },
        subtitle,
        shelves: browse_shelves(response),
        tracks,
        // The header's own picture or none. Searching the whole response picked whichever
        // thumbnail came nearest the budget, which was usually a song or a video still.
        thumbnail: artist_header_node(response)
            .and_then(|header| json::thumbnail(header, THUMBNAIL_BUDGET)),
        next_cursor: None,
        all_tracks_id: all_tracks_id(response),
        truncated: false,
        artist: artist_header(response),
    }
}

/// Header renderers an artist page arrives with, newest layout first.
const ARTIST_HEADERS: [&str; 3] = [
    "musicImmersiveHeaderRenderer",
    "musicVisualHeaderRenderer",
    "musicDetailHeaderRenderer",
];

/// A square portrait is asked for at this size, enough for a hero on a high-density display.
const PORTRAIT_SIZE: u64 = 1080;
/// A banner is asked for at this width, its height following the picture's own proportions.
/// Some artists list nothing wider than 1080, which is soft across a wide window.
const BANNER_WIDTH: u64 = 2880;

fn artist_header_node(response: &Value) -> Option<&Value> {
    let header = response.get("header")?;
    ARTIST_HEADERS.iter().find_map(|key| header.get(*key))
}

/// Reads the artist's pictures and facts from the page header, and from nowhere else.
///
/// The immersive header has one wide picture. The visual header has a wide one behind a square
/// portrait. A header whose only picture is square has a portrait and no banner, because
/// stretching a square across a hero is what made the old page blurry.
fn artist_header(response: &Value) -> Option<ArtistHeader> {
    let header = artist_header_node(response)?;
    let text = |field: &str| {
        header
            .get(field)
            .map(json::runs_text)
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let mut banner = None;
    let mut portrait = None;
    if let Some(picture) = header.get("thumbnail").and_then(largest_thumbnail) {
        if picture.width > picture.height {
            banner = Some(if picture.width < BANNER_WIDTH {
                let height = picture.height * BANNER_WIDTH / picture.width;
                sized(&picture.url, BANNER_WIDTH, height)
            } else {
                picture.url
            });
        } else {
            portrait = Some(picture.url);
        }
    }
    if let Some(picture) = header
        .get("foregroundThumbnail")
        .and_then(largest_thumbnail)
    {
        portrait = Some(picture.url);
    }
    let subscribers = json::first(header, "subscribeButtonRenderer")
        .and_then(|button| button.get("subscriberCountText"))
        .map(json::runs_text)
        .filter(|text| !text.is_empty());
    let found = ArtistHeader {
        banner,
        portrait: portrait.map(|url| sized(&url, PORTRAIT_SIZE, PORTRAIT_SIZE)),
        audience: text("monthlyListenerCount"),
        description: text("description"),
        subscribers,
    };
    (found != ArtistHeader::default()).then_some(found)
}

struct Picture {
    url: String,
    width: u64,
    height: u64,
}

/// The widest entry of a renderer's thumbnail list, with its dimensions.
fn largest_thumbnail(node: &Value) -> Option<Picture> {
    json::collect(node, "thumbnails")
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|entry| {
            Some(Picture {
                url: entry.get("url")?.as_str()?.to_owned(),
                width: entry.get("width").and_then(Value::as_u64).unwrap_or(0),
                height: entry.get("height").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .max_by_key(|picture| picture.width)
}

/// Asks Google's image host for a picture `width` by `height`.
///
/// The host serves any size named in the URL's last segment: `=w226-h226-...` or `=s226-...`.
/// Only that leading size token changes, so crop and format options after it are kept. An
/// `s` token names one edge of a square, so it takes `width`. A URL with neither token is
/// returned unchanged rather than guessed at.
fn sized(url: &str, width: u64, height: u64) -> String {
    let Some((base, options)) = url.rsplit_once('=') else {
        return url.to_owned();
    };
    let (token, rest) = options.split_once('-').unwrap_or((options, ""));
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let replaced = if token.strip_prefix('s').is_some_and(digits) {
        format!("s{width}")
    } else if token.strip_prefix('w').is_some_and(digits) {
        // `w<n>-h<n>` spans two tokens; drop the height that follows the width.
        let rest = match rest.split_once('-') {
            Some((height, after)) if height.strip_prefix('h').is_some_and(digits) => after,
            None if rest.strip_prefix('h').is_some_and(digits) => "",
            _ => rest,
        };
        return join(base, &format!("w{width}-h{height}"), rest);
    } else {
        return url.to_owned();
    };
    join(base, &replaced, rest)
}

fn join(base: &str, size: &str, rest: &str) -> String {
    if rest.is_empty() {
        format!("{base}={size}")
    } else {
        format!("{base}={size}-{rest}")
    }
}

/// The full song list behind an artist's top songs.
///
/// The songs shelf shows five rows and links the rest from its "Show all" button, and from its
/// title on layouts that have no button. Either way the target is a playlist browse id.
fn all_tracks_id(response: &Value) -> Option<String> {
    json::collect(response, "musicShelfRenderer")
        .into_iter()
        .find_map(|shelf| {
            shelf
                .pointer("/bottomEndpoint/browseEndpoint/browseId")
                .or_else(|| {
                    shelf.pointer("/title/runs/0/navigationEndpoint/browseEndpoint/browseId")
                })
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flex(text: Value) -> Value {
        json!({"musicResponsiveListItemFlexColumnRenderer": {"text": text}})
    }

    fn song_row() -> Value {
        json!({
            "flexColumns": [
                flex(json!({"runs": [{
                    "text": "Afterglow",
                    "navigationEndpoint": {"watchEndpoint": {"videoId": "abcdefghijk"}}
                }]})),
                flex(json!({"runs": [
                    {"text": "Song"},
                    {"text": " • "},
                    {
                        "text": "Signal Fires",
                        "navigationEndpoint": {"browseEndpoint": {
                            "browseId": "UCartist",
                            "browseEndpointContextSupportedConfigs": {
                                "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ARTIST"}
                            }
                        }}
                    },
                    {"text": " • "},
                    {
                        "text": "Night Windows",
                        "navigationEndpoint": {"browseEndpoint": {
                            "browseId": "MPREalbum",
                            "browseEndpointContextSupportedConfigs": {
                                "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                            }
                        }}
                    },
                    {"text": " • "},
                    {"text": "3:42"}
                ]}))
            ],
            "playlistItemData": {"videoId": "abcdefghijk"},
            "badges": [{"musicInlineBadgeRenderer": {"icon": {"iconType": "MUSIC_EXPLICIT_BADGE"}}}],
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://example.test/art.jpg", "width": 226, "height": 226}
            ]}}}
        })
    }

    #[test]
    fn song_row_normalizes_identity_artist_album_and_duration() {
        let item = responsive_item(&song_row()).expect("row parses");
        assert_eq!(item.kind, CatalogItemKind::Song);
        assert_eq!(item.id, "abcdefghijk");
        assert_eq!(item.video_id.as_deref(), Some("abcdefghijk"));
        assert_eq!(item.title, "Afterglow");
        assert_eq!(item.artist.as_deref(), Some("Signal Fires"));
        assert_eq!(item.artist_id.as_deref(), Some("UCartist"));
        assert_eq!(item.album.as_deref(), Some("Night Windows"));
        assert_eq!(item.album_id.as_deref(), Some("MPREalbum"));
        assert_eq!(item.duration.as_deref(), Some("3:42"));
        assert_eq!(
            item.thumbnail.as_deref(),
            Some("https://example.test/art.jpg")
        );
        assert!(item.explicit);
    }

    #[test]
    fn album_row_uses_its_browse_destination_and_is_not_playable() {
        let row = json!({
            "flexColumns": [
                flex(json!({"runs": [{
                    "text": "Night Windows",
                    "navigationEndpoint": {"browseEndpoint": {
                        "browseId": "MPREalbum",
                        "browseEndpointContextSupportedConfigs": {
                            "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                        }
                    }}
                }]})),
                flex(json!({"runs": [{"text": "Album"}, {"text": " • "}, {"text": "Signal Fires"}]}))
            ]
        });
        let item = responsive_item(&row).expect("row parses");
        assert_eq!(item.kind, CatalogItemKind::Album);
        assert_eq!(item.id, "MPREalbum");
        assert!(item.video_id.is_none());
        assert!(!item.explicit);
    }

    #[test]
    fn rows_without_a_destination_are_skipped_rather_than_guessed() {
        let row = json!({"flexColumns": [flex(json!({"runs": [{"text": "Some header"}]}))]});
        assert!(responsive_item(&row).is_none());
    }

    #[test]
    fn a_songs_artist_is_not_mistaken_for_the_row_destination() {
        let item = responsive_item(&song_row()).expect("row parses");
        assert_ne!(item.id, "UCartist");
    }

    #[test]
    fn flat_search_results_are_grouped_by_kind_in_a_stable_order() {
        let response = json!({"contents": [
            {"musicResponsiveListItemRenderer": song_row()},
            {"musicResponsiveListItemRenderer": {
                "flexColumns": [
                    flex(json!({"runs": [{
                        "text": "Signal Fires",
                        "navigationEndpoint": {"browseEndpoint": {
                            "browseId": "UCartist",
                            "browseEndpointContextSupportedConfigs": {
                                "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ARTIST"}
                            }
                        }}
                    }]})),
                    flex(json!({"runs": [{"text": "Artist"}]}))
                ]
            }}
        ]});
        let page = search_page("signal", &response);
        assert_eq!(page.shelves.len(), 2);
        assert_eq!(page.shelves[0].title, "Songs");
        assert_eq!(page.shelves[1].title, "Artists");
        assert_eq!(page.id, "search:signal");
    }

    #[test]
    fn a_top_result_card_leads_and_lends_its_artist_to_its_songs() {
        let artist = json!({"browseEndpoint": {
            "browseId": "UCbunny",
            "browseEndpointContextSupportedConfigs": {
                "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ARTIST"}
            }
        }});
        let card_song = json!({
            "flexColumns": [
                flex(json!({"runs": [{"text": "DtMF"}]})),
                flex(json!({"runs": [{"text": "Song"}, {"text": " • "}, {"text": "3:58"}]}))
            ],
            "playlistItemData": {"videoId": "dtmf"}
        });
        let response = json!({"contents": [
            {"musicCardShelfRenderer": {
                "title": {"runs": [{"text": "Bad Bunny", "navigationEndpoint": artist}]},
                "subtitle": {"runs": [{"text": "Artist"}, {"text": " • "}, {"text": "162M monthly audience"}]},
                "contents": [{"musicResponsiveListItemRenderer": card_song}]
            }},
            {"musicResponsiveListItemRenderer": song_row()}
        ]});
        let page = search_page("bad bunny", &response);
        assert_eq!(page.shelves[0].title, "Top result");
        let top = &page.shelves[0].items;
        assert_eq!(top[0].kind, CatalogItemKind::Artist);
        assert_eq!(top[0].id, "UCbunny");
        assert_eq!(top[1].artist.as_deref(), Some("Bad Bunny"));
        assert_eq!(top[1].subtitle, "Song • Bad Bunny • 3:58");
        // The card's song is not listed a second time under Songs.
        let songs = &page.shelves[1];
        assert_eq!(songs.title, "Songs");
        assert!(songs.items.iter().all(|item| item.id != "dtmf"));
    }

    #[test]
    fn podcast_episodes_are_filed_as_videos_not_songs() {
        let row = json!({
            "flexColumns": [
                flex(json!({"runs": [{"text": "An episode"}]})),
                flex(json!({"runs": [{"text": "Episode"}, {"text": " • "}, {"text": "Feb 11"}]}))
            ],
            "playlistItemData": {"videoId": "ep1"}
        });
        let item = responsive_item(&row).unwrap();
        assert_eq!(item.kind, CatalogItemKind::Video);
    }

    #[test]
    fn mood_buttons_become_categories_that_carry_their_parameters_and_colour() {
        let response = json!({"gridRenderer": {
            "header": {"gridHeaderRenderer": {"title": {"runs": [{"text": "Moods & moments"}]}}},
            "items": [{"musicNavigationButtonRenderer": {
                "buttonText": {"runs": [{"text": "Chill"}]},
                "solid": {"leftStripeColor": 4288988671u64},
                "clickCommand": {"browseEndpoint": {
                    "browseId": "FEmusic_moods_and_genres_category",
                    "params": "ggMPOg1uX1JOQWZFeDByc2Jm"
                }}
            }}]
        }});
        let shelves = browse_shelves(&response);
        assert_eq!(shelves[0].title, "Moods & moments");
        let chill = &shelves[0].items[0];
        assert_eq!(chill.kind, CatalogItemKind::Category);
        assert_eq!(chill.title, "Chill");
        assert_eq!(chill.color.as_deref(), Some("#A4C5FF"));
        assert_eq!(
            split_category_id(&chill.id),
            Some((
                "FEmusic_moods_and_genres_category",
                "ggMPOg1uX1JOQWZFeDByc2Jm"
            ))
        );
    }

    #[test]
    fn a_category_id_without_a_browse_id_is_refused() {
        assert_eq!(split_category_id("|params"), None);
        assert_eq!(split_category_id("no separator"), None);
    }

    #[test]
    fn titled_shelves_win_over_kind_grouping() {
        let response = json!({"musicShelfRenderer": {
            "title": {"runs": [{"text": "Top result"}]},
            "contents": [{"musicResponsiveListItemRenderer": song_row()}]
        }});
        let page = search_page("signal", &response);
        assert_eq!(page.shelves.len(), 1);
        assert_eq!(page.shelves[0].title, "Top result");
    }

    #[test]
    fn a_filtered_search_keeps_the_cursor_its_shelf_ends_with() {
        let response = json!({"contents": {"tabbedSearchResultsRenderer": {"tabs": [{"tabRenderer": {
            "content": {"sectionListRenderer": {"contents": [{"musicShelfRenderer": {
                "title": {"runs": [{"text": "Songs"}]},
                "contents": [{"musicResponsiveListItemRenderer": song_row()}],
                "continuations": [{"nextContinuationData": {"continuation": "songs-page-two"}}]
            }}]}}
        }}]}}});
        let page = search_page("signal", &response);
        assert_eq!(page.shelves[0].title, "Songs");
        assert_eq!(page.next_cursor.as_deref(), Some("songs-page-two"));
    }

    #[test]
    fn a_search_continuation_extends_the_shelf_it_was_asked_for() {
        let response = json!({
            "continuationContents": {"musicShelfContinuation": {
                "contents": [{"musicResponsiveListItemRenderer": song_row()}],
                "continuations": [{"nextContinuationData": {"continuation": "songs-page-three"}}]
            }},
            "header": {"musicHeaderRenderer": {"header": {"chipCloudRenderer": {"chips": []}}}}
        });
        let page = search_continuation_page("shelf-0", "Songs", &response);
        assert_eq!(page.shelves.len(), 1);
        assert_eq!(page.shelves[0].id, "shelf-0");
        assert_eq!(page.shelves[0].title, "Songs");
        assert_eq!(page.shelves[0].layout, ShelfLayout::List);
        assert_eq!(page.shelves[0].items[0].id, "abcdefghijk");
        assert_eq!(page.next_cursor.as_deref(), Some("songs-page-three"));
    }

    #[test]
    fn the_last_search_continuation_has_no_cursor() {
        let response = json!({"continuationContents": {"musicShelfContinuation": {
            "contents": [{"musicResponsiveListItemRenderer": song_row()}]
        }}});
        let page = search_continuation_page("shelf-0", "Songs", &response);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn shelves_keep_page_order_and_say_whether_they_are_rows() {
        let card = json!({"musicTwoRowItemRenderer": {
            "title": {"runs": [{"text": "Night Windows"}]},
            "subtitle": {"runs": [{"text": "Album • Signal Fires"}]},
            "navigationEndpoint": {"browseEndpoint": {
                "browseId": "MPREalbum",
                "browseEndpointContextSupportedConfigs": {
                    "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                }
            }}
        }});
        let carousel = |title: &str, item: Value| {
            json!({"musicCarouselShelfRenderer": {
                "header": {"musicCarouselShelfBasicHeaderRenderer": {
                    "title": {"runs": [{"text": title}]}
                }},
                "contents": [item]
            }})
        };
        let response = json!({"sectionListRenderer": {"contents": [
            carousel("Listen again", card),
            {"musicShelfRenderer": {
                "title": {"runs": [{"text": "Recommended"}]},
                "contents": [{"musicResponsiveListItemRenderer": song_row()}]
            }},
            carousel("Quick picks", json!({"musicResponsiveListItemRenderer": song_row()})),
        ]}});
        let shelves = browse_shelves(&response);
        let seen: Vec<(&str, ShelfLayout)> = shelves
            .iter()
            .map(|shelf| (shelf.title.as_str(), shelf.layout))
            .collect();
        assert_eq!(
            seen,
            [
                ("Listen again", ShelfLayout::Cards),
                ("Recommended", ShelfLayout::List),
                ("Quick picks", ShelfLayout::List),
            ]
        );
    }

    #[test]
    fn a_cards_shelf_leaves_layout_off_the_wire() {
        let shelf = CatalogShelf {
            id: "s".into(),
            title: "t".into(),
            items: Vec::new(),
            layout: ShelfLayout::Cards,
        };
        let wire = serde_json::to_value(&shelf).unwrap();
        assert!(wire.get("layout").is_none());
        let list = CatalogShelf {
            layout: ShelfLayout::List,
            ..shelf
        };
        assert_eq!(serde_json::to_value(&list).unwrap()["layout"], "list");
    }

    #[test]
    fn carousel_cards_become_shelf_items() {
        let response = json!({"musicCarouselShelfRenderer": {
            "header": {"musicCarouselShelfBasicHeaderRenderer": {
                "title": {"runs": [{"text": "Listen again"}]}
            }},
            "contents": [{"musicTwoRowItemRenderer": {
                "title": {"runs": [{
                    "text": "Night Windows",
                    "navigationEndpoint": {"browseEndpoint": {
                        "browseId": "MPREalbum",
                        "browseEndpointContextSupportedConfigs": {
                            "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                        }
                    }}
                }]},
                "subtitle": {"runs": [{"text": "Album • Signal Fires"}]},
                "navigationEndpoint": {"browseEndpoint": {
                    "browseId": "MPREalbum",
                    "browseEndpointContextSupportedConfigs": {
                        "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                    }
                }}
            }}]
        }});
        let shelves = browse_shelves(&response);
        assert_eq!(shelves.len(), 1);
        assert_eq!(shelves[0].title, "Listen again");
        assert_eq!(shelves[0].items[0].kind, CatalogItemKind::Album);
        assert_eq!(shelves[0].items[0].id, "MPREalbum");
    }

    #[test]
    fn browse_keeps_responsive_song_shelves_and_the_next_cursor() {
        let response = json!({
            "contents": [
                {"musicShelfRenderer": {
                    "title": {"runs": [{"text": "Quick picks"}]},
                    "contents": [{"musicResponsiveListItemRenderer": song_row()}]
                }},
                {"continuationItemRenderer": {"continuationEndpoint": {
                    "continuationCommand": {"token": "next-home-page"}
                }}}
            ]
        });
        let page = browse_page("FEmusic_home", "Home", &response);
        assert_eq!(page.shelves.len(), 1);
        assert_eq!(page.shelves[0].title, "Quick picks");
        assert_eq!(page.shelves[0].items[0].id, "abcdefghijk");
        assert_eq!(page.next_cursor.as_deref(), Some("next-home-page"));
    }

    #[test]
    fn a_continuation_page_retains_shelves_and_its_following_cursor() {
        let response = json!({
            "onResponseReceivedActions": [{"appendContinuationItemsAction": {
                "continuationItems": [
                    {"musicShelfRenderer": {
                        "title": {"runs": [{"text": "More for you"}]},
                        "contents": [{"musicResponsiveListItemRenderer": song_row()}]
                    }},
                    {"continuationItemRenderer": {"continuationEndpoint": {
                        "continuationCommand": {"token": "page-three"}
                    }}}
                ]
            }}]
        });
        let page = browse_continuation_page(&response);
        assert_eq!(page.shelves[0].title, "More for you");
        assert_eq!(page.next_cursor.as_deref(), Some("page-three"));
    }

    fn queue_row(video_id: &str, title: &str) -> Value {
        json!({
            "videoId": video_id,
            "title": {"runs": [{"text": title}]},
            "longBylineText": {"runs": [
                {
                    "text": "Signal Fires",
                    "navigationEndpoint": {"browseEndpoint": {
                        "browseId": "UCartist",
                        "browseEndpointContextSupportedConfigs": {
                            "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ARTIST"}
                        }
                    }}
                },
                {"text": " • Album • 2026"}
            ]},
            "lengthText": {"runs": [{"text": "3:42"}]},
            "thumbnail": {"thumbnails": [{"url": "https://example.test/q.jpg", "width": 226, "height": 226}]}
        })
    }

    #[test]
    fn a_queue_row_is_normalized_with_its_artist_and_duration() {
        let item = queue_item(&queue_row("abcdefghijk", "Afterglow")).expect("row parses");
        assert_eq!(item.kind, CatalogItemKind::Song);
        assert_eq!(item.id, "abcdefghijk");
        assert_eq!(item.video_id.as_deref(), Some("abcdefghijk"));
        assert_eq!(item.artist.as_deref(), Some("Signal Fires"));
        assert_eq!(item.duration.as_deref(), Some("3:42"));
        assert_eq!(
            item.thumbnail.as_deref(),
            Some("https://example.test/q.jpg")
        );
    }

    #[test]
    fn a_queue_row_without_a_video_id_is_skipped() {
        let row = json!({"title": {"runs": [{"text": "Header"}]}});
        assert!(queue_item(&row).is_none());
    }

    #[test]
    fn radio_drops_the_seed_track_so_it_is_not_replayed() {
        let response = json!({"contents": [
            {"playlistPanelVideoRenderer": queue_row("seedvideoid", "The seed")},
            {"playlistPanelVideoRenderer": queue_row("nextvideoid", "The next one")},
        ]});
        let page = radio_page("seedvideoid", &response);
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].id, "nextvideoid");
        assert_eq!(page.id, "radio:seedvideoid");
    }

    #[test]
    fn radio_keeps_a_leading_row_that_is_not_the_seed() {
        let response = json!({"contents": [
            {"playlistPanelVideoRenderer": queue_row("othervideoi", "Not the seed")},
        ]});
        assert_eq!(radio_page("seedvideoid", &response).tracks.len(), 1);
    }

    #[test]
    fn radio_removes_repeated_rows_so_a_track_cannot_play_twice_in_a_row() {
        let response = json!({"contents": [
            {"playlistPanelVideoRenderer": queue_row("aaaaaaaaaaa", "One")},
            {"playlistPanelVideoRenderer": queue_row("bbbbbbbbbbb", "Two")},
            {"playlistPanelVideoRenderer": queue_row("aaaaaaaaaaa", "One again")},
        ]});
        let page = radio_page("seedvideoid", &response);
        assert_eq!(
            page.tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["aaaaaaaaaaa", "bbbbbbbbbbb"]
        );
    }

    #[test]
    fn radio_keeps_its_own_continuation_cursor() {
        let response = json!({
            "playlistPanelRenderer": {
                "contents": [{"playlistPanelVideoRenderer": queue_row("nextvideoid", "The next one")}],
                "continuations": [{"nextRadioContinuationData": {"continuation": "same-station"}}]
            },
            "unrelated": {"continuations": [{"nextContinuationData": {"continuation": "wrong"}}]}
        });
        assert_eq!(
            radio_page("seedvideoid", &response).next_cursor.as_deref(),
            Some("same-station")
        );
    }

    #[test]
    fn track_list_page_keeps_only_playable_rows_and_reads_its_header() {
        let response = json!({
            "header": {"musicResponsiveHeaderRenderer": {
                "title": {"runs": [{"text": "Night Windows"}]},
                "straplineTextOne": {"runs": [{"text": "Signal Fires"}]}
            }},
            "contents": [
                {"musicResponsiveListItemRenderer": song_row()},
                {"musicResponsiveListItemRenderer": {
                    "flexColumns": [
                        flex(json!({"runs": [{
                            "text": "Night Windows",
                            "navigationEndpoint": {"browseEndpoint": {
                                "browseId": "MPREalbum",
                                "browseEndpointContextSupportedConfigs": {
                                    "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ALBUM"}
                                }
                            }}
                        }]})),
                        flex(json!({"runs": [{"text": "Album"}]}))
                    ]
                }}
            ]
        });
        let page = track_list_page("MPREalbum", &response);
        assert_eq!(page.title, "Night Windows");
        assert_eq!(page.subtitle, "Signal Fires");
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].video_id.as_deref(), Some("abcdefghijk"));
    }

    fn bare_row(title: &str, video_id: &str) -> Value {
        json!({"flexColumns": [
            flex(json!({"runs": [{
                "text": title,
                "navigationEndpoint": {"watchEndpoint": {"videoId": video_id}}
            }]})),
            flex(json!({"runs": [{"text": "1.2M plays"}]}))
        ]})
    }

    fn header_with_artist() -> Value {
        json!({"musicResponsiveHeaderRenderer": {
            "title": {"runs": [{"text": "Night Windows"}]},
            "straplineTextOne": {"runs": [{
                "text": "Signal Fires",
                "navigationEndpoint": {"browseEndpoint": {
                    "browseId": "UCartist",
                    "browseEndpointContextSupportedConfigs": {
                        "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_ARTIST"}
                    }
                }}
            }]}
        }})
    }

    #[test]
    fn an_albums_songs_lead_to_its_artist_and_back_to_the_album() {
        let response = json!({
            "header": header_with_artist(),
            "contents": [{"musicResponsiveListItemRenderer": bare_row("Afterglow", "abcdefghijk")}]
        });
        let page = track_list_page("MPREalbum", &response);
        let track = &page.tracks[0];
        assert_eq!(track.artist.as_deref(), Some("Signal Fires"));
        assert_eq!(track.artist_id.as_deref(), Some("UCartist"));
        assert_eq!(track.album.as_deref(), Some("Night Windows"));
        assert_eq!(track.album_id.as_deref(), Some("MPREalbum"));
    }

    #[test]
    fn a_playlists_owner_is_not_taken_for_the_artist_of_its_songs() {
        let response = json!({
            "header": header_with_artist(),
            "contents": [{"musicResponsiveListItemRenderer": bare_row("Afterglow", "abcdefghijk")}]
        });
        let page = track_list_page("VLPLplaylist", &response);
        assert_eq!(page.tracks[0].artist_id, None);
        assert_eq!(page.tracks[0].album_id, None);
    }

    #[test]
    fn an_artists_top_songs_without_a_byline_link_lead_back_to_the_artist() {
        let response = json!({
            "header": {"musicImmersiveHeaderRenderer": {"title": {"runs": [{"text": "Signal Fires"}]}}},
            "contents": [
                {"musicResponsiveListItemRenderer": bare_row("Afterglow", "abcdefghijk")},
                {"musicResponsiveListItemRenderer": song_row()}
            ]
        });
        let page = artist_page("UCartist", &response);
        assert_eq!(page.tracks[0].artist_id.as_deref(), Some("UCartist"));
        assert_eq!(page.tracks[0].artist.as_deref(), Some("Signal Fires"));
        assert_eq!(page.tracks[1].album_id.as_deref(), Some("MPREalbum"));
    }

    /// The header of a real artist page, trimmed to the fields that matter.
    fn immersive_artist_header() -> Value {
        let banner = |width: u64, height: u64| {
            json!({
                "url": format!("https://lh3.googleusercontent.com/banner=w{width}-h{height}-p-l90-rj"),
                "width": width,
                "height": height
            })
        };
        json!({"header": {"musicImmersiveHeaderRenderer": {
            "title": {"runs": [{"text": "Omar Courtz"}]},
            "subscriptionButton": {"subscribeButtonRenderer": {
                "subscriberCountText": {"runs": [{"text": "1.62M"}]},
                "channelId": "UCchannel"
            }},
            "description": {"runs": [{"text": "Known to fans as Ousi."}]},
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                banner(540, 225), banner(2880, 1200), banner(1440, 600)
            ]}}},
            "monthlyListenerCount": {"runs": [{"text": "28.9M monthly audience"}]}
        }},
        "contents": [{"musicCarouselShelfRenderer": {
            "header": {"musicCarouselShelfBasicHeaderRenderer": {"title": {"runs": [{"text": "Videos"}]}}},
            "contents": [{"musicTwoRowItemRenderer": {
                "title": {"runs": [{"text": "A video"}]},
                "thumbnailRenderer": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                    {"url": "https://i.ytimg.com/vi/still/hq.jpg", "width": 400, "height": 225}
                ]}}}
            }}]
        }}]})
    }

    #[test]
    fn an_artist_header_gives_its_largest_banner_and_its_facts() {
        let header = artist_page("UCartist", &immersive_artist_header())
            .artist
            .unwrap();
        assert_eq!(
            header.banner.as_deref(),
            Some("https://lh3.googleusercontent.com/banner=w2880-h1200-p-l90-rj")
        );
        assert_eq!(header.portrait, None);
        assert_eq!(header.audience.as_deref(), Some("28.9M monthly audience"));
        assert_eq!(
            header.description.as_deref(),
            Some("Known to fans as Ousi.")
        );
        assert_eq!(header.subscribers.as_deref(), Some("1.62M"));
    }

    #[test]
    fn an_artist_pages_picture_never_comes_from_its_shelves() {
        let page = artist_page("UCartist", &immersive_artist_header());
        let thumbnail = page.thumbnail.unwrap();
        assert!(thumbnail.contains("/banner="), "{thumbnail}");

        let mut bare = immersive_artist_header();
        bare["header"]["musicImmersiveHeaderRenderer"]
            .as_object_mut()
            .unwrap()
            .remove("thumbnail");
        let page = artist_page("UCartist", &bare);
        assert_eq!(page.thumbnail, None);
        assert_eq!(page.artist.unwrap().banner, None);
    }

    #[test]
    fn a_visual_header_has_a_banner_behind_a_large_portrait() {
        let response = json!({"header": {"musicVisualHeaderRenderer": {
            "title": {"runs": [{"text": "Signal Fires"}]},
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://lh3.googleusercontent.com/wide=w1440-h600-p", "width": 1440, "height": 600}
            ]}}},
            "foregroundThumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://lh3.googleusercontent.com/face=w226-h226-l90-rj", "width": 226, "height": 226}
            ]}}}
        }}});
        let header = artist_page("UCartist", &response).artist.unwrap();
        assert_eq!(
            header.banner.as_deref(),
            // Narrower than the hero, so it is asked for at the hero's width, in proportion.
            Some("https://lh3.googleusercontent.com/wide=w2880-h1200-p")
        );
        assert_eq!(
            header.portrait.as_deref(),
            Some("https://lh3.googleusercontent.com/face=w1080-h1080-l90-rj")
        );
    }

    #[test]
    fn a_square_only_header_is_a_portrait_and_not_a_banner() {
        let response = json!({"header": {"musicImmersiveHeaderRenderer": {
            "title": {"runs": [{"text": "Signal Fires"}]},
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://yt3.googleusercontent.com/face=s240-c-k", "width": 240, "height": 240}
            ]}}}
        }}});
        let header = artist_page("UCartist", &response).artist.unwrap();
        assert_eq!(header.banner, None);
        assert_eq!(
            header.portrait.as_deref(),
            Some("https://yt3.googleusercontent.com/face=s1080-c-k")
        );
    }

    #[test]
    fn a_header_with_nothing_to_say_is_left_out() {
        let response = json!({"header": {"musicImmersiveHeaderRenderer": {
            "title": {"runs": [{"text": "Signal Fires"}]}
        }}});
        assert_eq!(artist_page("UCartist", &response).artist, None);
    }

    #[test]
    fn only_the_leading_size_token_of_a_picture_changes() {
        assert_eq!(
            sized("https://x/a=w60-h60-l90-rj", 1080, 1080),
            "https://x/a=w1080-h1080-l90-rj"
        );
        assert_eq!(
            sized("https://x/a=w60-h25", 2880, 1200),
            "https://x/a=w2880-h1200"
        );
        assert_eq!(
            sized("https://x/a=s88-c-k", 1080, 1080),
            "https://x/a=s1080-c-k"
        );
        assert_eq!(sized("https://x/a=s88", 1080, 1080), "https://x/a=s1080");
        assert_eq!(sized("https://x/a", 1080, 1080), "https://x/a");
        assert_eq!(sized("https://x/a=p-l90", 1080, 1080), "https://x/a=p-l90");
    }

    #[test]
    fn an_artists_top_songs_link_their_full_list() {
        let response = json!({"musicShelfRenderer": {
            "title": {"runs": [{"text": "Top songs"}]},
            "contents": [{"musicResponsiveListItemRenderer": song_row()}],
            "bottomEndpoint": {"browseEndpoint": {"browseId": "VLOLAK5uy_every_song"}}
        }});
        let page = artist_page("UCartist", &response);
        assert_eq!(page.all_tracks_id.as_deref(), Some("VLOLAK5uy_every_song"));
    }

    #[test]
    fn a_songs_shelf_titled_as_a_link_still_leads_to_its_full_list() {
        let response = json!({"musicShelfRenderer": {
            "title": {"runs": [{
                "text": "Songs",
                "navigationEndpoint": {"browseEndpoint": {"browseId": "VLOLAK5uy_from_title"}}
            }]},
            "contents": [{"musicResponsiveListItemRenderer": song_row()}]
        }});
        let page = artist_page("UCartist", &response);
        assert_eq!(page.all_tracks_id.as_deref(), Some("VLOLAK5uy_from_title"));
    }

    #[test]
    fn a_complete_track_list_has_nothing_more_to_show() {
        let response = json!({"musicShelfRenderer": {
            "contents": [{"musicResponsiveListItemRenderer": song_row()}]
        }});
        assert_eq!(artist_page("UCartist", &response).all_tracks_id, None);
    }
}
