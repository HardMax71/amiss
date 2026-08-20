use crate::digest::hj;
use crate::json::Value;

mod assessment;
mod plan;

pub use assessment::{AssessDefect, assess};
pub use plan::{PlanDefect, plan};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/external-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/external-plan-payload";
pub const EVIDENCE_SCHEMA: &str = "amiss/external-evidence";
pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/external-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/external-assessment-payload";

/// Whether the value is an external plan envelope whose payload matches its
/// recorded digest: the check a producer makes before spending calls on it.
#[must_use]
pub fn bound_plan(plan: &Value) -> bool {
    let (Some(payload), Some(recorded)) = (plan.member("payload"), plan.text("payload_digest"))
    else {
        return false;
    };
    plan.text("schema") == Some(PLAN_ENVELOPE_SCHEMA)
        && payload.text("schema") == Some(PLAN_PAYLOAD_SCHEMA)
        && hj(PLAN_PAYLOAD_SCHEMA, payload).to_string() == recorded
}

/// One producer's evidence file over a plan: the binding digest is read from
/// the plan itself, so a producer never computes one.
#[must_use]
pub fn evidence_file(
    plan: &Value,
    producer_name: &str,
    producer_version: &str,
    rows: Vec<Value>,
) -> Option<Value> {
    let digest = plan.text("payload_digest")?;
    Some(object(vec![
        ("schema", string(EVIDENCE_SCHEMA)),
        ("plan_payload_digest", string(digest)),
        (
            "producer",
            object(vec![
                ("name", string(producer_name)),
                ("version", string(producer_version)),
            ]),
        ),
        ("rows", Value::Array(rows.into_boxed_slice())),
    ]))
}

/// One http-probe observation row: the final status or the transport
/// failure, exactly one of the two, and where redirects ended when that
/// differs from the destination.
#[must_use]
pub fn probe_evidence_row(
    destination: &str,
    method: &str,
    status: Option<i64>,
    failure: Option<&str>,
    final_destination: Option<&str>,
    checked_at: &str,
) -> Value {
    let mut members = vec![
        ("kind", string("http-probe")),
        ("destination", string(destination)),
        ("method", string(method)),
        ("checked_at", string(checked_at)),
    ];
    if let Some(status) = status {
        members.push(("status", Value::Integer(status)));
    }
    if let Some(failure) = failure {
        members.push(("failure", string(failure)));
    }
    if let Some(final_destination) = final_destination {
        members.push(("final_destination", string(final_destination)));
    }
    object(members)
}

/// One forge-api observation row, tail present only when a resolution was
/// actually established.
#[must_use]
pub fn forge_evidence_row(
    destination: &str,
    repository: &str,
    tail: Option<&str>,
    checked_at: &str,
) -> Value {
    let mut members = vec![
        ("kind", string("forge-api")),
        ("destination", string(destination)),
        ("repository", string(repository)),
        ("checked_at", string(checked_at)),
    ];
    if let Some(tail) = tail {
        members.push(("tail", string(tail)));
    }
    object(members)
}

/// An external object in canonical member order; the parser demands sorted keys,
/// so the builder sorts rather than trusting the spelling site.
fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::Object(members.into_boxed_slice())
}

fn string(value: &str) -> Value {
    Value::String(value.into())
}
