use amiss_wire::controls::{PREVIOUS_CODE_SINK, Profile, projection_source_value};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::{ProjectionDifference, RowsProjectionDifference};

use crate::projection::{Outcome, Verdict};

use super::Finding;
use super::claims::{source_multiplicities, sources_value};
use super::control::control_fact_finding;

mod tests;

fn difference_value(difference: &ProjectionDifference<Box<RowsProjectionDifference>>) -> Value {
    let integer = |value| Value::Integer(i64::try_from(value).unwrap_or(i64::MAX));
    let rows = |values: &[String]| {
        Value::array(
            values
                .iter()
                .cloned()
                .map(Value::string)
                .collect::<Vec<_>>(),
        )
    };
    match difference {
        ProjectionDifference::Rows(difference) => Value::object(vec![
            ("kind".to_owned(), Value::string("rows".to_owned())),
            (
                "ordering_only".to_owned(),
                Value::Bool(difference.ordering_only),
            ),
            (
                "expected_records".to_owned(),
                integer(difference.expected_records),
            ),
            (
                "observed_records".to_owned(),
                integer(difference.observed_records),
            ),
            (
                "missing_records".to_owned(),
                integer(difference.missing_records),
            ),
            (
                "extra_records".to_owned(),
                integer(difference.extra_records),
            ),
            (
                "missing_preview".to_owned(),
                rows(&difference.missing_preview),
            ),
            ("extra_preview".to_owned(), rows(&difference.extra_preview)),
            (
                "missing_omitted".to_owned(),
                integer(difference.missing_omitted),
            ),
            (
                "extra_omitted".to_owned(),
                integer(difference.extra_omitted),
            ),
        ]),
        ProjectionDifference::Count {
            expected_count,
            observed_count,
            ..
        } => Value::object(vec![
            ("kind".to_owned(), Value::string("count".to_owned())),
            ("expected_count".to_owned(), integer(*expected_count)),
            (
                "observed_count".to_owned(),
                observed_count.map_or(Value::Null, integer),
            ),
        ]),
    }
}

pub(super) fn projection_finding(
    outcome: &Outcome,
    profile: Profile,
) -> Result<Option<Finding>, crate::Error> {
    let Verdict::Drift {
        reason,
        expected_digest,
        observed_digest,
        expected_bytes,
        observed_bytes,
        ref difference,
    } = outcome.verdict
    else {
        return Ok(None);
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
    let mut evidence = vec![
        ("kind".to_owned(), Value::string("projection".to_owned())),
        ("name".to_owned(), Value::string(assertion.name.clone())),
        (
            "projection".to_owned(),
            Value::string(assertion.projection.as_ref().to_owned()),
        ),
        (
            "sink".to_owned(),
            Value::string(PREVIOUS_CODE_SINK.to_owned()),
        ),
        (
            "source".to_owned(),
            projection_source_value(&assertion.source),
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
    ];
    if let Some(difference) = difference.as_ref() {
        evidence.push(("difference".to_owned(), difference_value(difference)));
    }
    control_fact_finding(
        FindingKind::ProjectionDrift,
        &RepoPath::from(&assertion.document),
        &format!("claim/projection/{}", assertion.name),
        Value::object(evidence),
        1,
        (outcome.representative_span, outcome.representative_display),
        profile,
    )
    .map(Some)
}
