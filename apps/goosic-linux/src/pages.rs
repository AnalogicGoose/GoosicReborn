//! What the shell shows, decided without GTK.
//!
//! Which page is on screen, which catalog request it needs, and what its load state turns into row
//! by row need no widget, so they live here where a test can reach them. The GTK half only draws
//! the rows it is handed. The catalog conversions themselves come from `goosic-shell-support`, so
//! every shell turns a page into the same rows.

use std::collections::HashMap;

use goosic_protocol::{RequestPayload, ResponseEnvelope};
use goosic_shell_support::catalog::{detail_subtitle, failure_text, Card, PageView, Track};
use goosic_shell_support::navigation::{CatalogKey, EntityReference, Route, SearchFilter};
use goosic_shell_support::TransportError;

/// Where one catalog page stands.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadState {
    Idle,
    Loading,
    Loaded(PageView),
    Failed { code: String, message: String },
}

static IDLE: LoadState = LoadState::Idle;

/// One row of a screen, in the order it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum PageRow {
    Back,
    Header {
        title: String,
        subtitle: String,
    },
    /// Says plainly that the catalog is anonymous, so nobody reads a guest shelf as their own mix.
    GuestNotice,
    Loading {
        subject: String,
    },
    Failure {
        title: String,
        detail: String,
    },
    Empty {
        title: String,
        message: String,
    },
    NotLoaded {
        subject: String,
    },
    /// Boxed because a track is several times the size of every other row.
    Track(Box<Track>),
    ShelfTitle(String),
    Cards(Vec<Card>),
    /// The service clamped the page to fit one protocol frame, and the screen has to say so.
    Truncated,
}

/// The shell's navigation, and the catalog pages it has loaded.
///
/// Pages are kept per key, so a slow answer lands on its own key and is simply not shown if the
/// user has moved on, rather than replacing the screen they are looking at.
#[derive(Debug)]
pub struct Browser {
    route: Route,
    detail: Option<EntityReference>,
    submitted_query: String,
    filter: SearchFilter,
    pages: HashMap<CatalogKey, LoadState>,
}

impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}

impl Browser {
    pub fn new() -> Self {
        Self {
            route: Route::Home,
            detail: None,
            submitted_query: String::new(),
            filter: SearchFilter::All,
            pages: HashMap::new(),
        }
    }

    pub fn navigate(&mut self, route: Route) {
        self.route = route;
        self.detail = None;
    }

    pub fn open(&mut self, entity: EntityReference) {
        self.detail = Some(entity);
    }

    pub fn back(&mut self) {
        self.detail = None;
    }

    /// Records the query to search for. An empty one clears the results rather than searching
    /// for nothing, and returns false.
    pub fn submit_search(&mut self, text: &str) -> bool {
        self.submitted_query = text.trim().to_owned();
        !self.submitted_query.is_empty()
    }

    /// Each filter is its own upstream query, so switching tabs is a new page rather than a
    /// narrowing of the last one. Returns false when the filter did not change.
    pub fn select_filter(&mut self, filter: SearchFilter) -> bool {
        if self.filter == filter {
            return false;
        }
        self.filter = filter;
        true
    }

    /// The page the screen is showing, if it shows one.
    pub fn current_key(&self) -> Option<CatalogKey> {
        if let Some(entity) = &self.detail {
            return Some(CatalogKey::entity(entity));
        }
        match self.route {
            Route::Search => (!self.submitted_query.is_empty()).then(|| CatalogKey::Search {
                query: self.submitted_query.clone(),
                filter: self.filter,
            }),
            route => route.catalog_route().map(|_| CatalogKey::Route(route)),
        }
    }

    pub fn state(&self, key: &CatalogKey) -> &LoadState {
        self.pages.get(key).unwrap_or(&IDLE)
    }

    /// The request `key` needs, marking it as loading — or `None` when it is already loading or
    /// loaded and this is not a retry.
    pub fn begin(
        &mut self,
        key: &CatalogKey,
        force: bool,
    ) -> Option<(&'static str, RequestPayload)> {
        if !force && matches!(self.state(key), LoadState::Loading | LoadState::Loaded(_)) {
            return None;
        }
        let request = request_for(key)?;
        self.pages.insert(key.clone(), LoadState::Loading);
        Some(request)
    }

    /// Stores the service's answer for `key`, whichever page is on screen now.
    pub fn finish(&mut self, key: CatalogKey, result: Result<ResponseEnvelope, TransportError>) {
        let state = match result {
            Ok(response) => match response.payload.and_then(|payload| payload.catalog) {
                Some(page) => LoadState::Loaded(PageView::from_wire(&page)),
                None => LoadState::Failed {
                    code: "invalidResponse".to_owned(),
                    message: "The service answered without a catalog page.".to_owned(),
                },
            },
            Err(error) => {
                let (code, message) = error.describe();
                LoadState::Failed { code, message }
            }
        };
        self.pages.insert(key, state);
    }

    /// Marks `key` as unreachable because there is no service to ask.
    pub fn fail_offline(&mut self, key: CatalogKey) {
        self.pages.insert(
            key,
            LoadState::Failed {
                code: "offline".to_owned(),
                message: "Connect to the Rust service to load the catalog.".to_owned(),
            },
        );
    }

    pub fn shows_search_bar(&self) -> bool {
        self.detail.is_none() && self.route == Route::Search
    }

    /// Everything the screen draws, top to bottom.
    pub fn rows(&self) -> Vec<PageRow> {
        if let Some(entity) = &self.detail {
            return self.detail_rows(entity);
        }
        let title = route_title(self.route);
        let mut rows = vec![PageRow::Header {
            title: title.to_owned(),
            subtitle: route_subtitle(self.route).to_owned(),
        }];
        match self.route {
            Route::Search => match self.current_key() {
                Some(key) => rows.extend(body_rows(
                    self.state(&key),
                    &format!("“{}”", self.submitted_query),
                )),
                None => rows.push(PageRow::Empty {
                    title: "Start a search".to_owned(),
                    message: "Every result is a real YouTube Music entry.".to_owned(),
                }),
            },
            route if route.catalog_route().is_some() => {
                rows.push(PageRow::GuestNotice);
                rows.extend(body_rows(
                    self.state(&CatalogKey::Route(route)),
                    &title.to_lowercase(),
                ));
            }
            _ => rows.push(PageRow::Empty {
                title: "Not in the Linux shell yet".to_owned(),
                message: "This screen arrives in a later slice.".to_owned(),
            }),
        }
        rows
    }

    fn detail_rows(&self, entity: &EntityReference) -> Vec<PageRow> {
        let state = self.state(&CatalogKey::entity(entity));
        let kind = entity.kind_label();
        let header = match state {
            LoadState::Loaded(page) => PageRow::Header {
                title: page.title.clone(),
                subtitle: detail_subtitle(kind, &page.subtitle),
            },
            _ => PageRow::Header {
                title: kind.to_owned(),
                subtitle: "Loading from YouTube Music".to_owned(),
            },
        };
        let mut rows = vec![PageRow::Back, header];
        rows.extend(body_rows(state, &format!("this {}", kind.to_lowercase())));
        rows
    }
}

/// The command and payload that load `key`, or `None` for a route no catalog read can answer.
///
/// This is the same for every shell and belongs in `goosic-shell-support`; it lives here until the
/// next shared change carries it there.
pub fn request_for(key: &CatalogKey) -> Option<(&'static str, RequestPayload)> {
    let entity = |command, id: &String| {
        (
            command,
            RequestPayload {
                catalog_id: Some(id.clone()),
                ..Default::default()
            },
        )
    };
    Some(match key {
        // The service titles a browse page with `query`.
        CatalogKey::Route(route) => (
            "catalog.browse",
            RequestPayload {
                query: Some(route_title(*route).to_owned()),
                catalog_id: Some(route.catalog_route()?.to_owned()),
                ..Default::default()
            },
        ),
        CatalogKey::Search { query, filter } => (
            "catalog.search",
            RequestPayload {
                query: Some(query.clone()),
                filter: Some(filter.protocol_name().to_owned()),
                ..Default::default()
            },
        ),
        CatalogKey::Album(id) => entity("catalog.album", id),
        CatalogKey::Artist(id) => entity("catalog.artist", id),
        CatalogKey::Playlist(id) => entity("catalog.playlist", id),
    })
}

pub fn route_title(route: Route) -> &'static str {
    match route {
        Route::Home => "Home",
        Route::Explore => "Explore",
        Route::Search => "Search",
        Route::Library => "Library",
        Route::Charts => "Charts",
        Route::MoodsAndGenres => "Moods & genres",
        Route::NewReleases => "New releases",
        Route::Downloads => "Downloads",
        Route::Settings => "Settings",
    }
}

fn route_subtitle(route: Route) -> &'static str {
    match route {
        Route::Home => "Live from YouTube Music, browsed as a guest",
        Route::Explore => "New releases, charts, moods, and genres",
        Route::Search => "Search YouTube Music by title, artist, or album",
        Route::Library => "Your saved music",
        Route::Charts => "What is being played right now",
        Route::MoodsAndGenres => "Find a feeling, then a playlist",
        Route::NewReleases => "Albums and singles out now",
        Route::Downloads => "Finalized files from the previous Goosic",
        Route::Settings => "Preferences, accounts, and the service",
    }
}

/// A page's load state as rows. `subject` names the page in messages: "home", "this album".
fn body_rows(state: &LoadState, subject: &str) -> Vec<PageRow> {
    match state {
        LoadState::Idle => vec![PageRow::NotLoaded {
            subject: subject.to_owned(),
        }],
        LoadState::Loading => vec![PageRow::Loading {
            subject: subject.to_owned(),
        }],
        LoadState::Failed { code, message } => {
            let text = failure_text(code, message, subject);
            vec![PageRow::Failure {
                title: text.title,
                detail: text.detail,
            }]
        }
        LoadState::Loaded(page) if page.is_empty() => vec![PageRow::Empty {
            title: "Nothing to show".to_owned(),
            message: format!("{subject} came back empty."),
        }],
        LoadState::Loaded(page) => {
            let mut rows: Vec<PageRow> = page
                .tracks
                .iter()
                .map(|track| PageRow::Track(Box::new(track.clone())))
                .collect();
            for shelf in &page.shelves {
                rows.push(PageRow::ShelfTitle(shelf.title.clone()));
                // Songs read far better as rows than as artwork cards, so a shelf that holds only
                // songs is drawn as a track list.
                match shelf.track_list() {
                    Some(tracks) => {
                        rows.extend(tracks.into_iter().map(|t| PageRow::Track(Box::new(t))))
                    }
                    None => rows.push(PageRow::Cards(shelf.cards.clone())),
                }
            }
            if page.truncated {
                rows.push(PageRow::Truncated);
            }
            rows
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::{
        CatalogItem, CatalogItemKind, CatalogPage, CatalogShelf, ResponsePayload,
    };

    fn item(kind: CatalogItemKind, id: &str, video_id: Option<&str>) -> CatalogItem {
        CatalogItem {
            kind,
            id: id.into(),
            title: format!("Title {id}"),
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

    fn answer(page: CatalogPage) -> Result<ResponseEnvelope, TransportError> {
        Ok(ResponseEnvelope::success(
            "r",
            ResponsePayload {
                catalog: Some(page),
                ..Default::default()
            },
        ))
    }

    fn home_page() -> CatalogPage {
        CatalogPage {
            id: "home".into(),
            title: "Home".into(),
            shelves: vec![
                CatalogShelf {
                    id: "songs".into(),
                    title: "Quick picks".into(),
                    items: vec![item(CatalogItemKind::Song, "a", Some("a"))],
                },
                CatalogShelf {
                    id: "albums".into(),
                    title: "Albums".into(),
                    items: vec![item(CatalogItemKind::Album, "MPRE1", None)],
                },
            ],
            truncated: true,
            ..Default::default()
        }
    }

    fn loaded_home() -> Browser {
        let mut browser = Browser::new();
        let key = browser.current_key().unwrap();
        browser.begin(&key, false).unwrap();
        browser.finish(key, answer(home_page()));
        browser
    }

    #[test]
    fn home_asks_for_its_browse_route_under_its_title() {
        let mut browser = Browser::new();
        let key = browser.current_key().expect("home is a catalog page");
        let (command, payload) = browser.begin(&key, false).expect("nothing loaded yet");
        assert_eq!(command, "catalog.browse");
        assert_eq!(payload.catalog_id.as_deref(), Some("home"));
        assert_eq!(payload.query.as_deref(), Some("Home"));
    }

    #[test]
    fn a_page_already_loading_or_loaded_is_not_asked_for_twice_unless_retried() {
        let mut browser = Browser::new();
        let key = browser.current_key().unwrap();
        assert!(browser.begin(&key, false).is_some());
        assert!(browser.begin(&key, false).is_none(), "already loading");
        browser.finish(key.clone(), answer(home_page()));
        assert!(browser.begin(&key, false).is_none(), "already loaded");
        assert!(
            browser.begin(&key, true).is_some(),
            "a retry always asks again"
        );
    }

    #[test]
    fn a_loaded_page_draws_its_header_notice_shelves_and_truncation() {
        let rows = loaded_home().rows();
        assert!(matches!(&rows[0], PageRow::Header { title, .. } if title == "Home"));
        assert_eq!(rows[1], PageRow::GuestNotice);
        assert_eq!(rows[2], PageRow::ShelfTitle("Quick picks".into()));
        // A shelf of songs is drawn as rows, and a shelf of albums as cards.
        assert!(matches!(&rows[3], PageRow::Track(track) if track.video_id == "a"));
        assert_eq!(rows[4], PageRow::ShelfTitle("Albums".into()));
        assert!(matches!(&rows[5], PageRow::Cards(cards) if cards.len() == 1));
        assert_eq!(rows.last(), Some(&PageRow::Truncated));
    }

    #[test]
    fn a_failure_is_explained_rather_than_shown_as_a_code() {
        let mut browser = Browser::new();
        let key = browser.current_key().unwrap();
        browser.begin(&key, false);
        browser.finish(
            key,
            Err(TransportError::Remote {
                code: "catalogUnavailable".into(),
                message: "timed out upstream".into(),
            }),
        );
        let rows = browser.rows();
        assert!(
            matches!(&rows[2], PageRow::Failure { title, .. } if title == "Catalog unreachable")
        );
    }

    #[test]
    fn without_a_service_the_page_says_so() {
        let mut browser = Browser::new();
        let key = browser.current_key().unwrap();
        browser.fail_offline(key);
        let rows = browser.rows();
        assert!(
            matches!(&rows[2], PageRow::Failure { title, .. } if title == "Service not connected")
        );
    }

    #[test]
    fn search_waits_for_a_query_and_each_filter_is_its_own_page() {
        let mut browser = Browser::new();
        browser.navigate(Route::Search);
        assert!(browser.shows_search_bar());
        assert_eq!(browser.current_key(), None);
        assert!(
            matches!(&browser.rows()[1], PageRow::Empty { title, .. } if title == "Start a search")
        );

        assert!(!browser.submit_search("   "), "blank is not a query");
        assert!(browser.submit_search("  daft punk "));
        let all = browser.current_key().unwrap();
        let (command, payload) = request_for(&all).unwrap();
        assert_eq!(command, "catalog.search");
        assert_eq!(payload.query.as_deref(), Some("daft punk"));
        assert_eq!(payload.filter.as_deref(), Some("all"));

        assert!(browser.select_filter(SearchFilter::Songs));
        assert!(
            !browser.select_filter(SearchFilter::Songs),
            "the same tab again changes nothing"
        );
        assert_ne!(browser.current_key().unwrap(), all);
    }

    #[test]
    fn an_opened_entity_has_its_own_page_and_a_way_back() {
        let mut browser = loaded_home();
        browser.open(EntityReference::Album("MPRE1".into()));
        assert!(!browser.shows_search_bar());
        let key = browser.current_key().unwrap();
        assert_eq!(request_for(&key).unwrap().0, "catalog.album");
        let rows = browser.rows();
        assert_eq!(rows[0], PageRow::Back);
        assert!(matches!(&rows[1], PageRow::Header { title, .. } if title == "Album"));
        browser.back();
        assert_eq!(browser.current_key(), Some(CatalogKey::Route(Route::Home)));
    }

    #[test]
    fn an_answer_for_a_page_the_user_left_is_kept_but_not_drawn() {
        let mut browser = Browser::new();
        let home = browser.current_key().unwrap();
        browser.begin(&home, false);
        browser.navigate(Route::Charts);
        browser.finish(home.clone(), answer(home_page()));
        assert!(matches!(&browser.rows()[0], PageRow::Header { title, .. } if title == "Charts"));
        assert!(
            matches!(browser.state(&home), LoadState::Loaded(_)),
            "kept for coming back"
        );
    }

    #[test]
    fn screens_this_shell_does_not_have_yet_say_so() {
        let mut browser = Browser::new();
        browser.navigate(Route::Downloads);
        assert_eq!(browser.current_key(), None);
        assert!(
            matches!(&browser.rows()[1], PageRow::Empty { title, .. } if title == "Not in the Linux shell yet")
        );
    }
}
