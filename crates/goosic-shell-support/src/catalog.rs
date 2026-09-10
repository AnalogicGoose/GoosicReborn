//! Turning protocol catalog pages into the shapes a screen renders.
//!
//! The decisions here are the ones that would otherwise be made three times and made differently:
//! which row may be played, which only opened, which shown but left inert; how duplicate ids from
//! upstream are kept apart; when a shelf reads better as a track list. None of them depends on a
//! toolkit.

use std::collections::HashSet;

use goosic_protocol::{CatalogItem, CatalogItemKind, CatalogPage};

use crate::navigation::EntityReference;
use crate::text::trim_spaces;

/// A playable catalog row. Every track carries a real official-player video id; rows that are not
/// directly playable are cards, never tracks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Track {
    pub id: String,
    pub title: String,
    /// The upstream descriptor, e.g. `Song • Artist • Album • 3:42`.
    pub subtitle: String,
    pub artist: String,
    pub artist_id: Option<String>,
    pub album: String,
    pub album_id: Option<String>,
    pub duration: String,
    pub video_id: String,
    pub explicit: bool,
    pub thumbnail: Option<String>,
}

impl Track {
    /// Builds a track from a catalog row, or `None` when the row is not directly playable.
    pub fn from_catalog(item: &CatalogItem) -> Option<Track> {
        let video_id = item.video_id.as_deref().filter(|id| !id.is_empty())?;
        Some(Track {
            id: video_id.to_owned(),
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            artist: item.artist.clone().unwrap_or_default(),
            artist_id: item.artist_id.clone(),
            album: item.album.clone().unwrap_or_default(),
            album_id: item.album_id.clone(),
            duration: item.duration.clone().unwrap_or_default(),
            video_id: video_id.to_owned(),
            explicit: item.explicit,
            thumbnail: item.thumbnail.clone(),
        })
    }

    /// One line under the title.
    ///
    /// Prefers the artist and album the parser resolved. Otherwise it falls back to the upstream
    /// descriptor with the parts the row already shows elsewhere removed, so a row does not read
    /// "Song • 5:21" next to its own "5:21" column.
    pub fn secondary_text(&self) -> String {
        let known: Vec<&str> =
            [self.artist.as_str(), self.album.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
        if !known.is_empty() {
            return known.join(" · ");
        }
        let redundant = ["Song", "Video", self.duration.as_str()];
        self.subtitle
            .split('•')
            .map(trim_spaces)
            .filter(|part| !part.is_empty() && !redundant.contains(part))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// What activating a card does.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CardAction {
    Show(EntityReference),
    /// Boxed because a track is several times the size of an entity reference, and every card
    /// would otherwise carry that space whether it plays or only opens something.
    Play(Box<Track>),
}

/// One catalog row as a card on a shelf.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    /// `None` for a row this build does not understand: visible, but inert.
    pub action: Option<CardAction>,
    pub thumbnail: Option<String>,
}

impl Card {
    pub fn from_catalog(item: &CatalogItem) -> Card {
        let action = match item.kind {
            CatalogItemKind::Song | CatalogItemKind::Video => {
                Track::from_catalog(item).map(|track| CardAction::Play(Box::new(track)))
            }
            CatalogItemKind::Album => Some(CardAction::Show(EntityReference::Album(item.id.clone()))),
            CatalogItemKind::Artist => {
                Some(CardAction::Show(EntityReference::Artist(item.id.clone())))
            }
            CatalogItemKind::Playlist => {
                Some(CardAction::Show(EntityReference::Playlist(item.id.clone())))
            }
            // A row this build does not understand stays visible but inert rather than
            // navigating somewhere the shell cannot render.
            CatalogItemKind::Unknown => None,
        };
        Card {
            id: item.id.clone(),
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            action,
            thumbnail: item.thumbnail.clone(),
        }
    }

    fn track(&self) -> Option<&Track> {
        match &self.action {
            Some(CardAction::Play(track)) => Some(track.as_ref()),
            _ => None,
        }
    }
}

/// A titled row of cards.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shelf {
    pub id: String,
    pub title: String,
    pub cards: Vec<Card>,
}

impl Shelf {
    /// The shelf as an ordered track list, when every row in it is playable.
    ///
    /// Search returns songs and albums in the same page shape; songs read far better as rows than
    /// as artwork cards, so the shelf decides its own presentation.
    pub fn track_list(&self) -> Option<Vec<Track>> {
        let tracks: Vec<Track> = self.cards.iter().filter_map(Card::track).cloned().collect();
        (tracks.len() == self.cards.len() && !tracks.is_empty()).then_some(tracks)
    }

    /// Keeps the first card for each id. A search page can legitimately return the same album
    /// twice, and duplicate ids render unpredictably in every toolkit's list.
    pub fn uniqued(cards: Vec<Card>) -> Vec<Card> {
        let mut seen = HashSet::new();
        cards.into_iter().filter(|card| seen.insert(card.id.clone())).collect()
    }
}

/// A catalog page in the shape the screens render.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageView {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub shelves: Vec<Shelf>,
    pub tracks: Vec<Track>,
    /// The service clamped this page to fit one protocol frame. A screen must say so.
    pub truncated: bool,
}

impl PageView {
    pub fn from_wire(page: &CatalogPage) -> PageView {
        let mut seen_shelf_ids = HashSet::new();
        let shelves = page
            .shelves
            .iter()
            .enumerate()
            .map(|(index, shelf)| {
                // Upstream ids are not guaranteed unique within a page, and a list needs them to
                // be, so collisions are disambiguated by position rather than silently merged.
                let id = if seen_shelf_ids.insert(shelf.id.clone()) {
                    shelf.id.clone()
                } else {
                    format!("{}-{index}", shelf.id)
                };
                Shelf {
                    id,
                    title: shelf.title.clone(),
                    cards: Shelf::uniqued(shelf.items.iter().map(Card::from_catalog).collect()),
                }
            })
            .collect();
        PageView {
            id: page.id.clone(),
            title: page.title.clone(),
            subtitle: page.subtitle.clone(),
            shelves,
            tracks: page.tracks.iter().filter_map(Track::from_catalog).collect(),
            truncated: page.truncated,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.shelves.is_empty() && self.tracks.is_empty()
    }

    /// Every playable row on the page, in display order, for queueing.
    pub fn playable_tracks(&self) -> Vec<Track> {
        self.tracks
            .iter()
            .cloned()
            .chain(self.shelves.iter().flat_map(|shelf| shelf.cards.iter().filter_map(Card::track).cloned()))
            .collect()
    }
}

/// The subtitle for an album, artist, or playlist page.
///
/// Upstream headers often already lead with the kind ("Playlist • 2026"), so prefixing blindly
/// produces "Playlist · Playlist • 2026".
pub fn detail_subtitle(kind_label: &str, page_subtitle: &str) -> String {
    let trimmed = trim_spaces(page_subtitle);
    if trimmed.is_empty() {
        return kind_label.to_owned();
    }
    if trimmed.to_lowercase().starts_with(&kind_label.to_lowercase()) {
        return trimmed.to_owned();
    }
    format!("{kind_label} · {trimmed}")
}

/// A service failure in words a person can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureText {
    pub title: String,
    pub detail: String,
}

/// Turns a service error code into something worth showing a person.
pub fn failure_text(code: &str, message: &str, subject: &str) -> FailureText {
    let (title, detail) = match code {
        "catalogEmpty" => ("No results", format!("{subject} returned nothing to show.")),
        "catalogUnavailable" => (
            "Catalog unreachable",
            "Could not reach YouTube Music. Check your connection and try again.".to_owned(),
        ),
        "catalogUpstreamError" => ("Catalog rejected the request", message.to_owned()),
        "catalogDecodeError" => (
            "Unreadable catalog response",
            "YouTube Music answered in a shape this build does not understand.".to_owned(),
        ),
        "offline" => ("Service not connected", message.to_owned()),
        _ => ("Could not load", message.to_owned()),
    };
    FailureText { title: title.to_owned(), detail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::CatalogShelf;

    fn item(kind: CatalogItemKind, id: &str, title: &str, video_id: Option<&str>) -> CatalogItem {
        CatalogItem {
            kind,
            id: id.into(),
            title: title.into(),
            subtitle: String::new(),
            artist: None,
            artist_id: None,
            album: None,
            album_id: None,
            duration: None,
            thumbnail: None,
            video_id: video_id.map(Into::into),
            explicit: false,
        }
    }

    fn track(subtitle: &str, artist: &str, album: &str, duration: &str) -> Track {
        Track {
            id: "v".into(),
            title: "T".into(),
            subtitle: subtitle.into(),
            artist: artist.into(),
            artist_id: None,
            album: album.into(),
            album_id: None,
            duration: duration.into(),
            video_id: "v".into(),
            explicit: false,
            thumbnail: None,
        }
    }

    #[test]
    fn only_rows_with_a_video_id_become_tracks() {
        assert!(Track::from_catalog(&item(CatalogItemKind::Song, "abc", "Afterglow", Some("abc"))).is_some());
        assert!(Track::from_catalog(&item(CatalogItemKind::Album, "MPRE1", "Night Windows", None)).is_none());
    }

    #[test]
    fn an_empty_video_id_is_not_treated_as_playable() {
        assert!(Track::from_catalog(&item(CatalogItemKind::Song, "x", "Ghost", Some(""))).is_none());
    }

    #[test]
    fn card_actions_follow_the_row_kind() {
        let action = |kind, id: &str| Card::from_catalog(&item(kind, id, "t", None)).action;
        assert_eq!(
            action(CatalogItemKind::Album, "MPRE1"),
            Some(CardAction::Show(EntityReference::Album("MPRE1".into())))
        );
        assert_eq!(
            action(CatalogItemKind::Artist, "UC1"),
            Some(CardAction::Show(EntityReference::Artist("UC1".into())))
        );
        assert_eq!(
            action(CatalogItemKind::Playlist, "VLPL1"),
            Some(CardAction::Show(EntityReference::Playlist("VLPL1".into())))
        );
    }

    #[test]
    fn an_unknown_kind_stays_visible_but_inert() {
        let card = Card::from_catalog(&item(CatalogItemKind::Unknown, "?", "Something new", None));
        assert_eq!(card.action, None);
        assert_eq!(card.title, "Something new");
    }

    #[test]
    fn an_unknown_kind_from_a_newer_service_does_not_fail_the_page() {
        let wire = r#"{"kind":"podcast","id":"p","title":"A show","subtitle":"Podcast"}"#;
        let item: CatalogItem = serde_json::from_str(wire).expect("a new kind must still decode");
        assert_eq!(item.kind, CatalogItemKind::Unknown);
    }

    #[test]
    fn a_song_card_carries_its_playable_track() {
        let card = Card::from_catalog(&item(CatalogItemKind::Song, "abc", "Afterglow", Some("abc")));
        match card.action {
            Some(CardAction::Play(track)) => assert_eq!(track.video_id, "abc"),
            other => panic!("a song card should be playable, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_shelf_and_card_ids_are_disambiguated() {
        let repeated = item(CatalogItemKind::Album, "MPRE1", "Night Windows", None);
        let page = CatalogPage {
            id: "search:x".into(),
            title: "x".into(),
            shelves: vec![
                CatalogShelf {
                    id: "shelf".into(),
                    title: "One".into(),
                    items: vec![repeated.clone(), repeated.clone()],
                },
                CatalogShelf { id: "shelf".into(), title: "Two".into(), items: vec![repeated] },
            ],
            ..Default::default()
        };
        let view = PageView::from_wire(&page);
        assert_eq!(view.shelves.len(), 2);
        assert_ne!(view.shelves[0].id, view.shelves[1].id);
        assert_eq!(view.shelves[0].cards.len(), 1, "duplicate card ids collapse to the first");
    }

    #[test]
    fn a_shelf_of_songs_exposes_a_track_list_and_a_mixed_shelf_does_not() {
        let song = item(CatalogItemKind::Song, "a", "A", Some("a"));
        let other = item(CatalogItemKind::Song, "b", "B", Some("b"));
        let album = item(CatalogItemKind::Album, "MPRE1", "Album", None);
        let songs = Shelf {
            id: "s".into(),
            title: "Songs".into(),
            cards: [&song, &other].map(Card::from_catalog).to_vec(),
        };
        let mixed = Shelf {
            id: "m".into(),
            title: "Mixed".into(),
            cards: [&song, &album].map(Card::from_catalog).to_vec(),
        };
        assert_eq!(songs.track_list().map(|tracks| tracks.len()), Some(2));
        assert_eq!(mixed.track_list(), None);
    }

    #[test]
    fn a_page_view_keeps_only_playable_tracks() {
        let page = CatalogPage {
            id: "MPRE1".into(),
            title: "Night Windows".into(),
            subtitle: "Signal Fires".into(),
            tracks: vec![
                item(CatalogItemKind::Song, "a", "A", Some("a")),
                item(CatalogItemKind::Album, "MPRE2", "Not a track", None),
            ],
            truncated: true,
            ..Default::default()
        };
        let view = PageView::from_wire(&page);
        assert_eq!(view.tracks.len(), 1);
        assert!(view.truncated);
        assert!(!view.is_empty());
    }

    #[test]
    fn playable_tracks_span_tracks_and_song_shelves() {
        let page = CatalogPage {
            id: "artist".into(),
            title: "Signal Fires".into(),
            shelves: vec![CatalogShelf {
                id: "s".into(),
                title: "Songs".into(),
                items: vec![item(CatalogItemKind::Song, "b", "B", Some("b"))],
            }],
            tracks: vec![item(CatalogItemKind::Song, "a", "A", Some("a"))],
            ..Default::default()
        };
        let ids: Vec<String> =
            PageView::from_wire(&page).playable_tracks().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    /// A radio page is tracks-only, unlike the shelf pages the browse routes return.
    #[test]
    fn a_radio_page_decodes_into_playable_tracks_with_artwork() {
        let wire = r#"{"id":"radio:JhulBGMA7G4","title":"Radio","tracks":[
            {"kind":"song","id":"qXI87eMP-bs","title":"Face to Face","subtitle":"Daft Punk",
             "artist":"Daft Punk","duration":"4:01","videoId":"qXI87eMP-bs",
             "thumbnail":"https://yt3.googleusercontent.com/a"},
            {"kind":"song","id":"mllzzUjMezU","title":"Shooting Stars","subtitle":"Bag Raiders",
             "artist":"Bag Raiders","duration":"3:56","videoId":"mllzzUjMezU"}]}"#;
        let page = PageView::from_wire(&serde_json::from_str(wire).unwrap());
        assert_eq!(page.id, "radio:JhulBGMA7G4");
        assert!(page.shelves.is_empty());
        let ids: Vec<String> = page.playable_tracks().into_iter().map(|t| t.video_id).collect();
        assert_eq!(ids, ["qXI87eMP-bs", "mllzzUjMezU"]);
        assert_eq!(page.tracks[0].thumbnail.as_deref(), Some("https://yt3.googleusercontent.com/a"));
        assert_eq!(page.tracks[1].thumbnail, None, "artwork is optional on a queue row");
    }

    #[test]
    fn a_radio_page_that_came_back_empty_is_nothing_to_play() {
        let page = PageView::from_wire(&serde_json::from_str(r#"{"id":"radio:x","title":"Radio"}"#).unwrap());
        assert!(page.is_empty());
        assert!(page.playable_tracks().is_empty());
    }

    #[test]
    fn resolved_artist_and_album_win_over_the_upstream_descriptor() {
        let row = track(
            "Song • Signal Fires • Night Windows • 3:42",
            "Signal Fires",
            "Night Windows",
            "",
        );
        assert_eq!(row.secondary_text(), "Signal Fires · Night Windows");
    }

    #[test]
    fn the_descriptor_drops_what_the_row_already_shows() {
        // Upstream gives this shape when it resolves no artist link for the row.
        assert_eq!(track("Song • 5:21", "", "", "5:21").secondary_text(), "");
        assert_eq!(track("Video • 4:00", "", "", "4:00").secondary_text(), "");
    }

    #[test]
    fn unrecognized_descriptor_parts_survive() {
        assert_eq!(track("Song • 993K plays", "", "", "5:21").secondary_text(), "993K plays");
    }

    #[test]
    fn the_kind_is_not_repeated_when_upstream_already_leads_with_it() {
        assert_eq!(detail_subtitle("Playlist", "Playlist • 2026"), "Playlist • 2026");
        assert_eq!(detail_subtitle("Album", "album • Signal Fires"), "album • Signal Fires");
    }

    #[test]
    fn the_kind_is_added_when_upstream_omits_it() {
        assert_eq!(detail_subtitle("Album", "Signal Fires"), "Album · Signal Fires");
    }

    #[test]
    fn an_empty_subtitle_falls_back_to_the_kind() {
        assert_eq!(detail_subtitle("Artist", "   "), "Artist");
    }

    #[test]
    fn failure_text_explains_known_service_codes() {
        assert_eq!(failure_text("catalogEmpty", "", "“x”").title, "No results");
        assert_eq!(failure_text("catalogUnavailable", "", "home").title, "Catalog unreachable");
        assert_eq!(failure_text("somethingNew", "boom", "home").detail, "boom");
    }
}
