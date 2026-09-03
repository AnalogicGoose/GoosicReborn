//! Pure InnerTube JSON -> catalog normalization.
//!
//! Nothing here performs I/O, so every rule below is unit-testable against small fixtures.

use goosic_protocol::{CatalogItem, CatalogItemKind, CatalogPage, CatalogShelf};
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
        "Video" => Some(CatalogItemKind::Video),
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
                    json::path(column, &["musicResponsiveListItemFlexColumnRenderer", "text"])
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
                    json::path(column, &["musicResponsiveListItemFixedColumnRenderer", "text"])
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
    })
}

/// Normalizes one `musicTwoRowItemRenderer` (the cards inside home and explore carousels).
pub fn two_row_item(node: &Value) -> Option<CatalogItem> {
    let title_node = node.get("title")?;
    let title = json::runs_text(title_node);
    if title.is_empty() {
        return None;
    }
    let subtitle = node.get("subtitle").map(json::runs_text).unwrap_or_default();
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
    })
}

/// Flattens every responsive row in a search response, in relevance order.
pub fn search_items(response: &Value) -> Vec<CatalogItem> {
    json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .collect()
}

/// Groups search rows into the shelves the shell renders.
///
/// Explicit `musicShelfRenderer` titles are preferred; the modern flat "all" response has none,
/// so rows are then grouped by kind under stable titles.
pub fn search_page(query: &str, response: &Value) -> CatalogPage {
    let mut shelves: Vec<CatalogShelf> = Vec::new();
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
        });
    }

    if shelves.is_empty() {
        shelves = group_by_kind(search_items(response));
    }

    CatalogPage {
        id: format!("search:{query}"),
        title: query.to_owned(),
        subtitle: String::new(),
        shelves,
        tracks: Vec::new(),
        thumbnail: None,
        truncated: false,
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
            })
        })
        .collect()
}

/// Reads every carousel of a browse response into shelves (home, explore, moods, charts).
pub fn browse_shelves(response: &Value) -> Vec<CatalogShelf> {
    let mut shelves = Vec::new();
    for carousel in json::collect(response, "musicCarouselShelfRenderer") {
        let title = carousel
            .get("header")
            .map(|header| json::first(header, "title").map(json::runs_text).unwrap_or_default())
            .unwrap_or_default();
        let items: Vec<CatalogItem> = json::collect(carousel, "musicTwoRowItemRenderer")
            .into_iter()
            .filter_map(two_row_item)
            .collect();
        if items.is_empty() {
            continue;
        }
        shelves.push(CatalogShelf {
            id: format!("carousel-{}", shelves.len()),
            title: if title.is_empty() {
                format!("Shelf {}", shelves.len() + 1)
            } else {
                title
            },
            items,
        });
    }
    shelves
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
        truncated: false,
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
        let subtitle = ["straplineTextOne", "subtitle", "description", "secondSubtitle"]
            .iter()
            .filter_map(|field| header.get(*field))
            .map(json::runs_text)
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        return (title, subtitle);
    }
    (String::new(), String::new())
}

/// Builds an ordered track list page for an album or playlist.
pub fn track_list_page(id: &str, response: &Value) -> CatalogPage {
    let (title, subtitle) = header_text(response);
    let tracks: Vec<CatalogItem> = json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .filter(|item| item.video_id.is_some())
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
        truncated: false,
    }
}

/// Builds an artist page: its header, its top tracks, and its carousels.
pub fn artist_page(id: &str, response: &Value) -> CatalogPage {
    let (title, subtitle) = header_text(response);
    let tracks: Vec<CatalogItem> = json::collect(response, "musicResponsiveListItemRenderer")
        .into_iter()
        .filter_map(responsive_item)
        .filter(|item| item.video_id.is_some())
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
        thumbnail: json::thumbnail(response, THUMBNAIL_BUDGET),
        truncated: false,
    }
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
        assert_eq!(item.thumbnail.as_deref(), Some("https://example.test/art.jpg"));
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
}
