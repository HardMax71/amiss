use amiss_wire::controls::{Profile, canonical_execution_constraint, canonical_trusted_time};
use amiss_wire::digest::Digest;
use amiss_wire::report::model::{
    ControlProvenance, ControlStatus, ControlTrustSource, SandboxAssurance,
    SandboxEnforcementSource, SandboxProvenance, SemanticEvidenceProvenance,
    VerifiedExecutionConstraint, VerifiedTrustedTime,
};
use serde::Deserialize;
use serde_json::Value;

use super::model::Object;
use super::{AcceptanceDefect, SealedExpectations};

#[derive(Deserialize)]
struct ControlPayload {
    controls: Object<ControlView>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlView {
    #[serde(
        rename = "base_repository_policy_digest",
        deserialize_with = "Option::deserialize"
    )]
    _base_repository_policy_digest: Option<Digest>,
    #[serde(
        rename = "candidate_repository_policy_digest",
        deserialize_with = "Option::deserialize"
    )]
    _candidate_repository_policy_digest: Option<Digest>,
    debt_snapshot: Object<ControlProvenance>,
    execution_constraint: Object<VerifiedExecutionConstraint>,
    organization_floor: Object<ControlProvenance>,
    profile: Profile,
    sandbox: Object<SandboxProvenance>,
    semantic_evidence: Vec<Object<SemanticEvidenceProvenance>>,
    trusted_time_source: Object<VerifiedTrustedTime>,
    waiver_bundle: Object<ControlProvenance>,
}

pub(super) fn accept(
    payload: &Value,
    evaluation_instant: Option<&str>,
    identity_digest: Digest,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let payload =
        ControlPayload::deserialize(payload).map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let controls = payload.controls.fields;
    if controls.profile != expected.profile {
        return Err(AcceptanceDefect::SealedControls);
    }
    for (actual, expected) in [
        (
            &controls.organization_floor.fields,
            &expected.organization_floor,
        ),
        (&controls.debt_snapshot.fields, &expected.debt_snapshot),
        (&controls.waiver_bundle.fields, &expected.waiver_bundle),
    ] {
        let accepted = match expected {
            Some(expected) => {
                actual.status == ControlStatus::Verified
                    && actual.digest == Some(expected.digest)
                    && actual.trust_source.as_ref() == expected.trust_source.as_ref()
            }
            None => {
                actual.status == ControlStatus::None
                    && actual.digest.is_none()
                    && actual.trust_source == ControlTrustSource::None
            }
        };
        if !accepted {
            return Err(AcceptanceDefect::SealedControls);
        }
    }
    if controls
        .semantic_evidence
        .iter()
        .map(|row| &row.fields)
        .ne(&expected.semantic_evidence)
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let constraint = controls.execution_constraint.fields;
    let (_, descriptor_digest) = canonical_execution_constraint(&constraint.descriptor)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if constraint.descriptor_digest != expected.execution_constraint.digest
        || constraint.trust_source != expected.execution_constraint.trust_source
        || descriptor_digest != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let trusted = controls.trusted_time_source.fields;
    let statement = trusted.statement;
    let (_, statement_digest) =
        canonical_trusted_time(&statement).map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if trusted.statement_digest != expected.trusted_time_digest
        || statement_digest != expected.trusted_time_digest
        || statement.provider != expected.provider
        || statement.provider_run_id != expected.provider_run_id
        || statement.provider_run_attempt != expected.provider_run_attempt
        || statement.repository != expected.repository
        || statement.ref_name.as_str() != expected.target_ref
        || statement.candidate_identity_digest != identity_digest
        || evaluation_instant != Some(statement.evaluation_instant.as_str())
        || controls.sandbox.fields.assurance != SandboxAssurance::SelfAsserted
        || controls.sandbox.fields.enforcement_source != SandboxEnforcementSource::LocalProcess
        || controls.sandbox.fields.verification.is_some()
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    Ok(())
}
