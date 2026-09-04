//! Lyrics command handling.
//!
//! Like the catalog, this is a read-only lookup: it cannot change who owns playback, and it is
//! dispatched before the authority.

use goosic_lyrics::LyricsClient;
use goosic_protocol::{ErrorObject, RequestPayload, ResponseEnvelope, ResponsePayload};

fn failure(request_id: String, code: &str, message: String) -> ResponseEnvelope {
    ResponseEnvelope::failure(
        request_id,
        ErrorObject {
            code: code.to_owned(),
            message,
        },
    )
}

/// Handles every `lyrics.*` command. Returns `None` when the command is not one of them.
pub fn handle(
    lyrics: &LyricsClient,
    command: &str,
    request_id: &str,
    payload: &RequestPayload,
) -> Option<ResponseEnvelope> {
    let id = request_id.to_owned();
    Some(match command {
        "lyrics.get" => {
            let Some(query) = payload.lyrics.as_ref() else {
                return Some(failure(
                    id,
                    "invalidRequest",
                    "lyrics.get requires a lyrics query".into(),
                ));
            };
            match lyrics.lookup(query) {
                Ok(document) => ResponseEnvelope::success(
                    id,
                    ResponsePayload {
                        lyrics: Some(document),
                        ..Default::default()
                    },
                ),
                Err(error) => failure(id, error.code(), error.to_string()),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::LyricsQuery;

    #[test]
    fn non_lyrics_commands_are_not_claimed() {
        let client = LyricsClient::new();
        assert!(handle(&client, "playback.claim", "1", &RequestPayload::default()).is_none());
        assert!(handle(&client, "catalog.search", "1", &RequestPayload::default()).is_none());
        assert!(handle(&client, "settings.get", "1", &RequestPayload::default()).is_none());
    }

    #[test]
    fn a_request_without_a_query_is_rejected_without_network_access() {
        let client = LyricsClient::new();
        let response = handle(&client, "lyrics.get", "1", &RequestPayload::default()).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "invalidRequest");
    }

    #[test]
    fn a_query_without_a_title_is_rejected_without_network_access() {
        let client = LyricsClient::new();
        let response = handle(
            &client,
            "lyrics.get",
            "1",
            &RequestPayload {
                lyrics: Some(LyricsQuery {
                    title: "   ".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "invalidRequest");
    }
}
