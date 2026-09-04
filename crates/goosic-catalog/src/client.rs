//! Guest-only InnerTube transport.
//!
//! This client is deliberately unauthenticated: it sends no cookies, no `Authorization`, and no
//! account headers, so catalog browsing can never leak credentials onto the service protocol.
//! Signed-in surfaces (a real library) require the official web view, not this client.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::CatalogError;

const ORIGIN: &str = "https://music.youtube.com";
/// The public YouTube Music web client identity. `hl=en` keeps subtitle tokens parseable.
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_NAME_ID: &str = "67";
const CLIENT_VERSION: &str = "1.20260510.02.00";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Upstream responses are large but bounded; anything past this is a sign of a wrong endpoint.
const MAX_RESPONSE_BYTES: u64 = 12 * 1024 * 1024;
/// Kept below the shell's per-request wait so a slow upstream surfaces as a catalog error
/// rather than as a client-side timeout that invalidates the service process.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// Search filter page parameters used by the YouTube Music web client.
pub fn search_params(filter: &str) -> Result<Option<&'static str>, CatalogError> {
    Ok(match filter {
        "all" | "" => None,
        "songs" => Some("EgWKAQIIAWoQEAkQBRAKEAMQBBAQEBUQEQ=="),
        "videos" => Some("EgWKAQIQAWoQEAkQBRAKEAMQBBAQEBUQEQ=="),
        "albums" => Some("EgWKAQIYAWoQEAkQBRAKEAMQBBAQEBUQEQ=="),
        "artists" => Some("EgWKAQIgAWoQEAkQBRAKEAMQBBAQEBUQEQ=="),
        "playlists" => Some("EgWKAQIoAWoQEAkQBRAKEAMQBBAQEBUQEQ=="),
        other => {
            return Err(CatalogError::InvalidRequest(format!(
                "unknown search filter `{other}`"
            )))
        }
    })
}

/// Performs InnerTube POSTs and remembers the visitor identity upstream hands back.
pub struct InnertubeClient {
    agent: ureq::Agent,
    /// `visitorData` is an anonymous session token, not an account credential. Echoing it back
    /// keeps results stable across requests; it is never written to stdout or persisted.
    visitor_data: Mutex<Option<String>>,
}

impl Default for InnertubeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl InnertubeClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        Self {
            agent,
            visitor_data: Mutex::new(None),
        }
    }

    fn context(&self) -> Value {
        let mut client = json!({
            "clientName": CLIENT_NAME,
            "clientVersion": CLIENT_VERSION,
            "hl": "en",
            "gl": "US",
            "platform": "DESKTOP",
            "originalUrl": "https://music.youtube.com/",
        });
        if let Some(visitor) = self.visitor_data.lock().ok().and_then(|slot| slot.clone()) {
            client["visitorData"] = Value::String(visitor);
        }
        json!({
            "client": client,
            "user": {"lockedSafetyMode": false},
            "request": {"useSsl": true},
        })
    }

    fn capture_visitor_data(&self, response: &Value) {
        let Some(visitor) = response
            .get("responseContext")
            .and_then(|context| context.get("visitorData"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if visitor.is_empty() {
            return;
        }
        if let Ok(mut slot) = self.visitor_data.lock() {
            *slot = Some(visitor.to_owned());
        }
    }

    fn post(&self, endpoint: &str, mut body: Value) -> Result<Value, CatalogError> {
        body["context"] = self.context();
        let url = format!("{ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false");
        let mut response = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "*/*")
            .header("accept-language", "en-US,en;q=0.9")
            .header("origin", ORIGIN)
            .header("referer", "https://music.youtube.com/")
            .header("x-origin", ORIGIN)
            .header("x-youtube-client-name", CLIENT_NAME_ID)
            .header("x-youtube-client-version", CLIENT_VERSION)
            .send_json(&body)
            .map_err(|error| match error {
                ureq::Error::StatusCode(status) => CatalogError::Upstream(status),
                other => CatalogError::Network(other.to_string()),
            })?;

        let value: Value = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_json()
            .map_err(|error| CatalogError::Decode(error.to_string()))?;
        self.capture_visitor_data(&value);
        Ok(value)
    }

    pub fn search(&self, query: &str, filter: &str) -> Result<Value, CatalogError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(CatalogError::InvalidRequest("search query is empty".into()));
        }
        let mut body = json!({"query": query});
        if let Some(params) = search_params(filter)? {
            body["params"] = Value::String(params.to_owned());
        }
        self.post("search", body)
    }

    pub fn browse(&self, browse_id: &str) -> Result<Value, CatalogError> {
        if browse_id.trim().is_empty() {
            return Err(CatalogError::InvalidRequest("browse id is empty".into()));
        }
        self.post("browse", json!({"browseId": browse_id}))
    }

    /// Asks for the queue that follows a track.
    ///
    /// `RDAMVM<videoId>` is the radio playlist the web client uses for "start radio from this
    /// song"; it is a well-known id shape, not a value from a previous response.
    pub fn radio(&self, video_id: &str) -> Result<Value, CatalogError> {
        let video_id = video_id.trim();
        if video_id.is_empty() {
            return Err(CatalogError::InvalidRequest("video id is empty".into()));
        }
        self.post(
            "next",
            json!({
                "videoId": video_id,
                "playlistId": format!("RDAMVM{video_id}"),
                "isAudioOnly": true,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_filters_map_to_params_and_all_is_unfiltered() {
        assert!(search_params("all").unwrap().is_none());
        assert!(search_params("").unwrap().is_none());
        assert!(search_params("songs").unwrap().is_some());
        assert!(search_params("artists").unwrap().is_some());
    }

    #[test]
    fn unknown_filter_is_rejected_rather_than_silently_ignored() {
        let error = search_params("podcasts").unwrap_err();
        assert!(matches!(error, CatalogError::InvalidRequest(_)));
    }

    #[test]
    fn radio_requires_a_video_id_before_any_network_call() {
        let client = InnertubeClient::new();
        let error = client.radio("  ").map(|_| ()).unwrap_err();
        assert!(matches!(error, CatalogError::InvalidRequest(_)));
    }

    #[test]
    fn empty_query_is_rejected_before_any_network_call() {
        let client = InnertubeClient::new();
        let error = client.search("   ", "all").unwrap_err();
        assert!(matches!(error, CatalogError::InvalidRequest(_)));
    }

    #[test]
    fn visitor_data_is_captured_and_echoed_in_the_next_context() {
        let client = InnertubeClient::new();
        assert!(client.context()["client"].get("visitorData").is_none());
        client.capture_visitor_data(&serde_json::json!({
            "responseContext": {"visitorData": "Cgt0ZXN0"}
        }));
        assert_eq!(client.context()["client"]["visitorData"], "Cgt0ZXN0");
    }

    #[test]
    fn context_never_carries_account_or_credential_fields() {
        let client = InnertubeClient::new();
        let wire = serde_json::to_string(&client.context()).unwrap();
        for forbidden in ["cookie", "Authorization", "SAPISID", "pageId", "authUser"] {
            assert!(
                !wire.contains(forbidden),
                "context leaked `{forbidden}`: {wire}"
            );
        }
    }
}
