use amiss_wire::controls::{Profile, ProjectionSink};
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::{FindingFactEvidence, ProjectionFactEvidenceKind};

use crate::projection::{Outcome, Verdict};

use super::Finding;
use super::claims::source_multiplicities;
use super::control::control_fact_finding;

mod tests;

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
    let assertion = &outcome.assertion;
    let evidence = FindingFactEvidence::Projection {
        kind: ProjectionFactEvidenceKind::Projection,
        name: assertion.name.clone(),
        projection: assertion.projection,
        sink: ProjectionSink::PreviousCode,
        source: assertion.source.clone(),
        observed: reason,
        expected_digest,
        observed_digest,
        expected_bytes,
        observed_bytes,
        sources: source_multiplicities(outcome.carrier_digests.iter().copied()),
        difference: difference.clone(),
    };
    control_fact_finding(
        FindingKind::ProjectionDrift,
        &RepoPath::from(&assertion.document),
        &format!("claim/projection/{}", assertion.name),
        evidence,
        1,
        (outcome.representative_span, outcome.representative_display),
        profile,
    )
    .map(Some)
}
