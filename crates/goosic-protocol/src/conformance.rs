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
}
