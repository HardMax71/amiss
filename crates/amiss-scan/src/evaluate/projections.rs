use amiss_wire::controls::{BLOB_LINES_SOURCE, CODE_TEXT_PROJECTION, PREVIOUS_CODE_SINK, Profile};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;

use crate::projection::{Outcome, Verdict};

use super::Finding;
use super::claims::{source_multiplicities, sources_value};
use super::control::control_fact_finding;

pub(super) fn projection_finding(outcome: &Outcome, profile: Profile) -> Option<Finding> {
    let Verdict::Drift {
        reason,
        expected_digest,
        observed_digest,
        expected_bytes,
        observed_bytes,
    } = outcome.verdict
    else {
        return None;
    };
    let nullable_digest = |digest: Option<amiss_wire::digest::Digest>| {
        digest.map_or(Value::Null, |value| Value::string(value.to_string()))
    };
    let nullable_bytes = |bytes: Option<u64>| {
        bytes.map_or(Value::Null, |value| {
            Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
        })
    };
    let assertion = &outcome.assertion;
    let evidence = Value::object(vec![
        ("kind".to_owned(), Value::string("projection".to_owned())),
        ("name".to_owned(), Value::string(assertion.name.clone())),
        (
            "projection".to_owned(),
            Value::string(CODE_TEXT_PROJECTION.to_owned()),
        ),
        (
            "sink".to_owned(),
            Value::string(PREVIOUS_CODE_SINK.to_owned()),
        ),
        (
            "source".to_owned(),
            Value::object(vec![
                (
                    "kind".to_owned(),
                    Value::string(BLOB_LINES_SOURCE.to_owned()),
                ),
                (
                    "path".to_owned(),
                    Value::string(assertion.source.path.as_str().to_owned()),
                ),
                (
                    "first_line".to_owned(),
                    Value::Integer(i64::try_from(assertion.source.first_line).unwrap_or(i64::MAX)),
                ),
                (
                    "last_line".to_owned(),
                    Value::Integer(i64::try_from(assertion.source.last_line).unwrap_or(i64::MAX)),
                ),
            ]),
        ),
        (
            "observed".to_owned(),
            Value::string(reason.as_ref().to_owned()),
        ),
        (
            "expected_digest".to_owned(),
            nullable_digest(expected_digest),
        ),
        (
            "observed_digest".to_owned(),
            nullable_digest(observed_digest),
        ),
        ("expected_bytes".to_owned(), nullable_bytes(expected_bytes)),
        ("observed_bytes".to_owned(), nullable_bytes(observed_bytes)),
        (
            "sources".to_owned(),
            sources_value(&source_multiplicities(
                outcome.carrier_digests.iter().copied(),
            )),
        ),
    ]);
    Some(control_fact_finding(
        FindingKind::ProjectionDrift,
        &RepoPath::from(&assertion.document),
        &format!("claim/projection/{}", assertion.name),
        evidence,
        1,
        (outcome.representative_span, outcome.representative_display),
        profile,
    ))
}
