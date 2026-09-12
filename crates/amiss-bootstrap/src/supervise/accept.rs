use amiss_wire::codec::{borrow_value, nullable};
use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};
use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::report::{ENVELOPE_SCHEMA, PAYLOAD_SCHEMA};
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
use serde::Deserialize;

use super::{
    AcceptanceDefect, Expectations, SealedControlExpectation, SealedExpectations,
    SealedSemanticExpectation,
};

#[derive(Deserialize)]
struct Header<'a> {
    schema: &'a str,
    payload_digest: &'a str,
}
#[derive(Deserialize)]
struct Engine<'a> {
    engine_digest: &'a str,
}
#[derive(Deserialize)]
struct Payload<'a> {
    schema: &'a str,
    #[serde(borrow)]
    engine: Engine<'a>,
}
#[derive(Deserialize)]
struct Snapshot<'a> {
    kind: Option<&'a str>,
    commit_oid: Option<&'a str>,
}
#[derive(Deserialize)]
struct Evaluation<'a> {
    status: Option<&'a str>,
    #[serde(borrow)]
    base: Option<Snapshot<'a>>,
    #[serde(borrow)]
    candidate: Option<Snapshot<'a>>,
}
#[derive(Deserialize)]
struct ResultFields {
    exit_code: i64,
    complete: bool,
    finding_count: i64,
}
#[derive(Deserialize)]
struct SealedEvaluation<'a> {
    candidate_ref: &'a str,
    target_ref: &'a str,
    trusted_time: bool,
    evaluation_instant: &'a str,
}
#[derive(Deserialize)]
struct Control<'a> {
    status: &'a str,
    #[serde(borrow, deserialize_with = "nullable")]
    digest: Option<&'a str>,
    trust_source: &'a str,
}
#[derive(Deserialize)]
struct Producer<'a> {
    kind: &'a str,
    identity: &'a str,
    version: &'a str,
    input_digest: &'a str,
}
#[derive(Deserialize)]
struct Semantic<'a> {
    payload_digest: &'a str,
    #[serde(borrow)]
    producer: Producer<'a>,
}
#[derive(Deserialize)]
struct Constraint<'a> {
    status: &'a str,
    descriptor_digest: &'a str,
    trust_source: &'a str,
    descriptor: ExecutionConstraintDescriptor,
}
#[derive(Deserialize)]
struct TrustedTime<'a> {
    status: &'a str,
    statement_digest: &'a str,
    trust_source: &'a str,
    statement: TrustedTimeStatement,
}
#[derive(Deserialize)]
struct Sandbox<'a> {
    assurance: &'a str,
    enforcement_source: &'a str,
    verification: (),
}
#[derive(Deserialize)]
struct Controls<'a> {
    profile: &'a str,
    #[serde(borrow)]
    organization_floor: Control<'a>,
    #[serde(borrow)]
    debt_snapshot: Control<'a>,
    #[serde(borrow)]
    waiver_bundle: Control<'a>,
    #[serde(borrow)]
    semantic_evidence: Vec<Semantic<'a>>,
    #[serde(borrow)]
    execution_constraint: Constraint<'a>,
    #[serde(borrow)]
    trusted_time_source: TrustedTime<'a>,
    #[serde(borrow)]
    sandbox: Sandbox<'a>,
}

/// Accepts only canonical reports bound to the validated engine and requested identities.
///
/// # Errors
///
/// The wire, digest, identities, sealed controls, or result counts do not agree.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<i64, AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    let envelope = parse(trimmed).map_err(|_defect| AcceptanceDefect::Shape)?;
    if canonical(&envelope) != trimmed {
        return Err(AcceptanceDefect::Noncanonical);
    }
    let header: Header<'_> =
        borrow_value("$", &envelope).map_err(|_defect| AcceptanceDefect::Shape)?;
    if header.schema != ENVELOPE_SCHEMA {
        return Err(AcceptanceDefect::Shape);
    }
    let payload = envelope.get("payload").ok_or(AcceptanceDefect::Shape)?;
    let fields: Payload<'_> =
        borrow_value("$.payload", payload).map_err(|_defect| AcceptanceDefect::Shape)?;
    if fields.schema != PAYLOAD_SCHEMA {
        return Err(AcceptanceDefect::Shape);
    }
    if hj(PAYLOAD_SCHEMA, payload).to_string() != header.payload_digest {
        return Err(AcceptanceDefect::PayloadDigest);
    }
    if fields.engine.engine_digest != expectations.engine_digest {
        return Err(AcceptanceDefect::Engine);
    }
    let evaluation = payload.get("evaluation").ok_or(AcceptanceDefect::Shape)?;
    let fields: Evaluation<'_> = borrow_value("$.payload.evaluation", evaluation)
        .map_err(|_defect| AcceptanceDefect::Shape)?;
    let resolved = fields.status != Some("unavailable");
    if expectations.sealed.is_some() && !resolved {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    if resolved {
        let base = fields.base.ok_or(AcceptanceDefect::Shape)?;
        if base.commit_oid != Some(expectations.base_commit.as_str()) {
            return Err(AcceptanceDefect::BaseIdentity);
        }
        let candidate = fields.candidate.ok_or(AcceptanceDefect::Shape)?;
        if let Some(expected) = expectations.candidate_commit.as_deref()
            && (candidate.kind != Some("git-commit") || candidate.commit_oid != Some(expected))
        {
            return Err(AcceptanceDefect::CandidateIdentity);
        }
        if let Some(sealed) = &expectations.sealed {
            accept_sealed(payload, evaluation, sealed)?;
        }
    }
    let result = payload.get("result").ok_or(AcceptanceDefect::Shape)?;
    let result: ResultFields =
        borrow_value("$.payload.result", result).map_err(|_defect| AcceptanceDefect::Shape)?;
    if result.complete != (result.exit_code == 0 || result.exit_code == 1) {
        return Err(AcceptanceDefect::Completeness);
    }
    let findings = payload
        .get("findings")
        .and_then(Value::as_array)
        .ok_or(AcceptanceDefect::Shape)?
        .len();
    if i64::try_from(findings).map_err(|_defect| AcceptanceDefect::Shape)? != result.finding_count {
        return Err(AcceptanceDefect::FindingCount);
    }
    Ok(result.exit_code)
}

fn accept_sealed(
    payload: &Value,
    evaluation: &Value,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let fields: SealedEvaluation<'_> = borrow_value("$.payload.evaluation", evaluation)
        .map_err(|_defect| AcceptanceDefect::SealedIdentity)?;
    if fields.candidate_ref != expected.candidate_ref
        || fields.target_ref != expected.target_ref
        || !fields.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let members = evaluation.as_object().ok_or(AcceptanceDefect::Shape)?;
    if members.contains_key("schema") {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let mut identity = members.clone();
    identity.remove("evaluation_instant");
    identity.remove("trusted_time");
    identity.insert(
        "schema".to_owned(),
        Value::String(CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    );
    let identity_digest = hj(CANDIDATE_IDENTITY_DOMAIN, &Value::Object(identity)).to_string();
    if identity_digest != expected.candidate_identity_digest {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let controls = payload
        .get("controls")
        .ok_or(AcceptanceDefect::SealedControls)?;
    let controls: Controls<'_> = borrow_value("$.payload.controls", controls)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if controls.profile != expected.profile {
        return Err(AcceptanceDefect::SealedControls);
    }
    accept_control(
        &controls.organization_floor,
        expected.organization_floor.as_ref(),
    )?;
    accept_control(&controls.debt_snapshot, expected.debt_snapshot.as_ref())?;
    accept_control(&controls.waiver_bundle, expected.waiver_bundle.as_ref())?;
    accept_semantic(&controls.semantic_evidence, &expected.semantic_evidence)?;
    let constraint = controls.execution_constraint;
    if constraint.status != "verified"
        || constraint.descriptor_digest != expected.execution_constraint.digest
        || constraint.trust_source != expected.execution_constraint.trust_source
        || constraint.descriptor.digest().to_string() != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let trusted = controls.trusted_time_source;
    let statement = trusted.statement;
    if trusted.status != "verified"
        || trusted.trust_source != "external-required-check"
        || trusted.statement_digest != expected.trusted_time_digest
        || statement.digest().to_string() != expected.trusted_time_digest
        || statement.provider() != expected.provider
        || statement.provider_run_id() != expected.provider_run_id
        || statement.provider_run_attempt() != expected.provider_run_attempt
        || statement.repository() != &expected.repository
        || statement.ref_name().as_str() != expected.target_ref
        || statement.candidate_identity_digest().to_string() != identity_digest
        || fields.evaluation_instant != statement.evaluation_instant().as_str()
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let () = controls.sandbox.verification;
    if controls.sandbox.assurance != "self-asserted"
        || controls.sandbox.enforcement_source != "local-process"
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    Ok(())
}

fn accept_semantic(
    actual: &[Semantic<'_>],
    expected: &[SealedSemanticExpectation],
) -> Result<(), AcceptanceDefect> {
    if actual.len() != expected.len() {
        return Err(AcceptanceDefect::SealedControls);
    }
    for (actual, expected) in actual.iter().zip(expected) {
        if actual.payload_digest != expected.payload_digest
            || actual.producer.kind != expected.producer_kind
            || actual.producer.identity != expected.producer_identity
            || actual.producer.version != expected.producer_version
            || actual.producer.input_digest != expected.input_digest
        {
            return Err(AcceptanceDefect::SealedControls);
        }
    }
    Ok(())
}

fn accept_control(
    actual: &Control<'_>,
    expected: Option<&SealedControlExpectation>,
) -> Result<(), AcceptanceDefect> {
    let accepted = match expected {
        Some(expected) => {
            actual.status == "verified"
                && actual.digest == Some(expected.digest.as_str())
                && actual.trust_source == expected.trust_source
        }
        None => actual.status == "none" && actual.digest.is_none() && actual.trust_source == "none",
    };
    if accepted {
        Ok(())
    } else {
        Err(AcceptanceDefect::SealedControls)
    }
}
