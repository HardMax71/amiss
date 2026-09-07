mod tests;

use amiss_wire::digest::{Digest, hj_serde, sha256};
use amiss_wire::model::{BranchRef, Oid, RepositoryIdentity};
use amiss_wire::report::model::{
    BaseSnapshot, Evaluation, IdentityPreimage, ReportPayload, Snapshot,
};
use amiss_wire::requests::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateEventKind, CandidateFinality, CandidateIdentitySchema,
    CandidateSnapshot, RequestMode, SnapshotMaterialization,
};

use crate::ArtifactError;

pub(crate) struct AcceptedReport {
    pub(crate) report_digest: Digest,
    pub(crate) payload_digest: Digest,
    pub(crate) repository: RepositoryIdentity,
    pub(crate) target_ref: Option<BranchRef>,
    pub(crate) base: AcceptedSnapshot,
    pub(crate) candidate: AcceptedSnapshot,
    pub(crate) candidate_identity_digest: Digest,
}

pub(crate) struct AcceptedSnapshot {
    pub(crate) commit: Oid,
    pub(crate) tree: Oid,
}

pub(crate) fn accepted_report(bytes: &[u8]) -> Result<AcceptedReport, ArtifactError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Err(ArtifactError::TooLarge);
    }
    let (ReportPayload { evaluation, .. }, payload_digest, verdict) =
        amiss_wire::report::validate_envelope(bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    if verdict == amiss_wire::ExitClass::Failure {
        return Err(ArtifactError::Corrupt);
    }
    let Evaluation::Resolved(evaluation) = evaluation else {
        return Err(ArtifactError::Corrupt);
    };
    if evaluation.mode != RequestMode::CommitPair
        || evaluation.event_kind != CandidateEventKind::ExplicitCommitPair
        || evaluation.finality != CandidateFinality::ExplicitReplay
        || evaluation.materialization != SnapshotMaterialization::GitObjects
        || evaluation.skip_worktree_paths != 0
        || evaluation.index_only_materialized_paths != 0
    {
        return Err(ArtifactError::Corrupt);
    }
    let preimage = IdentityPreimage {
        evaluation: &evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let candidate_identity_digest = hj_serde(CANDIDATE_IDENTITY_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
    })
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let (BaseSnapshot::Git(base), Snapshot::Available(CandidateSnapshot::Git(candidate))) =
        (evaluation.base, evaluation.candidate)
    else {
        return Err(ArtifactError::Corrupt);
    };
    if base.object_format != candidate.object_format
        || [&base, &candidate].into_iter().any(|snapshot| {
            snapshot.commit_oid.object_format() != snapshot.object_format
                || snapshot.tree_oid.object_format() != snapshot.object_format
        })
    {
        return Err(ArtifactError::Corrupt);
    }
    let repository = evaluation.repository.ok_or(ArtifactError::Corrupt)?;
    let repository = RepositoryIdentity::new(
        repository.host().to_owned(),
        repository.owner().to_owned(),
        repository.name().to_owned(),
    )
    .ok_or(ArtifactError::Corrupt)?;
    let target_ref = evaluation.target_ref;

    Ok(AcceptedReport {
        report_digest: sha256(bytes),
        payload_digest,
        repository,
        target_ref,
        base: AcceptedSnapshot {
            commit: base.commit_oid,
            tree: base.tree_oid,
        },
        candidate: AcceptedSnapshot {
            commit: candidate.commit_oid,
            tree: candidate.tree_oid,
        },
        candidate_identity_digest,
    })
}
