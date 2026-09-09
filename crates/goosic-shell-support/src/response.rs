//! Deciding whether a frame is the answer to the question that was asked.
//!
//! A response arriving on the pipe is not yet an answer. It has to parse, it has to speak the
//! protocol version this shell was built against, and it has to carry the request id that was
//! sent — a frame that fails any of those means the stream is no longer in step, and pairing the
//! next answer with the following question would be worse than failing here. Only the last check,
//! `ok`, is the service disagreeing rather than the channel breaking.

use goosic_protocol::{ErrorObject, ResponseEnvelope, PROTOCOL_VERSION};

use crate::TransportError;

/// Turns one received frame into a response the caller may believe, or into the reason it cannot.
///
/// The order of the checks is load-bearing. Version is compared before the request id so that a
/// service from a different build reports the mismatch it actually has rather than an id
/// confusion, and both are compared before `ok` so that an error object belonging to another
/// conversation is never surfaced as this call's failure.
pub fn accept_response(
    expected_request_id: &str,
    frame: &[u8],
) -> Result<ResponseEnvelope, TransportError> {
    let response: ResponseEnvelope =
        serde_json::from_slice(frame).map_err(|_| TransportError::InvalidResponse)?;

    if response.protocol_version != PROTOCOL_VERSION {
        return Err(TransportError::ProtocolVersionMismatch {
            expected: PROTOCOL_VERSION.to_owned(),
            actual: response.protocol_version,
        });
    }

    if response.request_id != expected_request_id {
        return Err(TransportError::RequestIdMismatch {
            expected: expected_request_id.to_owned(),
            actual: response.request_id,
        });
    }

    if !response.ok {
        // A refusal is required to say why, but a client that trusts that has nothing to report
        // when the service is itself the broken thing. Naming the gap beats unwrapping into it.
        let error = response.error.unwrap_or_else(|| ErrorObject {
            code: "serviceFailure".to_owned(),
            message: "unknown service error".to_owned(),
        });
        return Err(TransportError::Remote { code: error.code, message: error.message });
    }

    Ok(response)
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
    fn a_matching_success_is_accepted() {
        let wire = response_fixture("response/state");
        let response = accept_response("r-1", wire.as_bytes()).expect("fixture was refused");
        assert!(response.ok);
        assert_eq!(response.request_id, "r-1");
    }

    #[test]
    fn a_refusal_becomes_a_remote_error_that_keeps_the_channel() {
        let wire = response_fixture("response/owner-conflict");
        let error = accept_response("r-9", wire.as_bytes()).expect_err("a refusal is not success");
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
        let error = accept_response("r-4", wire.as_bytes()).expect_err("ok:false is not success");
        assert_eq!(
            error,
            TransportError::Remote {
                code: "serviceFailure".into(),
                message: "unknown service error".into(),
            }
        );
    }

    #[test]
    fn a_frame_that_is_not_json_is_refused_without_reading_further() {
        let error = accept_response("r-1", b"not json").expect_err("garbage is not a response");
        assert_eq!(error, TransportError::InvalidResponse);
        assert!(error.invalidates_connection());
    }

    #[test]
    fn a_response_from_a_different_protocol_version_is_refused() {
        let wire = concat!(
            r#"{"protocolVersion":"0.4.0","requestId":"r-1","#,
            r#""ok":true,"payload":{},"error":null}"#
        );
        let error = accept_response("r-1", wire.as_bytes()).expect_err("a newer wire is not ours");
        assert_eq!(
            error,
            TransportError::ProtocolVersionMismatch {
                expected: PROTOCOL_VERSION.to_owned(),
                actual: "0.4.0".to_owned(),
            }
        );
        assert!(error.invalidates_connection());
    }

    #[test]
    fn a_response_to_somebody_elses_request_is_refused() {
        let wire = response_fixture("response/state");
        let error = accept_response("r-77", wire.as_bytes()).expect_err("that is another answer");
        assert_eq!(
            error,
            TransportError::RequestIdMismatch { expected: "r-77".into(), actual: "r-1".into() }
        );
        assert!(error.invalidates_connection());
    }

    /// A stale conversation whose refusal would otherwise look like this call's own failure. The
    /// version and id checks run first precisely so this reports the desynchronisation instead.
    #[test]
    fn a_refusal_carrying_the_wrong_id_is_a_desynchronised_stream_not_a_remote_error() {
        let wire = response_fixture("response/owner-conflict");
        let error = accept_response("r-10", wire.as_bytes()).expect_err("wrong conversation");
        assert!(matches!(error, TransportError::RequestIdMismatch { .. }));
        assert!(error.invalidates_connection());
    }
}
