use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, TrustedTimeStatement, canonical_execution_constraint,
    canonical_trusted_time,
};
use amiss_wire::digest::Digest;
use amiss_wire::report::model::{
    ControlProvenance, ControlStatus, ControlTrustSource, ReportEnvelope, SandboxAssurance,
    SandboxEnforcementSource, SemanticEvidenceProducer, SemanticEvidenceProvenance,
    VerifiedExecutionConstraint, VerifiedTrustedTime,
};
use serde::Deserialize;

use super::{AcceptanceDefect, SealedExpectations};

#[derive(Deserialize)]
struct Object<T> {
    // Flatten requires an object instead of accepting a positional struct array.
    #[serde(flatten)]
    fields: T,
}

#[derive(Deserialize)]
struct ControlPayload {
    controls: Object<ControlView>,
}

#[derive(Deserialize)]
struct ControlView {
    debt_snapshot: Object<ControlProvenance>,
    execution_constraint:
        Object<VerifiedExecutionConstraint<Object<ExecutionConstraintDescriptor>>>,
    organization_floor: Object<ControlProvenance>,
    profile: Profile,
    sandbox: Object<SandboxView>,
    semantic_evidence: Vec<Object<SemanticEvidenceProvenance<Object<SemanticEvidenceProducer>>>>,
    trusted_time_source: Object<VerifiedTrustedTime<Object<TrustedTimeStatement>>>,
    waiver_bundle: Object<ControlProvenance>,
}

#[derive(Deserialize)]
struct SandboxView {
    assurance: SandboxAssurance,
    enforcement_source: SandboxEnforcementSource,
    #[serde(rename = "verification")]
    _verification: (),
}

pub(super) fn accept(
    wire: &[u8],
    evaluation_instant: Option<&str>,
    identity_digest: Digest,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let mut deserializer = serde_json::Deserializer::from_slice(wire);
    // The caller already applied the strict parser's nesting limit.
    deserializer.disable_recursion_limit();
    let envelope = ReportEnvelope::<ControlPayload>::deserialize(&mut deserializer)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let controls = envelope.payload.controls.fields;
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
    if controls.semantic_evidence.len() != expected.semantic_evidence.len()
        || controls
            .semantic_evidence
            .iter()
            .zip(&expected.semantic_evidence)
            .any(|(actual, expected)| {
                actual.fields.payload_digest != expected.payload_digest
                    || actual.fields.producer.fields != expected.producer
            })
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let constraint = controls.execution_constraint.fields;
    let (_, descriptor_digest) = canonical_execution_constraint(&constraint.descriptor.fields)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if constraint.descriptor_digest != expected.execution_constraint.digest
        || constraint.trust_source != expected.execution_constraint.trust_source
        || descriptor_digest != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let trusted = controls.trusted_time_source.fields;
    let statement = trusted.statement.fields;
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
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    Ok(())
}
