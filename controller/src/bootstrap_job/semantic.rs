use std::collections::BTreeSet;

use amiss_wire::digest::Digest;
use amiss_wire::json::{self, Value};
use amiss_wire::requests::SuppliedSemanticEvidence;

use super::plan::normalized_expectations;
use super::{BootstrapJobError, SemanticEvidenceExpectation, SemanticEvidenceTemplate};

/// Binds controller-produced templates and acquired envelopes to one exact
/// candidate and orders the resulting set by payload identity.
///
/// # Errors
///
/// An envelope is malformed, exceeds a limit, names another subject, or
/// collides with another envelope.
pub fn bind_semantic_evidence(
    templates: &[SemanticEvidenceTemplate],
    expectations: &[SemanticEvidenceExpectation],
    acquired: &[Value],
    candidate_identity_digest: Digest,
) -> Result<Vec<SuppliedSemanticEvidence>, BootstrapJobError> {
    if expectations.len() != acquired.len() {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let count = templates
        .len()
        .checked_add(acquired.len())
        .ok_or(BootstrapJobError::SemanticEvidence)?;
    if count > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let mut expected = BTreeSet::from_iter(normalized_expectations(expectations)?);
    let templates = templates.iter().map(|template| {
        let value = amiss_wire::semantic::bind_template(template, candidate_identity_digest)
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        let envelope = amiss_wire::semantic::parse(&json::canonical(&value))
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        checked_evidence(
            value,
            &envelope,
            template.context_digest,
            candidate_identity_digest,
        )
    });
    let acquired = acquired.iter().cloned().map(|value| {
        let envelope = amiss_wire::semantic::parse(&json::canonical(&value))
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        let expectation = SemanticEvidenceExpectation {
            producer_kind: envelope.payload.producer_kind.clone(),
            producer_identity: envelope.payload.producer_identity.clone(),
            producer_version: envelope.payload.producer_version.clone(),
            context_digest: envelope.payload.context_digest,
        };
        expected
            .remove(&expectation)
            .then_some(())
            .ok_or(BootstrapJobError::SemanticEvidence)?;
        checked_evidence(
            value,
            &envelope,
            expectation.context_digest,
            candidate_identity_digest,
        )
    });
    let mut envelopes = templates
        .chain(acquired)
        .collect::<Result<Vec<_>, BootstrapJobError>>()?;
    envelopes.sort_by_key(|(digest, _evidence)| *digest);
    if envelopes
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.0 == right.0))
    {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    Ok(envelopes
        .into_iter()
        .map(|(_digest, evidence)| evidence)
        .collect())
}

fn checked_evidence(
    value: Value,
    envelope: &amiss_wire::semantic::SemanticEvidenceEnvelope,
    expected_context_digest: Digest,
    candidate_identity_digest: Digest,
) -> Result<(Digest, SuppliedSemanticEvidence), BootstrapJobError> {
    (envelope.payload.candidate_identity_digest == candidate_identity_digest
        && envelope.payload.source_report_payload_digest.is_none()
        && envelope.payload.context_digest == expected_context_digest)
        .then_some((
            envelope.payload_digest,
            SuppliedSemanticEvidence {
                value,
                expected_context_digest,
            },
        ))
        .ok_or(BootstrapJobError::SemanticEvidence)
}
