//! Deciding whether a frame is a response worth routing, and what it says.
//!
//! A line arriving on the pipe is not yet an answer. It has to parse and it has to speak the
//! protocol version this shell was built against; a frame that fails either means the stream can
//! no longer be trusted. What it does *not* have to do is answer the most recent question. The
//! service answers catalog and lyrics reads off its main loop, so responses arrive in whatever
//! order their work finished, and matching them to requests is the client's job, by id.

use goosic_protocol::{ErrorObject, ResponseEnvelope, PROTOCOL_VERSION};

use crate::TransportError;

/// Decodes one frame into a response, or into the reason the stream is unusable.
///
/// A response reporting `ok: false` is a well-formed frame and is returned, not refused: whether
/// the service agreed is a question about one request, and this function answers a question about
/// the stream. [`into_result`] is the step that turns a refusal into an error.
pub fn decode_response(frame: &[u8]) -> Result<ResponseEnvelope, TransportError> {
    let response: ResponseEnvelope =
        serde_json::from_slice(frame).map_err(|_| TransportError::InvalidResponse)?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(TransportError::ProtocolVersionMismatch {
            expected: PROTOCOL_VERSION.to_owned(),
            actual: response.protocol_version,
        });
    }
    Ok(response)
}

/// The answer to one request: the response itself, or the service's refusal as an error.
pub fn into_result(response: ResponseEnvelope) -> Result<ResponseEnvelope, TransportError> {
    if response.ok {
        return Ok(response);
    }
    // A refusal is required to say why, but a client that trusts that has nothing to report when
    // the service is itself the broken thing. Naming the gap beats unwrapping into it.
    let error = response.error.unwrap_or_else(|| ErrorObject {
        code: "serviceFailure".to_owned(),
        message: "unknown service error".to_owned(),
    });
    Err(TransportError::Remote { code: error.code, message: error.message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::conformance::{encoding_cases, Envelope};

    fn response_fixture(name: &str) -> String {
        encoding_cases()
            .into_iter()
            .find(|case| case.name == name && case.envelope == Envelope::Response)
            .unwrap_or_else(|| panic!("no response fixture named {name}"))
            .wire
    }

    #[test]
    fn a_success_decodes_and_is_the_answer() {
        let wire = response_fixture("response/state");
        let response = decode_response(wire.as_bytes()).expect("fixture was refused");
        assert_eq!(response.request_id, "r-1");
        assert!(into_result(response).is_ok());
    }

    #[test]
    fn a_refusal_is_a_frame_rather_than_a_broken_stream() {
        let wire = response_fixture("response/owner-conflict");
        let response = decode_response(wire.as_bytes()).expect("a refusal is a valid frame");
        assert!(!response.ok);
        let error = into_result(response).expect_err("a refusal is not success");
        assert_eq!(
            error,
            TransportError::Remote {
                code: "ownerConflict".into(),
                message: "another owner holds the lease".into(),
            }
        );
        assert!(!error.invalidates_connection());
    }

    #[test]
    fn a_refusal_with_no_error_object_still_names_a_failure() {
        let wire = concat!(
            r#"{"protocolVersion":"0.3.0","requestId":"r-4","#,
            r#""ok":false,"payload":null,"error":null}"#
        );
        let response = decode_response(wire.as_bytes()).expect("still a valid frame");
        assert_eq!(
            into_result(response),
            Err(TransportError::Remote {
                code: "serviceFailure".into(),
                message: "unknown service error".into(),
            })
        );
    }

    #[test]
    fn a_frame_that_is_not_json_ends_the_stream() {
        let error = decode_response(b"not json").expect_err("garbage is not a response");
        assert_eq!(error, TransportError::InvalidResponse);
        assert!(error.invalidates_connection());
    }

    #[test]
    fn a_response_from_a_different_protocol_version_ends_the_stream() {
        let wire = concat!(
            r#"{"protocolVersion":"0.4.0","requestId":"r-1","#,
            r#""ok":true,"payload":{},"error":null}"#
        );
        let error = decode_response(wire.as_bytes()).expect_err("a newer wire is not ours");
        assert_eq!(
            error,
            TransportError::ProtocolVersionMismatch {
                expected: PROTOCOL_VERSION.to_owned(),
                actual: "0.4.0".to_owned(),
            }
        );
        assert!(error.invalidates_connection());
    }

    /// The version is checked before `ok` is looked at, so a foreign service's refusal is reported
    /// as the incompatibility it is rather than as this request's failure.
    #[test]
    fn a_refusal_in_a_foreign_version_is_an_incompatibility_not_a_refusal() {
        let wire = concat!(
            r#"{"protocolVersion":"0.4.0","requestId":"r-9","ok":false,"payload":null,"#,
            r#""error":{"code":"ownerConflict","message":"another owner holds the lease"}}"#
        );
        let error = decode_response(wire.as_bytes()).expect_err("wrong version");
        assert!(matches!(error, TransportError::ProtocolVersionMismatch { .. }));
    }

    /// Request ids are the client's business, not the decoder's: a response to a question asked
    /// earlier is exactly what an out-of-order service produces.
    #[test]
    fn any_request_id_decodes() {
        let wire = response_fixture("response/state");
        assert!(decode_response(wire.as_bytes()).is_ok());
    }
}
