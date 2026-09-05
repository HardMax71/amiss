use amiss_wire::controls::GitMode;
use amiss_wire::controls::Profile;
use amiss_wire::controls::ProjectionSource;
use amiss_wire::digest::Digest;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::FindingKind;
use amiss_wire::report::model::ProjectionDifference;
use amiss_wire::report::model::RowsProjectionDifference;
use amiss_wire::report::model::{
    ControlFactEvidenceKind, ControlFindingKeyScopeKind, ControlState, ControlStateInput,
    ControlStateSchema, ControlStateSource, ExceptionDiagnostic, FindingFactEvidence,
};
use amiss_wire::resolution::Resolution;

use crate::scan::SpanDisplay;

use super::finding::candidate_fact_finding;
use super::{Finding, FindingKeyScope, Location, LocationSide};

/// One candidate document's reserved governed definitions: the exact node
/// count and the distinct source digests with their multiplicities, plus the
/// least location as the representative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSeed {
    pub document: RepoPath,
    pub member_count: u64,
    pub sources: Vec<ControlStateSource>,
    pub representative_span: Option<(usize, usize)>,
    pub representative_display: Option<SpanDisplay>,
}

/// One control-scoped, candidate-only finding: the shared shell of the
/// governed boundary and the claim kinds, differing only in evidence.
pub(super) fn control_fact_finding(
    kind: FindingKind,
    document: &RepoPath,
    rule_id: &str,
    evidence: FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
    member_count: u64,
    representative: (Option<(usize, usize)>, Option<SpanDisplay>),
    profile: Profile,
) -> Result<Finding, crate::Error> {
    candidate_fact_finding(
        kind,
        FindingKeyScope::Control {
            control_path: Some(document.clone()),
            kind: ControlFindingKeyScopeKind::Control,
            rule_id: rule_id.to_owned(),
        },
        evidence,
        member_count,
        Location {
            side: LocationSide::Candidate,
            path: Some(document.clone()),
            span: representative.0,
            display: representative.1,
        },
        profile,
    )
}

/// The reserved governed declaration boundary: control-scoped at the affected
/// document under the one closed rule, with null base state, candidate
/// `unsupported`, exact node multiplicity, and the sorted distinct source
/// digests.
pub(super) fn governed_finding(
    seed: &GovernedSeed,
    profile: Profile,
) -> Result<Finding, crate::Error> {
    let rule_id = "unsupported/governed-claim";
    let evidence = FindingFactEvidence::Control {
        kind: ControlFactEvidenceKind::Control,
        control_path: Some(seed.document.clone()),
        rule_id: rule_id.to_owned(),
        base_control_state: None,
        base_control_digest: None,
        candidate_control_state: Some(ControlStateInput {
            schema: ControlStateSchema::Current,
            rule_id: rule_id.to_owned(),
            path: seed
                .document
                .as_str()
                .map(|path| RepoPathText::new(path.to_owned()).ok_or(crate::Error::Internal))
                .transpose()?,
            sources: seed.sources.clone(),
            state: ControlState::Unsupported,
        }),
        candidate_control_digest: None,
        exception: None,
    };
    control_fact_finding(
        FindingKind::UnsupportedCapability,
        &seed.document,
        rule_id,
        evidence,
        seed.member_count,
        (seed.representative_span, seed.representative_display),
        profile,
    )
}

pub(super) fn control_finding(
    seed: &crate::policy::ControlSeed,
    policy: &crate::policy::Effects,
    profile: Profile,
) -> Result<Finding, crate::Error> {
    control_row(
        seed.kind,
        seed.rule_id.clone(),
        seed.control_path.clone(),
        (policy.base_digest, policy.candidate_digest),
        None,
        profile,
    )
}

/// One control-scoped finding under an exact rule: the fact embeds the
/// governing control's digests and, for exception defects, the complete
/// typed diagnostic.
pub(super) fn control_row(
    kind: FindingKind,
    rule_id: String,
    control_path: Option<RepoPath>,
    control_digests: (Option<Digest>, Option<Digest>),
    exception: Option<ExceptionDiagnostic>,
    profile: Profile,
) -> Result<Finding, crate::Error> {
    let scope = FindingKeyScope::Control {
        control_path: control_path.clone(),
        kind: ControlFindingKeyScopeKind::Control,
        rule_id: rule_id.clone(),
    };
    let evidence = FindingFactEvidence::Control {
        kind: ControlFactEvidenceKind::Control,
        control_path: control_path.clone(),
        rule_id,
        base_control_state: None,
        base_control_digest: control_digests.0,
        candidate_control_state: None,
        candidate_control_digest: control_digests.1,
        exception: exception.map(Box::new),
    };
    let side = if control_path.is_some() {
        LocationSide::Control
    } else {
        LocationSide::Global
    };
    candidate_fact_finding(
        kind,
        scope,
        evidence,
        1,
        Location {
            side,
            path: control_path,
            span: None,
            display: None,
        },
        profile,
    )
}
