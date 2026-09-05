//! Read-only YouTube Music catalog access for the Goosic authority.
//!
//! The crate has two layers on purpose: `parse` is pure and fully unit-tested, and `client` is
//! the only part that performs network I/O. Neither layer touches playback ownership; the
//! catalog answers "what exists" and never "who may play".

mod client;
mod json;
mod parse;

pub use client::{search_params, InnertubeClient};
pub use parse::{
    artist_page, browse_page, browse_shelves, queue_item, radio_page, search_page,
    track_list_page,
};

use goosic_protocol::CatalogPage;
use thiserror::Error;

/// Browse ids for the surfaces the shell exposes as top-level routes.
pub const HOME_BROWSE_ID: &str = "FEmusic_home";
pub const EXPLORE_BROWSE_ID: &str = "FEmusic_explore";
pub const CHARTS_BROWSE_ID: &str = "FEmusic_charts";
pub const MOODS_BROWSE_ID: &str = "FEmusic_moods_and_genres";
pub const NEW_RELEASES_BROWSE_ID: &str = "FEmusic_new_releases";

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("catalog request failed: {0}")]
    Network(String),
    #[error("catalog upstream returned HTTP {0}")]
    Upstream(u16),
    #[error("catalog response could not be decoded: {0}")]
    Decode(String),
    #[error("catalog response contained no usable rows")]
    Empty,
}

impl CatalogError {
    /// Stable wire codes so the shell can distinguish "ask again" from "fix the request".
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalidRequest",
            Self::Network(_) => "catalogUnavailable",
            Self::Upstream(_) => "catalogUpstreamError",
            Self::Decode(_) => "catalogDecodeError",
            Self::Empty => "catalogEmpty",
        }
    }
}

/// The catalog surfaces the service exposes, resolved from a route name.
pub fn browse_id_for_route(route: &str) -> Option<&'static str> {
    match route {
        "home" => Some(HOME_BROWSE_ID),
        "explore" => Some(EXPLORE_BROWSE_ID),
        "charts" => Some(CHARTS_BROWSE_ID),
        "moods" | "moodsAndGenres" => Some(MOODS_BROWSE_ID),
        "newReleases" => Some(NEW_RELEASES_BROWSE_ID),
        _ => None,
    }
}

/// A playlist's browse id is its playlist id with the `VL` browse prefix.
pub fn playlist_browse_id(id: &str) -> String {
    if id.starts_with("VL") {
        id.to_owned()
    } else {
        format!("VL{id}")
    }
}

/// Live catalog reads. Held by the service for its process lifetime.
pub struct Catalog {
    client: InnertubeClient,
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new()
    }
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            client: InnertubeClient::new(),
        }
    }

    pub fn search(&self, query: &str, filter: &str) -> Result<CatalogPage, CatalogError> {
        let response = self.client.search(query, filter)?;
        let page = parse::search_page(query, &response);
        if page.shelves.is_empty() {
            return Err(CatalogError::Empty);
        }
        Ok(page)
    }

    pub fn browse_route(&self, route: &str, title: &str) -> Result<CatalogPage, CatalogError> {
        let browse_id = browse_id_for_route(route).ok_or_else(|| {
            CatalogError::InvalidRequest(format!("`{route}` is not a catalog route"))
        })?;
        let response = self.client.browse(browse_id)?;
        let page = parse::browse_page(browse_id, title, &response);
        if page.shelves.is_empty() {
            return Err(CatalogError::Empty);
        }
        Ok(page)
    }

    pub fn album(&self, browse_id: &str) -> Result<CatalogPage, CatalogError> {
        let response = self.client.browse(browse_id)?;
        let page = parse::track_list_page(browse_id, &response);
        if page.tracks.is_empty() {
            return Err(CatalogError::Empty);
        }
        Ok(page)
    }

    pub fn playlist(&self, id: &str) -> Result<CatalogPage, CatalogError> {
        let browse_id = playlist_browse_id(id);
        let response = self.client.browse(&browse_id)?;
        let mut page = parse::track_list_page(&browse_id, &response);
        if page.tracks.is_empty() {
            return Err(CatalogError::Empty);
        }
        page.id = browse_id;
        Ok(page)
    }

    /// The queue that follows a track, for continuing playback when a list runs out.
    pub fn radio(&self, video_id: &str) -> Result<CatalogPage, CatalogError> {
        let response = self.client.radio(video_id)?;
        let page = parse::radio_page(video_id, &response);
        if page.tracks.is_empty() {
            return Err(CatalogError::Empty);
        }
        Ok(page)
    }

    pub fn artist(&self, browse_id: &str) -> Result<CatalogPage, CatalogError> {
        let response = self.client.browse(browse_id)?;
        let page = parse::artist_page(browse_id, &response);
        if page.tracks.is_empty() && page.shelves.is_empty() {
            return Err(CatalogError::Empty);
        }
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_resolve_to_the_documented_browse_ids() {
        assert_eq!(browse_id_for_route("home"), Some(HOME_BROWSE_ID));
        assert_eq!(browse_id_for_route("charts"), Some(CHARTS_BROWSE_ID));
        assert_eq!(browse_id_for_route("moodsAndGenres"), Some(MOODS_BROWSE_ID));
        assert_eq!(browse_id_for_route("search"), None);
        assert_eq!(browse_id_for_route("library"), None);
    }

    #[test]
    fn playlist_browse_prefix_is_applied_once() {
        assert_eq!(playlist_browse_id("PL123"), "VLPL123");
        assert_eq!(playlist_browse_id("VLPL123"), "VLPL123");
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(CatalogError::Empty.code(), "catalogEmpty");
        assert_eq!(CatalogError::Upstream(503).code(), "catalogUpstreamError");
        assert_eq!(
            CatalogError::Network("boom".into()).code(),
            "catalogUnavailable"
        );
    }
}
