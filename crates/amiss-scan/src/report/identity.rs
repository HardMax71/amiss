use amiss_wire::codec;
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile, TrustedTimeStatement};
use amiss_wire::de::Error;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::{ForgeDialect, RepositoryIdentity, UtcInstant};
use amiss_wire::report::sandbox_descriptor;
use amiss_wire::requests::RequestTrust;
use serde::Serialize;

use super::{CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, SNAPSHOT_SCHEMA, Setup, SnapshotIdentity};

#[derive(Serialize)]
struct Snapshot<'a> {
    commit_oid: &'a str,
    kind: &'static str,
    object_format: &'static str,
    tree_oid: &'a str,
}

fn snapshot(snapshot: &SnapshotIdentity) -> Snapshot<'_> {
    Snapshot {
        commit_oid: &snapshot.commit_oid,
        kind: "git-commit",
        object_format: snapshot.object_format,
        tree_oid: &snapshot.tree_oid,
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Candidate<'a> {
    GitCommit {
        commit_oid: &'a str,
        object_format: &'static str,
        tree_oid: &'a str,
    },
    Index {
        base_commit_oid: &'a str,
        base_object_format: &'static str,
        entry_count: u64,
        identity_scope: &'static str,
        index_projection_digest: Digest,
        snapshot_digest: Digest,
        snapshot_schema: &'static str,
    },
    Unavailable {
        reasons: &'a [&'static str],
        request_digest: Option<Digest>,
    },
}

fn candidate(candidate: &CandidateBlock, request_digest: Option<Digest>) -> Candidate<'_> {
    match candidate {
        CandidateBlock::Commit(identity) => Candidate::GitCommit {
            commit_oid: &identity.commit_oid,
            object_format: identity.object_format,
            tree_oid: &identity.tree_oid,
        },
        CandidateBlock::Index(index) => Candidate::Index {
            base_commit_oid: &index.base_commit_oid,
            base_object_format: index.base_object_format,
            entry_count: index.entry_count,
            identity_scope: "complete-logical-index",
            index_projection_digest: index.projection_digest,
            snapshot_digest: index.snapshot_digest,
            snapshot_schema: SNAPSHOT_SCHEMA,
        },
        CandidateBlock::Unavailable(reasons) => Candidate::Unavailable {
            reasons,
            request_digest,
        },
    }
}

#[derive(Serialize)]
struct Identity<'a> {
    base: Snapshot<'a>,
    candidate: Candidate<'a>,
    candidate_ref: Option<&'a str>,
    default_branch_ref: Option<&'a str>,
    event_kind: &'static str,
    finality: &'static str,
    forge: Option<ForgeDialect>,
    index_only_materialized_paths: u8,
    materialization: &'static str,
    mode: &'static str,
    repository: Option<&'a RepositoryIdentity>,
    skip_worktree_paths: u64,
    target_ref: Option<&'a str>,
}

fn identity(setup: &Setup) -> Identity<'_> {
    let (mode, event_kind, finality, materialization, skip_worktree_paths) = match &setup.candidate
    {
        CandidateBlock::Commit(_) => (
            "commit-pair",
            "explicit-commit-pair",
            "explicit-replay",
            "git-objects",
            0,
        ),
        CandidateBlock::Index(index) => (
            "index",
            "local-index",
            "local-nonfinal",
            "index",
            index.skip_worktree_paths,
        ),
        CandidateBlock::Unavailable(_) => ("index", "local-index", "local-nonfinal", "index", 0),
    };
    Identity {
        base: snapshot(&setup.base),
        candidate: candidate(&setup.candidate, setup.requests.snapshot),
        candidate_ref: setup.candidate_ref.as_deref(),
        default_branch_ref: setup.default_branch_ref.as_deref(),
        event_kind,
        finality,
        forge: setup.forge,
        index_only_materialized_paths: 0,
        materialization,
        mode,
        repository: setup.repository.as_ref(),
        skip_worktree_paths,
        target_ref: setup.target_ref.as_deref(),
    }
}

#[derive(Serialize)]
struct CandidateIdentity<'a> {
    #[serde(flatten)]
    identity: Identity<'a>,
    schema: &'static str,
}

/// The candidate identity bound by trusted time, including the selected forge.
///
/// # Errors
///
/// An identity cannot be represented in the strict JSON profile.
pub fn candidate_identity_digest(setup: &Setup) -> Result<Digest, Error> {
    codec::digest(
        CANDIDATE_IDENTITY_DOMAIN,
        &CandidateIdentity {
            identity: identity(setup),
            schema: CANDIDATE_IDENTITY_DOMAIN,
        },
    )
}

#[derive(Serialize)]
struct EvaluationProjection<'a> {
    evaluation_instant: Option<&'a UtcInstant>,
    #[serde(flatten)]
    identity: Identity<'a>,
    trusted_time: bool,
}

pub(super) fn evaluation_value(setup: &Setup) -> Result<Value, Error> {
    codec::to_value(&EvaluationProjection {
        evaluation_instant: setup
            .policy
            .time
            .as_ref()
            .map(|time| time.statement.evaluation_instant()),
        identity: identity(setup),
        trusted_time: setup.policy.time.is_some(),
    })
}

#[derive(Serialize)]
struct Provenance {
    digest: Option<Digest>,
    status: &'static str,
    trust_source: &'static str,
}

fn verified_provenance(control: Option<(Digest, RequestTrust)>) -> Provenance {
    match control {
        Some((digest, trust)) => Provenance {
            digest: Some(digest),
            status: "verified",
            trust_source: trust.into(),
        },
        None => Provenance {
            digest: None,
            status: "none",
            trust_source: "none",
        },
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum Constraint<'a> {
    None,
    Verified {
        descriptor: &'a ExecutionConstraintDescriptor,
        descriptor_digest: Digest,
        trust_source: &'static str,
    },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum TimeSource<'a> {
    None,
    Verified {
        statement: &'a TrustedTimeStatement,
        statement_digest: Digest,
        trust_source: &'static str,
    },
}

#[derive(Serialize)]
struct Sandbox {
    assurance: &'static str,
    descriptor: Value,
    descriptor_digest: Digest,
    enforcement_source: &'static str,
    verification: (),
}

#[derive(Serialize)]
struct SemanticEvidence<'a> {
    payload_digest: Digest,
    producer: Producer<'a>,
}

#[derive(Serialize)]
struct Producer<'a> {
    identity: &'a str,
    input_digest: Digest,
    kind: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct Controls<'a> {
    base_repository_policy_digest: Option<Digest>,
    candidate_repository_policy_digest: Option<Digest>,
    debt_snapshot: Provenance,
    execution_constraint: Constraint<'a>,
    organization_floor: Provenance,
    profile: Profile,
    sandbox: Sandbox,
    semantic_evidence: Vec<SemanticEvidence<'a>>,
    trusted_time_source: TimeSource<'a>,
    waiver_bundle: Provenance,
}

#[derive(Serialize)]
struct Unavailable {
    reasons: [&'static str; 1],
    request_digest: Option<Digest>,
    status: &'static str,
}

pub(super) fn controls_value(setup: &Setup) -> Result<Value, Error> {
    if let Some(reason) = setup.controls_unavailable {
        return codec::to_value(&Unavailable {
            reasons: [reason],
            request_digest: setup.requests.controls,
            status: "unavailable",
        });
    }
    let (descriptor, descriptor_digest) = sandbox_descriptor()?;
    codec::to_value(&Controls {
        base_repository_policy_digest: setup.policy.base_digest,
        candidate_repository_policy_digest: setup.policy.candidate_digest,
        debt_snapshot: verified_provenance(
            setup
                .policy
                .debt
                .as_ref()
                .map(|debt| (debt.digest, debt.trust_source)),
        ),
        execution_constraint: setup.policy.constraint.as_ref().map_or(
            Constraint::None,
            |(descriptor, trust)| Constraint::Verified {
                descriptor,
                descriptor_digest: descriptor.digest(),
                trust_source: (*trust).into(),
            },
        ),
        organization_floor: verified_provenance(setup.policy.floor),
        profile: setup.profile,
        sandbox: Sandbox {
            assurance: "self-asserted",
            descriptor,
            descriptor_digest,
            enforcement_source: "local-process",
            verification: (),
        },
        semantic_evidence: setup
            .policy
            .semantic_evidence
            .iter()
            .map(|evidence| SemanticEvidence {
                payload_digest: evidence.payload_digest,
                producer: Producer {
                    identity: evidence.producer_identity.as_str(),
                    input_digest: evidence.input_digest,
                    kind: evidence.producer_kind.as_str(),
                    version: &evidence.producer_version,
                },
            })
            .collect(),
        trusted_time_source: setup.policy.time.as_ref().map_or(TimeSource::None, |time| {
            TimeSource::Verified {
                statement: &time.statement,
                statement_digest: time.digest,
                trust_source: "external-required-check",
            }
        }),
        waiver_bundle: verified_provenance(
            setup
                .policy
                .waiver
                .as_ref()
                .map(|waiver| (waiver.digest, waiver.trust_source)),
        ),
    })
}
