use sha2::Digest as _;

use amiss_wire::model::Digest;
use amiss_wire::report::model::{
    ControlStatus, ControlTrustSource, Controls, ExecutionConstraintProvenance, SandboxAssurance,
    SandboxEnforcementSource, TrustedTimeProvenance,
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
    if semantic_evidence != &expected.semantic_evidence {
        return Err(AcceptanceDefect::SealedControls);
    }
    constraint
        .descriptor
        .validate()
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let mut writer = digest_io::IoWrapper(
        sha2::Sha256::new_with_prefix(amiss_wire::controls::EXECUTION_CONSTRAINT_SCHEMA)
            .chain_update([0_u8]),
    );
    serde_json_canonicalizer::to_writer(&constraint.descriptor, &mut writer)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let descriptor_digest = Digest::from(writer.0.finalize().0);
    if constraint.descriptor_digest != expected.execution_constraint.digest
        || constraint.trust_source != expected.execution_constraint.trust_source
        || descriptor_digest != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let statement = &trusted.statement;
    statement
        .validate()
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let mut writer = digest_io::IoWrapper(
        sha2::Sha256::new_with_prefix(amiss_wire::controls::TRUSTED_TIME_STATEMENT_SCHEMA)
            .chain_update([0_u8]),
    );
    serde_json_canonicalizer::to_writer(&statement, &mut writer)
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let statement_digest = Digest::from(writer.0.finalize().0);
    if trusted.statement_digest != expected.trusted_time_digest
        || statement_digest != expected.trusted_time_digest
        || statement.provider != expected.provider
        || statement.provider_run_id != expected.provider_run_id
        || statement.provider_run_attempt != expected.provider_run_attempt
        || statement.repository != expected.repository
        || statement.ref_name != expected.target_ref
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
