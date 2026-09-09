use amiss_wire::controls::{canonical_execution_constraint, canonical_trusted_time};
use amiss_wire::digest::Digest;
use amiss_wire::report::model::{
    ControlProvenance, ControlStatus, ControlTrustSource, Controls, ExecutionConstraintProvenance,
    NoControlStatus, SandboxAssurance, SandboxEnforcementSource, TrustedTimeProvenance,
};

use super::{AcceptanceDefect, SealedExpectations};

pub(super) fn accept(
    controls: &Controls,
    evaluation_instant: Option<&str>,
    identity_digest: Digest,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let Controls::Resolved(controls) = controls else {
        return Err(AcceptanceDefect::SealedControls);
    };
    let (
        ExecutionConstraintProvenance::Verified(constraint),
        TrustedTimeProvenance::Verified(trusted),
        Some(semantic_evidence),
    ) = (
        &controls.execution_constraint,
        &controls.trusted_time_source,
        &controls.semantic_evidence,
    )
    else {
        return Err(AcceptanceDefect::SealedControls);
    };
    if controls.profile != expected.profile {
        return Err(AcceptanceDefect::SealedControls);
    }
    for (actual, expected) in [
        (&controls.organization_floor, &expected.organization_floor),
        (&controls.debt_snapshot, &expected.debt_snapshot),
        (&controls.waiver_bundle, &expected.waiver_bundle),
    ] {
        let expected = expected.as_ref();
        let expected = ControlProvenance {
            digest: expected.map(|expected| expected.digest),
            status: expected.map_or(ControlStatus::None, |_| ControlStatus::Verified),
            trust_source: expected.map_or(
                ControlTrustSource::None(NoControlStatus::None),
                |expected| ControlTrustSource::Verified(expected.trust_source),
            ),
        };
        if *actual != expected {
            return Err(AcceptanceDefect::SealedControls);
        }
    }
    if semantic_evidence != &expected.semantic_evidence {
        return Err(AcceptanceDefect::SealedControls);
    }
    let (_, descriptor_digest) = canonical_execution_constraint(&constraint.descriptor)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if constraint.descriptor_digest != expected.execution_constraint.digest
        || constraint.trust_source != expected.execution_constraint.trust_source
        || descriptor_digest != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let statement = &trusted.statement;
    let (_, statement_digest) =
        canonical_trusted_time(statement).map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if trusted.statement_digest != expected.trusted_time_digest
        || statement_digest != expected.trusted_time_digest
        || statement.provider != expected.provider
        || statement.provider_run_id != expected.provider_run_id
        || statement.provider_run_attempt != expected.provider_run_attempt
        || statement.repository != expected.repository
        || statement.ref_name.as_str() != expected.target_ref
        || statement.candidate_identity_digest != identity_digest
        || evaluation_instant != Some(statement.evaluation_instant.as_str())
        || controls.sandbox.assurance != SandboxAssurance::SelfAsserted
        || controls.sandbox.enforcement_source != SandboxEnforcementSource::LocalProcess
        || controls.sandbox.verification.is_some()
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    Ok(())
}
