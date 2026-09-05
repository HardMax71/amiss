use amiss_wire::digest::{Digest, sha256};
use amiss_wire::relation::{
    self, RELATION_DOCUMENT_BYTES, RelationVerdict, assess, parse_assessment, parse_evidence,
    parse_plan,
};

use crate::audit_report::accepted_report;
use crate::{ArtifactError, RelationSubjectTransition, RelationTransition, relation_transition};

#[derive(Clone, Copy)]
pub struct RelationAuditBundle<'a> {
    pub transition: &'a RelationTransition,
    pub report: &'a [u8],
    pub plan: &'a [u8],
    pub evidence: Option<&'a [u8]>,
    pub assessment: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationAuditDigests {
    pub report_digest: Digest,
    pub plan_digest: Digest,
    pub evidence_digest: Option<Digest>,
    pub assessment_digest: Digest,
    pub verdict: RelationVerdict,
}

/// Builds the unique canonical audit plan for one accepted trigger report and frozen relation.
///
/// # Errors
///
/// The report is not accepted, the transition is invalid, or the report does not reproduce the
/// registered trigger subject and its exact base/candidate snapshots.
pub fn relation_audit_plan(
    transition: &RelationTransition,
    report: &[u8],
) -> Result<Vec<u8>, ArtifactError> {
    let plan = checked_relation_plan(transition, report)?.0;
    relation::plan(&plan).map_err(|_defect| ArtifactError::Corrupt)
}

/// Validates one complete relation audit against its accepted trigger report
/// and frozen operator transition before retention.
///
/// No repository or provider material is acquired or interpreted here.
///
/// # Errors
///
/// Returns [`ArtifactError::TooLarge`] when a component crosses its contract
/// ceiling and [`ArtifactError::Corrupt`] for every malformed, substituted,
/// or non-replayable chain.
pub fn validate_relation_audit(
    bundle: RelationAuditBundle<'_>,
) -> Result<RelationAuditDigests, ArtifactError> {
    if [bundle.plan, bundle.assessment]
        .into_iter()
        .chain(bundle.evidence)
        .any(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES)
    {
        return Err(ArtifactError::TooLarge);
    }
    let (expected, report_digest) = checked_relation_plan(bundle.transition, bundle.report)?;
    let plan = parse_plan(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    (plan.payload == expected)
        .then_some(())
        .ok_or(ArtifactError::Corrupt)?;
    let evidence = bundle
        .evidence
        .map(parse_evidence)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment =
        parse_assessment(bundle.assessment).map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = assess(
        &plan,
        evidence.as_ref(),
        &assessment.payload.engine.engine_version,
        assessment.payload.engine.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = parse_assessment(&replayed).map_err(|_defect| ArtifactError::Corrupt)?;
    if replayed.payload_digest != assessment.payload_digest {
        return Err(ArtifactError::Corrupt);
    }
    Ok(RelationAuditDigests {
        report_digest,
        plan_digest: sha256(bundle.plan),
        evidence_digest: bundle.evidence.map(sha256),
        assessment_digest: sha256(bundle.assessment),
        verdict: assessment.payload.verdict,
    })
}

fn checked_relation_plan(
    transition: &RelationTransition,
    report: &[u8],
) -> Result<(relation::RelationPlan, Digest), ArtifactError> {
    let transition = relation_transition(
        transition.relation.clone(),
        transition.coordination.clone(),
        transition.subjects.clone(),
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let report = accepted_report(report)?;
    let registered = transition.relation.plan.as_ref();
    let planned = |frozen: &RelationSubjectTransition| -> Result<_, ArtifactError> {
        let subject = registered
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .ok_or(ArtifactError::Corrupt)?;
        Ok(relation::RelationSubject {
            role: frozen.role.clone(),
            repository: subject.scope.repository.clone(),
            target: subject.target.clone(),
            object_format: subject.object_format,
            source: subject.source.clone(),
            base: relation::RelationSnapshot {
                commit: frozen.commits.base.clone(),
                tree: frozen.trees.base.clone(),
            },
            candidate: relation::RelationSnapshot {
                commit: frozen.commits.candidate.clone(),
                tree: frozen.trees.candidate.clone(),
            },
        })
    };
    let [left, right] = transition.subjects.each_ref();
    let subjects = [planned(left)?, planned(right)?];
    subjects
        .iter()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .filter(|trigger| {
            trigger.repository == report.repository
                && report.target_ref.as_ref() == Some(&trigger.target)
                && trigger.base.commit == report.base.commit
                && trigger.base.tree == report.base.tree
                && trigger.candidate.commit == report.candidate.commit
                && trigger.candidate.tree == report.candidate.tree
        })
        .ok_or(ArtifactError::Corrupt)?;
    Ok((
        relation::RelationPlan {
            report_payload_digest: report.payload_digest,
            relation: relation::RelationIdentity {
                identity: registered.identity.clone(),
                context_digest: registered.context_digest,
            },
            coordination: transition.coordination,
            trigger_role: transition.relation.trigger_role,
            projection: registered.projection,
            subjects,
        },
        report.report_digest,
    ))
}
