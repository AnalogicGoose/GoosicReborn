//! Versioned, newline-delimited JSON messages shared by the Rust service and Swift UI.
//!
//! Fields use lower camel case so the same wire representation can be decoded directly by
//! Swift's `Codable`. Unknown fields should be ignored by clients to permit additive evolution.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "0.2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Owner {
    None,
    OfficialWebView,
    LocalDownloadedFile,
}

impl Owner {
    pub fn is_claimable(self) -> bool {
        !matches!(self, Self::None)
    }

    /// The only online playback authority in this protocol is the official web view.
    pub fn is_online(self) -> bool {
        matches!(self, Self::OfficialWebView)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// `audio` or `advertisement`; markers are informational and never teardown playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Free-text catalog search terms. Only meaningful for `catalog.search`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Catalog search filter tab; `all`, `songs`, `videos`, `albums`, `artists`, `playlists`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Catalog entity identifier: a browse id, playlist id, or video id depending on command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    /// Caller-requested result cap. The service clamps this to its own frame-safe maximum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub protocol_version: String,
    pub request_id: String,
    pub command: String,
    #[serde(default)]
    pub payload: RequestPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub account_id: Option<String>,
    pub owner: Owner,
    pub generation: u64,
    pub sample_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<PlaybackState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_accepted: Option<bool>,
    /// Present only for `catalog.*` responses. Catalog data never carries credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<CatalogPage>,
}

/// What a catalog row is, and therefore where the shell may navigate or what it may play.
///
/// Only `song` and `video` carry a `videoId`; everything else is a browse destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogItemKind {
    Song,
    Video,
    Album,
    Artist,
    Playlist,
}

/// One normalized catalog row.
///
/// This is deliberately flat and small: every response must fit the service's NDJSON frame
/// budget, so raw upstream JSON never crosses this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub kind: CatalogItemKind,
    /// Navigation identity: a browse id, playlist id, or video id depending on `kind`.
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// The official-player identifier. `None` means this row is not directly playable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub explicit: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogShelf {
    pub id: String,
    pub title: String,
    pub items: Vec<CatalogItem>,
}

/// A whole catalog screen: shelves for browse surfaces, `tracks` for ordered track lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shelves: Vec<CatalogShelf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<CatalogItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// True when the service clamped the upstream result set to stay inside the frame budget.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorObject {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub protocol_version: String,
    pub request_id: String,
    pub ok: bool,
    pub payload: Option<ResponsePayload>,
    pub error: Option<ErrorObject>,
}

impl ResponseEnvelope {
    pub fn success(request_id: impl Into<String>, payload: ResponsePayload) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            request_id: request_id.into(),
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn failure(request_id: impl Into<String>, error: ErrorObject) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            request_id: request_id.into(),
            ok: false,
            payload: None,
            error: Some(error),
        }
    }
}

/// Notifications are reserved for a later async service channel. They intentionally share the
/// same version and state fields as responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub protocol_version: String,
    pub event: String,
    pub state: PlaybackState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_wire_snapshot_is_stable() {
        let request = RequestEnvelope {
            protocol_version: PROTOCOL_VERSION.into(),
            request_id: "r-1".into(),
            command: "playback.sample".into(),
            payload: RequestPayload {
                owner: Some(Owner::OfficialWebView),
                generation: Some(3),
                sequence: Some(8),
                account_id: None,
                marker: Some("advertisement".into()),
                // Listed explicitly: a new payload field must force a look at this snapshot.
                query: None,
                filter: None,
                catalog_id: None,
                limit: None,
            },
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"protocolVersion":"0.2.0","requestId":"r-1","command":"playback.sample","payload":{"owner":"officialWebView","generation":3,"sequence":8,"marker":"advertisement"}}"#
        );
        let decoded: RequestEnvelope =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn response_round_trip_preserves_error_code() {
        let response = ResponseEnvelope::failure(
            "r-2",
            ErrorObject {
                code: "generationMismatch".into(),
                message: "stale lease".into(),
            },
        );
        let wire = serde_json::to_string(&response).unwrap();
        let decoded: ResponseEnvelope = serde_json::from_str(&wire).unwrap();
        assert_eq!(decoded, response);
        assert_eq!(decoded.error.unwrap().code, "generationMismatch");
    }
}
