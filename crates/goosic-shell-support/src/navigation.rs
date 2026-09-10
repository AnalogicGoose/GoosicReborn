//! Where a shell can be, and the names those places have on the wire.
//!
//! Labels and glyphs are not here: how a route is titled or drawn is presentation, and a shell
//! localises it. What is here is identity — the raw value a restored preference is matched
//! against, which routes a guest catalog read can answer, and the key a loaded page is cached
//! under so a slow answer can never land on a screen the user has left.

use crate::text::trim_spaces;

/// A top-level place in the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    Home,
    Explore,
    Search,
    Library,
    Charts,
    MoodsAndGenres,
    NewReleases,
    Downloads,
    Settings,
}

impl Route {
    pub const ALL: [Route; 9] = [
        Route::Home,
        Route::Explore,
        Route::Search,
        Route::Library,
        Route::Charts,
        Route::MoodsAndGenres,
        Route::NewReleases,
        Route::Downloads,
        Route::Settings,
    ];

    /// The value stored as `lastRoute`, so a restart reopens the same place.
    pub fn raw_value(self) -> &'static str {
        match self {
            Route::Home => "home",
            Route::Explore => "explore",
            Route::Search => "search",
            Route::Library => "library",
            Route::Charts => "charts",
            Route::MoodsAndGenres => "moodsAndGenres",
            Route::NewReleases => "newReleases",
            Route::Downloads => "downloads",
            Route::Settings => "settings",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Route> {
        Route::ALL.into_iter().find(|route| route.raw_value() == raw)
    }

    /// The `catalog.browse` route name, for routes backed by a live catalog surface.
    ///
    /// Search has its own command; Library, Downloads, and Settings are local surfaces that no
    /// guest catalog read can answer.
    pub fn catalog_route(self) -> Option<&'static str> {
        match self {
            Route::Home => Some("home"),
            Route::Explore => Some("explore"),
            Route::Charts => Some("charts"),
            Route::MoodsAndGenres => Some("moodsAndGenres"),
            Route::NewReleases => Some("newReleases"),
            Route::Search | Route::Library | Route::Downloads | Route::Settings => None,
        }
    }
}

/// A catalog entity a row can open.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityReference {
    Album(String),
    Artist(String),
    Playlist(String),
}

impl EntityReference {
    /// The word a detail page's subtitle leads with. Product copy rather than layout, and used by
    /// [`crate::catalog::detail_subtitle`] to avoid saying it twice.
    pub fn kind_label(&self) -> &'static str {
        match self {
            EntityReference::Album(_) => "Album",
            EntityReference::Artist(_) => "Artist",
            EntityReference::Playlist(_) => "Playlist",
        }
    }
}

/// The search filter tabs, and their protocol names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchFilter {
    All,
    Songs,
    Albums,
    Artists,
    Playlists,
    Videos,
}

impl SearchFilter {
    pub const ALL: [SearchFilter; 6] = [
        SearchFilter::All,
        SearchFilter::Songs,
        SearchFilter::Albums,
        SearchFilter::Artists,
        SearchFilter::Playlists,
        SearchFilter::Videos,
    ];

    /// The wire value understood by `catalog.search`.
    pub fn protocol_name(self) -> &'static str {
        match self {
            SearchFilter::All => "all",
            SearchFilter::Songs => "songs",
            SearchFilter::Albums => "albums",
            SearchFilter::Artists => "artists",
            SearchFilter::Playlists => "playlists",
            SearchFilter::Videos => "videos",
        }
    }
}

/// Identifies one catalog page the shell has asked for.
///
/// Pages are cached under this key, so a slow response can never land on the screen the user
/// has since navigated away from — it lands on its own key and is simply not displayed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CatalogKey {
    Route(Route),
    Search { query: String, filter: SearchFilter },
    Album(String),
    Artist(String),
    Playlist(String),
}

impl CatalogKey {
    pub fn entity(reference: &EntityReference) -> CatalogKey {
        match reference {
            EntityReference::Album(id) => CatalogKey::Album(id.clone()),
            EntityReference::Artist(id) => CatalogKey::Artist(id.clone()),
            EntityReference::Playlist(id) => CatalogKey::Playlist(id.clone()),
        }
    }
}

/// What happens when the queue reaches its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub const ALL: [RepeatMode; 3] = [RepeatMode::Off, RepeatMode::All, RepeatMode::One];

    /// The next mode in the cycle a single button steps through.
    pub fn next(self) -> RepeatMode {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    pub fn raw_value(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
    }

    pub fn from_raw(raw: &str) -> Option<RepeatMode> {
        RepeatMode::ALL.into_iter().find(|mode| mode.raw_value() == raw)
    }
}

/// The appearance the shell renders in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Follow whatever the operating system is set to.
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn raw_value(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    /// Decodes a stored value, falling back to following the system.
    ///
    /// The preference can come from a hand-edited file or from a previous Goosic install, so an
    /// unrecognized value is corrected rather than trusted.
    pub fn named(raw: &str) -> Theme {
        let normalized = trim_spaces(raw).to_lowercase();
        Theme::ALL
            .into_iter()
            .find(|theme| theme.raw_value() == normalized)
            .unwrap_or(Theme::System)
    }
}

/// A playback change the shell has asked for and not yet seen confirmed.
///
/// While one is in flight nothing else may start: a second claim racing the first is how two
/// renderers end up believing they own the speakers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaybackTransition {
    Idle,
    Claiming,
    PreparingLocal,
    Releasing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_catalog_backed_routes_resolve_to_a_browse_route() {
        assert_eq!(Route::Home.catalog_route(), Some("home"));
        assert_eq!(Route::MoodsAndGenres.catalog_route(), Some("moodsAndGenres"));
        assert_eq!(Route::Search.catalog_route(), None);
        assert_eq!(Route::Library.catalog_route(), None);
        assert_eq!(Route::Downloads.catalog_route(), None);
        assert_eq!(Route::Settings.catalog_route(), None);
    }

    #[test]
    fn every_route_raw_value_round_trips_so_a_restored_route_is_never_lost() {
        for route in Route::ALL {
            assert_eq!(Route::from_raw(route.raw_value()), Some(route));
        }
        assert_eq!(Route::from_raw("charts"), Some(Route::Charts));
        assert_eq!(Route::from_raw("nowhere"), None);
    }

    #[test]
    fn search_filters_map_to_protocol_names() {
        assert_eq!(SearchFilter::All.protocol_name(), "all");
        assert_eq!(SearchFilter::Songs.protocol_name(), "songs");
        assert_eq!(SearchFilter::Playlists.protocol_name(), "playlists");
    }

    #[test]
    fn entity_keys_are_distinct_per_kind() {
        let album = CatalogKey::entity(&EntityReference::Album("x".into()));
        assert_ne!(album, CatalogKey::entity(&EntityReference::Artist("x".into())));
        assert_ne!(album, CatalogKey::entity(&EntityReference::Playlist("x".into())));
    }

    #[test]
    fn search_keys_separate_filters() {
        assert_ne!(
            CatalogKey::Search { query: "a".into(), filter: SearchFilter::All },
            CatalogKey::Search { query: "a".into(), filter: SearchFilter::Songs }
        );
    }

    #[test]
    fn the_repeat_cycle_visits_every_mode_and_returns() {
        assert_eq!(RepeatMode::Off.next(), RepeatMode::All);
        assert_eq!(RepeatMode::All.next(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next(), RepeatMode::Off);
        for mode in RepeatMode::ALL {
            assert_eq!(RepeatMode::from_raw(mode.raw_value()), Some(mode));
        }
    }

    #[test]
    fn stored_theme_values_decode_after_normalization() {
        assert_eq!(Theme::named("system"), Theme::System);
        assert_eq!(Theme::named("light"), Theme::Light);
        assert_eq!(Theme::named("dark"), Theme::Dark);
        // The value can come from a hand-edited file or a previous Goosic install.
        assert_eq!(Theme::named("  Dark  "), Theme::Dark);
        assert_eq!(Theme::named("LIGHT"), Theme::Light);
    }

    #[test]
    fn an_unrecognized_theme_falls_back_to_following_the_system() {
        assert_eq!(Theme::named("neon"), Theme::System);
        assert_eq!(Theme::named(""), Theme::System);
        for theme in Theme::ALL {
            assert_eq!(Theme::named(theme.raw_value()), theme);
        }
    }
}
