pub mod catalog;

use goosic_catalog::Catalog;
use goosic_core::{CoreError, PlaybackAuthority};
use goosic_protocol::{
    ErrorObject, Owner, RequestEnvelope, ResponseEnvelope, ResponsePayload, PROTOCOL_VERSION,
};

pub fn handle_request(
    authority: &mut PlaybackAuthority,
    catalog: &Catalog,
    request: RequestEnvelope,
) -> ResponseEnvelope {
    let request_id = request.request_id;
    if request.protocol_version != PROTOCOL_VERSION {
        return failure(
            request_id,
            "unsupportedProtocolVersion",
            format!("expected protocol version {PROTOCOL_VERSION}"),
        );
    }

    let payload = request.payload;
    // Catalog reads answer "what exists" and never touch the playback authority, so they are
    // dispatched before it and cannot alter ownership, generation, or sequence.
    if let Some(response) = catalog::handle(catalog, &request.command, &request_id, &payload) {
        return response;
    }

    let state = match request.command.as_str() {
        "hello" | "handshake" => {
            return ResponseEnvelope::success(
                request_id,
                ResponsePayload {
                    message: Some("goosic-service ready".into()),
                    state: Some(authority.state().clone()),
                    ..Default::default()
                },
            )
        }
        "state.get" => authority.state().clone(),
        "account.change" => {
            let generation = match payload.generation {
                Some(generation) => generation,
                None => {
                    return failure(
                        request_id,
                        "invalidRequest",
                        "account change requires generation",
                    )
                }
            };
            return core_result(
                request_id,
                authority.change_account(payload.account_id, generation),
            );
        }
        "playback.claim" => {
            let owner = match payload.owner {
                Some(owner) if owner != Owner::None => owner,
                _ => return failure(request_id, "invalidRequest", "claim requires an owner"),
            };
            let generation = match payload.generation {
                Some(generation) => generation,
                None => return failure(request_id, "invalidRequest", "claim requires generation"),
            };
            return core_result(request_id, authority.claim(owner, generation));
        }
        "playback.release" => {
            let owner = match payload.owner {
                Some(owner) => owner,
                None => return failure(request_id, "invalidRequest", "release requires an owner"),
            };
            let generation = match payload.generation {
                Some(generation) => generation,
                None => {
                    return failure(request_id, "invalidRequest", "release requires generation")
                }
            };
            return core_result(request_id, authority.release(owner, generation));
        }
        "playback.sample" => {
            let owner = match payload.owner {
                Some(owner) => owner,
                None => return failure(request_id, "invalidRequest", "sample requires an owner"),
            };
            let generation = match payload.generation {
                Some(generation) => generation,
                None => return failure(request_id, "invalidRequest", "sample requires generation"),
            };
            let sequence = match payload.sequence {
                Some(sequence) => sequence,
                None => return failure(request_id, "invalidRequest", "sample requires sequence"),
            };
            let marker = payload.marker.as_deref();
            return match authority.sample(owner, generation, sequence, marker) {
                Ok(state) => ResponseEnvelope::success(
                    request_id,
                    ResponsePayload {
                        state: Some(state),
                        sequence: Some(sequence),
                        marker_accepted: Some(marker.is_some()),
                        ..Default::default()
                    },
                ),
                Err(error) => core_failure(request_id, error),
            };
        }
        _ => return failure(request_id, "unknownCommand", "command is not supported"),
    };

    ResponseEnvelope::success(
        request_id,
        ResponsePayload {
            state: Some(state),
            ..Default::default()
        },
    )
}

fn core_result(
    request_id: String,
    result: Result<goosic_protocol::PlaybackState, CoreError>,
) -> ResponseEnvelope {
    match result {
        Ok(state) => ResponseEnvelope::success(
            request_id,
            ResponsePayload {
                state: Some(state.clone()),
                owner: Some(state.owner),
                generation: Some(state.generation),
                ..Default::default()
            },
        ),
        Err(error) => core_failure(request_id, error),
    }
}

fn core_failure(request_id: String, error: CoreError) -> ResponseEnvelope {
    failure(request_id, error.code(), error.to_string())
}

fn failure(
    request_id: String,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ResponseEnvelope {
    ResponseEnvelope::failure(
        request_id,
        ErrorObject {
            code: code.into(),
            message: message.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::{RequestPayload, PROTOCOL_VERSION};

    fn catalog() -> Catalog {
        Catalog::new()
    }

    fn request(id: &str, command: &str, payload: RequestPayload) -> RequestEnvelope {
        RequestEnvelope {
            protocol_version: PROTOCOL_VERSION.into(),
            request_id: id.into(),
            command: command.into(),
            payload,
        }
    }

    #[test]
    fn handshake_and_state_are_json_round_trippable() {
        let mut authority = PlaybackAuthority::new();
        let response = handle_request(
            &mut authority,
            &catalog(),
            request("1", "hello", RequestPayload::default()),
        );
        assert!(response.ok);
        let wire = serde_json::to_string(&response).unwrap();
        assert!(wire.contains("\"protocolVersion\":\"0.2.0\""));
        let decoded: ResponseEnvelope = serde_json::from_str(&wire).unwrap();
        assert_eq!(decoded.request_id, "1");
    }

    #[test]
    fn advertisement_marker_is_successful_and_does_not_release() {
        let mut authority = PlaybackAuthority::new();
        let claimed = handle_request(
            &mut authority,
            &catalog(),
            request(
                "1",
                "playback.claim",
                RequestPayload {
                    owner: Some(Owner::OfficialWebView),
                    generation: Some(0),
                    ..Default::default()
                },
            ),
        );
        let generation = claimed.payload.unwrap().generation.unwrap();
        let marked = handle_request(
            &mut authority,
            &catalog(),
            request(
                "2",
                "playback.sample",
                RequestPayload {
                    owner: Some(Owner::OfficialWebView),
                    generation: Some(generation),
                    sequence: Some(1),
                    marker: Some("advertisement".into()),
                    ..Default::default()
                },
            ),
        );
        assert!(marked.ok);
        assert_eq!(marked.payload.unwrap().marker_accepted, Some(true));
        assert_eq!(authority.state().owner, Owner::OfficialWebView);
    }

    #[test]
    fn account_change_rejects_stale_generation() {
        let mut authority = PlaybackAuthority::new();
        let response = handle_request(
            &mut authority,
            &catalog(),
            request(
                "1",
                "account.change",
                RequestPayload {
                    account_id: Some("account-2".into()),
                    generation: Some(1),
                    ..Default::default()
                },
            ),
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "generationMismatch");
    }
}
