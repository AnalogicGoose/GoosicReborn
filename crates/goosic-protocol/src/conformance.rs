//! Wire cases every shell must agree on, as data rather than as assertions.
//!
//! Three shells will encode and decode this protocol independently, and the only thing stopping
//! them from drifting is a description that is not written in any one of their languages. These
//! fixtures are that description: JSON files a Rust test, a Swift test, or a WinUI test can all
//! read and hold themselves to. The Rust types are the source of truth for what the protocol
//! *is*; the fixtures are what makes that truth checkable from outside.
//!
//! The files are embedded at build time, so a consumer needs no path and no working directory.

use serde::Deserialize;

use crate::{EventEnvelope, RequestEnvelope, ResponseEnvelope};

/// Which envelope a case is written against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Envelope {
    Request,
    Response,
    Event,
}

/// One wire case, carrying the reason it exists so a failure explains itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub name: String,
    pub envelope: Envelope,
    pub why: String,
    pub wire: String,
}

#[derive(Debug, Deserialize)]
struct Fixtures {
    cases: Vec<Case>,
}

fn parse(source: &str) -> Vec<Case> {
    serde_json::from_str::<Fixtures>(source)
        .expect("fixture file is not valid JSON")
        .cases
}

/// Lines that must decode and re-encode to exactly the same bytes.
pub fn encoding_cases() -> Vec<Case> {
    parse(include_str!("../fixtures/encoding.json"))
}

/// Lines a conforming client must refuse.
pub fn rejection_cases() -> Vec<Case> {
    parse(include_str!("../fixtures/rejection.json"))
}

/// Lines a conforming client must accept even though they are not canonical.
pub fn tolerance_cases() -> Vec<Case> {
    parse(include_str!("../fixtures/tolerance.json"))
}

/// One scripted conversation with the service, replayed from a freshly started state.
///
/// A single response line pins a shape. It cannot pin what the service answers *given what was
/// asked before*, and owner conflict, stale generation and a non-monotonic sample only exist as
/// the second half of a conversation. A client that never tracked the lease would pass a fixture
/// holding one of those responses on its own.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exchange {
    pub name: String,
    pub why: String,
    pub steps: Vec<Step>,
}

/// One request and the exact response it must produce at that point in the conversation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub why: String,
    pub request: String,
    pub response: String,
}

#[derive(Debug, Deserialize)]
struct Exchanges {
    exchanges: Vec<Exchange>,
}

/// Conversations a conforming service must hold, and a conforming shell must expect.
///
/// These are exposed here rather than beside the service because they are contract data: the
/// crate that owns the wire owns the description of it, and a shell in any language reads the
/// same file. The test that drives the real service against them lives in `goosic-service`,
/// which is the only side that can.
pub fn exchange_cases() -> Vec<Exchange> {
    serde_json::from_str::<Exchanges>(include_str!("../fixtures/exchanges.json"))
        .expect("exchange fixture file is not valid JSON")
        .exchanges
}

/// Decodes `wire` as `envelope` and encodes it again, which is the whole conformance check in
/// one step: it proves the fields were understood and that writing them back produces the same
/// frame another implementation would.
pub fn reencode(envelope: Envelope, wire: &str) -> Result<String, serde_json::Error> {
    match envelope {
        Envelope::Request => serde_json::to_string(&serde_json::from_str::<RequestEnvelope>(wire)?),
        Envelope::Response => {
            serde_json::to_string(&serde_json::from_str::<ResponseEnvelope>(wire)?)
        }
        Envelope::Event => serde_json::to_string(&serde_json::from_str::<EventEnvelope>(wire)?),
    }
}

/// Whether `wire` decodes as `envelope` at all.
pub fn decodes(envelope: Envelope, wire: &str) -> bool {
    reencode(envelope, wire).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_lines_survive_a_round_trip_byte_for_byte() {
        let cases = encoding_cases();
        assert!(!cases.is_empty(), "the encoding fixtures are empty");
        for case in cases {
            let actual = reencode(case.envelope, &case.wire).unwrap_or_else(|error| {
                panic!("{}: did not decode: {error}\n{}", case.name, case.why)
            });
            assert_eq!(
                actual, case.wire,
                "{}: re-encoding changed the frame.\n{}",
                case.name, case.why
            );
        }
    }

    #[test]
    fn malformed_lines_are_refused() {
        for case in rejection_cases() {
            assert!(
                !decodes(case.envelope, &case.wire),
                "{}: was accepted and should not have been.\n{}",
                case.name,
                case.why
            );
        }
    }

    #[test]
    fn additive_changes_are_tolerated() {
        for case in tolerance_cases() {
            assert!(
                decodes(case.envelope, &case.wire),
                "{}: was refused and should have been accepted.\n{}",
                case.name,
                case.why
            );
        }
    }

    /// A case that is in two files at once is a contradiction, and the kind that survives review
    /// because each file reads correctly on its own.
    #[test]
    fn no_case_is_both_required_and_refused() {
        let refused: Vec<String> = rejection_cases().into_iter().map(|case| case.wire).collect();
        for case in encoding_cases().into_iter().chain(tolerance_cases()) {
            assert!(
                !refused.contains(&case.wire),
                "{} appears in the rejection fixtures as well",
                case.name
            );
        }
    }

    /// The exchange file is checked here for shape even though only the service can check it
    /// for behaviour. A fixture whose recorded lines stopped being valid envelopes would
    /// otherwise fail far away, inside a service test, looking like a service bug.
    #[test]
    fn every_line_in_an_exchange_is_a_valid_envelope() {
        let exchanges = exchange_cases();
        assert!(!exchanges.is_empty(), "the exchange fixtures are empty");
        for exchange in exchanges {
            assert!(
                !exchange.steps.is_empty(),
                "{}: an exchange with no steps proves nothing",
                exchange.name
            );
            for step in exchange.steps {
                assert!(
                    decodes(Envelope::Request, &step.request),
                    "{}: request did not decode.\n{}",
                    exchange.name,
                    step.why
                );
                assert!(
                    decodes(Envelope::Response, &step.response),
                    "{}: response did not decode.\n{}",
                    exchange.name,
                    step.why
                );
            }
        }
    }

    /// Every step must be answerable by request id alone, which is what lets a shell correlate
    /// a response without tracking position in the stream. A fixture that reused an id across
    /// two steps would quietly describe a protocol nobody can implement that way.
    #[test]
    fn request_ids_are_unique_across_every_exchange() {
        let mut seen: Vec<String> = Vec::new();
        for exchange in exchange_cases() {
            for step in exchange.steps {
                let envelope: crate::RequestEnvelope =
                    serde_json::from_str(&step.request).expect("request did not decode");
                assert!(
                    !seen.contains(&envelope.request_id),
                    "{}: request id {} appears twice",
                    exchange.name,
                    envelope.request_id
                );
                seen.push(envelope.request_id);
            }
        }
    }
}
