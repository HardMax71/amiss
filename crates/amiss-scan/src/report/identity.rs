use super::{BaseBlock, CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, Setup};
use amiss_wire::envelope::document_digest;
use amiss_wire::model::Digest;
use amiss_wire::report::{model, sandbox_descriptor};
use amiss_wire::requests::{
    CandidateEventKind, CandidateFinality, CandidateIdentitySchema, CandidateSnapshot, RequestMode,
    RequestTrust, SnapshotMaterialization,
};

/// The candidate-identity digest a trusted-time statement must carry: `HJ`
/// over the resolved-evaluation identity, including its forge.
///
/// # Errors
///
/// Returns [`crate::Error::Internal`] when the candidate snapshot is
/// unavailable or its closed serde model cannot be encoded.
pub fn candidate_identity_digest(setup: &Setup) -> Result<Digest, crate::Error> {
    let evaluation = evaluation(setup);
    let (model::BaseSnapshot::Git(_), model::Snapshot::Available(_)) =
        (&evaluation.base, &evaluation.candidate)
    else {
        return Err(crate::Error::Internal);
    };
    let identity = model::IdentityPreimage {
        evaluation: &evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    document_digest(CANDIDATE_IDENTITY_DOMAIN, &identity).ok_or(crate::Error::Internal)
}

pub(super) fn evaluation(setup: &Setup) -> model::ResolvedEvaluation {
    let (mode, event_kind, finality, materialization) = match setup.candidate {
        CandidateBlock::Commit(_) | CandidateBlock::CommitUnavailable(_) => (
            RequestMode::CommitPair,
            CandidateEventKind::ExplicitCommitPair,
            CandidateFinality::ExplicitReplay,
            SnapshotMaterialization::GitObjects,
        ),
        CandidateBlock::Index(_) | CandidateBlock::Unavailable(_) => (
            RequestMode::Index,
            CandidateEventKind::LocalIndex,
            CandidateFinality::LocalNonfinal,
            SnapshotMaterialization::Index,
        ),
    };
    let (base, candidate, skip_worktree_paths) = snapshots(setup);
    model::ResolvedEvaluation {
        mode,
        event_kind,
        finality,
        repository: setup.repository.clone(),
        candidate_ref: setup.candidate_ref.clone(),
        target_ref: setup.target_ref.clone(),
        default_branch_ref: setup.default_branch_ref.clone(),
        base,
        candidate,
        materialization,
        skip_worktree_paths,
        index_only_materialized_paths: 0,
        evaluation_instant: setup
            .policy
            .time
            .as_ref()
            .map(|time| time.statement.evaluation_instant.clone()),
        trusted_time: setup.policy.time.is_some(),
        forge: setup.forge,
    }
}

/// Each side as the report spells it, a side that never resolved spelled
/// unavailable, with the index candidate's skip-worktree count.
fn snapshots(setup: &Setup) -> (model::BaseSnapshot, model::Snapshot, u64) {
    let (candidate, skip_worktree_paths) = match &setup.candidate {
        CandidateBlock::Commit(snapshot) => (
            model::Snapshot::Available(CandidateSnapshot::Git(snapshot.clone())),
            0,
        ),
        CandidateBlock::Index(index) => (
            model::Snapshot::Available(CandidateSnapshot::Index(index.snapshot.clone())),
            index.skip_worktree_paths,
        ),
        CandidateBlock::Unavailable(reasons) | CandidateBlock::CommitUnavailable(reasons) => {
            (model::Snapshot::Unavailable(unavailable(setup, reasons)), 0)
        }
    };
    let base = match &setup.base {
        BaseBlock::Commit(snapshot) => model::BaseSnapshot::Git(snapshot.clone()),
        BaseBlock::Unavailable(reasons) => {
            model::BaseSnapshot::Unavailable(unavailable(setup, reasons))
        }
    };
    (base, candidate, skip_worktree_paths)
}

fn unavailable(
    setup: &Setup,
    reasons: &[model::SnapshotUnavailableReason],
) -> model::UnavailableSnapshot {
    model::UnavailableSnapshot {
        kind: model::UnavailableSnapshotKind::Unavailable,
        request_digest: setup.requests.snapshot,
        reasons: reasons.to_vec(),
    }
}

pub(super) fn controls(setup: &Setup) -> Result<model::Controls, crate::Error> {
    if let Some(reason) = setup.controls_unavailable {
        return Ok(model::Controls::Unavailable(model::UnavailableControls {
            status: model::UnavailableStatus::Unavailable,
            request_digest: setup.requests.controls,
            reasons: vec![reason],
        }));
    }
    let provenance = |control: Option<(Digest, RequestTrust)>| {
        let (status, trust_source) = match control {
            Some((_, RequestTrust::ExternalRequiredCheck)) => (
                model::ControlStatus::Verified,
                model::ControlTrustSource::ExternalRequiredCheck,
            ),
            Some((_, RequestTrust::OrganizationPolicy)) => (
                model::ControlStatus::Verified,
                model::ControlTrustSource::OrganizationPolicy,
            ),
            None => (model::ControlStatus::None, model::ControlTrustSource::None),
        };
        model::ControlProvenance {
            digest: control.map(|(digest, _)| digest),
            status,
            trust_source,
        }
    };
    let (descriptor, descriptor_digest) =
        sandbox_descriptor().map_err(|_defect| crate::Error::Internal)?;
    Ok(model::Controls::Resolved(Box::new(
        model::ResolvedControls {
            profile: setup.profile,
            base_repository_policy_digest: setup.policy.base_digest,
            candidate_repository_policy_digest: setup.policy.candidate_digest,
            organization_floor: provenance(setup.policy.floor),
            debt_snapshot: provenance(
                setup
                    .policy
                    .debt
                    .as_ref()
                    .map(|debt| (debt.digest, debt.trust_source)),
            ),
            waiver_bundle: provenance(
                setup
                    .policy
                    .waiver
                    .as_ref()
                    .map(|waiver| (waiver.digest, waiver.trust_source)),
            ),
            execution_constraint: setup.policy.constraint.as_ref().map_or_else(
                || {
                    model::ExecutionConstraintProvenance::None(model::NoExecutionConstraint {
                        status: model::NoControlStatus::None,
                    })
                },
                |constraint| {
                    model::ExecutionConstraintProvenance::Verified(Box::new(
                        model::VerifiedExecutionConstraint {
                            status: model::VerifiedControlStatus::Verified,
                            descriptor: constraint.descriptor.clone(),
                            descriptor_digest: constraint.digest,
                            trust_source: constraint.trust_source,
                        },
                    ))
                },
            ),
            semantic_evidence: Some(setup.policy.semantic_evidence.clone()),
            sandbox: model::SandboxProvenance {
                assurance: model::SandboxAssurance::SelfAsserted,
                enforcement_source: model::SandboxEnforcementSource::LocalProcess,
                descriptor,
                descriptor_digest,
                verification: None,
            },
            trusted_time_source: setup.policy.time.as_ref().map_or_else(
                || {
                    model::TrustedTimeProvenance::None(model::NoTrustedTime {
                        status: model::NoControlStatus::None,
                    })
                },
                |time| {
                    model::TrustedTimeProvenance::Verified(Box::new(model::VerifiedTrustedTime {
                        status: model::VerifiedControlStatus::Verified,
                        statement: time.statement.clone(),
                        statement_digest: time.digest,
                        trust_source: model::TrustedTimeTrustSource::ExternalRequiredCheck,
                    }))
                },
            ),
        },
    )))
}
