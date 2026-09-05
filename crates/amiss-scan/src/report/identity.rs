use amiss_wire::assessment::Nullable;
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::json::Value;
use amiss_wire::model::BranchRef;
use amiss_wire::report::sandbox_descriptor;
use amiss_wire::requests::{
    CandidateEventKind, CandidateFinality, CandidateIdentity, CandidateIdentitySchema,
    CandidateSnapshot, RequestMode, SnapshotMaterialization,
};

use super::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, SNAPSHOT_SCHEMA, Setup, SnapshotIdentity,
    digest_value, integer, nullable, object, string,
};

fn snapshot_value(snapshot: &SnapshotIdentity) -> Value {
    object(vec![
        ("kind", string("git-commit")),
        ("object_format", string(snapshot.object_format.as_ref())),
        ("commit_oid", string(snapshot.commit_oid.as_str())),
        ("tree_oid", string(snapshot.tree_oid.as_str())),
    ])
}

fn candidate_value(candidate: &CandidateBlock, snapshot_request: Option<Digest>) -> Value {
    match candidate {
        CandidateBlock::Commit(identity) => snapshot_value(identity),
        CandidateBlock::Index(index) => object(vec![
            ("kind", string("index")),
            ("snapshot_schema", string(SNAPSHOT_SCHEMA)),
            ("identity_scope", string("complete-logical-index")),
            (
                "base_object_format",
                string(index.snapshot.base_object_format.as_ref()),
            ),
            (
                "base_commit_oid",
                string(index.snapshot.base_commit_oid.as_str()),
            ),
            (
                "index_projection_digest",
                digest_value(index.snapshot.index_projection_digest),
            ),
            ("entry_count", integer(index.snapshot.entry_count)),
            (
                "snapshot_digest",
                digest_value(index.snapshot.snapshot_digest),
            ),
        ]),
        CandidateBlock::Unavailable(reasons) => object(vec![
            ("kind", string("unavailable")),
            (
                "request_digest",
                snapshot_request.map_or(Value::Null, digest_value),
            ),
            (
                "reasons",
                Value::array(
                    reasons
                        .iter()
                        .map(|reason| string(reason.as_ref()))
                        .collect(),
                ),
            ),
        ]),
    }
}

fn repository_value(identity: &amiss_wire::model::RepositoryIdentity) -> Value {
    object(vec![
        ("host", string(identity.host())),
        ("owner", string(identity.owner())),
        ("name", string(identity.name())),
    ])
}

/// The evaluation's identity rows: everything of the resolved evaluation
/// value that precedes time, in the candidate-identity preimage order.
fn identity_rows(setup: &Setup) -> Vec<(&'static str, Value)> {
    let (mode, event_kind, finality, materialization) = match &setup.candidate {
        CandidateBlock::Commit(_) => (
            "commit-pair",
            "explicit-commit-pair",
            "explicit-replay",
            "git-objects",
        ),
        CandidateBlock::Index(_) | CandidateBlock::Unavailable(_) => {
            ("index", "local-index", "local-nonfinal", "index")
        }
    };
    let skip = match &setup.candidate {
        CandidateBlock::Index(index) => index.skip_worktree_paths,
        CandidateBlock::Commit(_) | CandidateBlock::Unavailable(_) => 0,
    };
    vec![
        ("mode", string(mode)),
        ("event_kind", string(event_kind)),
        ("finality", string(finality)),
        (
            "repository",
            setup
                .repository
                .as_ref()
                .map_or(Value::Null, repository_value),
        ),
        (
            "candidate_ref",
            nullable(setup.candidate_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "target_ref",
            nullable(setup.target_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "default_branch_ref",
            nullable(setup.default_branch_ref.as_ref().map(BranchRef::as_str)),
        ),
        ("base", snapshot_value(&setup.base)),
        (
            "candidate",
            candidate_value(&setup.candidate, setup.requests.snapshot),
        ),
        ("materialization", string(materialization)),
        ("skip_worktree_paths", integer(skip)),
        ("index_only_materialized_paths", integer(0)),
    ]
}

/// The candidate-identity digest a trusted-time statement must carry: `HJ`
/// over the resolved-evaluation identity, including its forge.
///
/// # Errors
///
/// Returns [`crate::Error::Internal`] when the candidate snapshot is
/// unavailable or its closed serde model cannot be encoded.
pub fn candidate_identity_digest(setup: &Setup) -> Result<Digest, crate::Error> {
    let (mode, event_kind, finality, candidate, materialization, skip_worktree_paths) =
        match &setup.candidate {
            CandidateBlock::Commit(snapshot) => (
                RequestMode::CommitPair,
                CandidateEventKind::ExplicitCommitPair,
                CandidateFinality::ExplicitReplay,
                CandidateSnapshot::Git(snapshot.clone()),
                SnapshotMaterialization::GitObjects,
                0,
            ),
            CandidateBlock::Index(index) => (
                RequestMode::Index,
                CandidateEventKind::LocalIndex,
                CandidateFinality::LocalNonfinal,
                CandidateSnapshot::Index(index.snapshot.clone()),
                SnapshotMaterialization::Index,
                index.skip_worktree_paths,
            ),
            CandidateBlock::Unavailable(_reasons) => return Err(crate::Error::Internal),
        };
    let identity = CandidateIdentity {
        schema: CandidateIdentitySchema::Current,
        mode,
        event_kind,
        finality,
        repository: setup
            .repository
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        candidate_ref: setup
            .candidate_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        target_ref: setup
            .target_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        default_branch_ref: setup
            .default_branch_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        base: setup.base.clone(),
        candidate,
        materialization,
        skip_worktree_paths,
        index_only_materialized_paths: 0,
        forge: setup.forge.map_or(Nullable::Null, Nullable::Value),
    };
    hj_serde(CANDIDATE_IDENTITY_DOMAIN, |writer| {
        serde_json::to_writer(writer, &identity)
    })
    .map_err(|_defect| crate::Error::Internal)
}

pub(super) fn evaluation_value(setup: &Setup) -> Value {
    let mut rows = identity_rows(setup);
    rows.extend([
        (
            "evaluation_instant",
            setup.policy.time.as_ref().map_or(Value::Null, |time| {
                string(time.statement.evaluation_instant.as_str())
            }),
        ),
        ("trusted_time", Value::Bool(setup.policy.time.is_some())),
    ]);
    rows.push((
        "forge",
        setup
            .forge
            .map_or(Value::Null, |dialect| string(dialect.as_ref())),
    ));
    object(rows)
}

fn verified_provenance(control: Option<(Digest, amiss_wire::requests::RequestTrust)>) -> Value {
    control.map_or_else(
        || {
            object(vec![
                ("status", string("none")),
                ("digest", Value::Null),
                ("trust_source", string("none")),
            ])
        },
        |(digest, trust)| {
            object(vec![
                ("status", string("verified")),
                ("digest", digest_value(digest)),
                ("trust_source", string(trust.as_ref())),
            ])
        },
    )
}

pub(super) fn controls_value(setup: &Setup) -> Value {
    if let Some(reason) = setup.controls_unavailable {
        return object(vec![
            ("status", string("unavailable")),
            (
                "request_digest",
                setup.requests.controls.map_or(Value::Null, digest_value),
            ),
            ("reasons", Value::array(vec![string(reason.as_ref())])),
        ]);
    }
    let (descriptor, descriptor_digest) = sandbox_descriptor();
    object(vec![
        ("profile", string(setup.profile.as_ref())),
        (
            "base_repository_policy_digest",
            setup.policy.base_digest.map_or(Value::Null, digest_value),
        ),
        (
            "candidate_repository_policy_digest",
            setup
                .policy
                .candidate_digest
                .map_or(Value::Null, digest_value),
        ),
        (
            "organization_floor",
            verified_provenance(setup.policy.floor),
        ),
        (
            "debt_snapshot",
            verified_provenance(
                setup
                    .policy
                    .debt
                    .as_ref()
                    .map(|debt| (debt.digest, debt.trust_source)),
            ),
        ),
        (
            "waiver_bundle",
            verified_provenance(
                setup
                    .policy
                    .waiver
                    .as_ref()
                    .map(|waiver| (waiver.digest, waiver.trust_source)),
            ),
        ),
        (
            "execution_constraint",
            constraint_value(setup.policy.constraint.as_ref()),
        ),
        (
            "semantic_evidence",
            Value::array(
                setup
                    .policy
                    .semantic_evidence
                    .iter()
                    .map(semantic_evidence_value)
                    .collect(),
            ),
        ),
        (
            "sandbox",
            object(vec![
                ("assurance", string("self-asserted")),
                ("enforcement_source", string("local-process")),
                ("descriptor", descriptor),
                ("descriptor_digest", digest_value(descriptor_digest)),
                ("verification", Value::Null),
            ]),
        ),
        (
            "trusted_time_source",
            setup.policy.time.as_ref().map_or_else(
                || object(vec![("status", string("none"))]),
                |time| {
                    object(vec![
                        ("status", string("verified")),
                        ("statement", time_statement_value(&time.statement)),
                        ("statement_digest", digest_value(time.digest)),
                        ("trust_source", string("external-required-check")),
                    ])
                },
            ),
        ),
    ])
}

fn constraint_value(constraint: Option<&crate::policy::ConstraintContext>) -> Value {
    constraint.map_or_else(
        || object(vec![("status", string("none"))]),
        |constraint| {
            object(vec![
                ("status", string("verified")),
                (
                    "descriptor",
                    constraint_descriptor_value(&constraint.descriptor),
                ),
                ("descriptor_digest", digest_value(constraint.digest)),
                ("trust_source", string(constraint.trust_source.as_ref())),
            ])
        },
    )
}

fn semantic_evidence_value(evidence: &crate::semantic::Provenance) -> Value {
    object(vec![
        ("payload_digest", digest_value(evidence.payload_digest)),
        (
            "producer",
            object(vec![
                ("kind", string(evidence.producer.kind.as_str())),
                ("identity", string(evidence.producer.identity.as_str())),
                ("version", string(&evidence.producer.version)),
                ("input_digest", digest_value(evidence.producer.input_digest)),
            ]),
        ),
    ])
}

fn constraint_descriptor_value(
    descriptor: &amiss_wire::controls::ExecutionConstraintDescriptor,
) -> Value {
    object(vec![
        (
            "schema",
            string(amiss_wire::controls::EXECUTION_CONSTRAINT_SCHEMA),
        ),
        (
            "action_repository",
            repository_value(&descriptor.action_repository),
        ),
        (
            "action_object_format",
            string(descriptor.action_object_format.as_ref()),
        ),
        (
            "action_commit_oid",
            string(descriptor.action_commit_oid.as_str()),
        ),
        (
            "action_tree_oid",
            string(descriptor.action_tree_oid.as_str()),
        ),
        ("manifest_path", string(descriptor.manifest_path.as_str())),
        (
            "release_manifest_digest",
            digest_value(descriptor.release_manifest_digest),
        ),
        (
            "selected_platform",
            string(descriptor.selected_platform.as_ref()),
        ),
        (
            "required_status_name",
            string(&descriptor.required_status_name),
        ),
        (
            "bootstrap_contract",
            string(amiss_wire::controls::ACTION_BOOTSTRAP_CONTRACT),
        ),
        (
            "bootstrap_digest",
            digest_value(descriptor.bootstrap_digest),
        ),
    ])
}

fn time_statement_value(statement: &amiss_wire::controls::TrustedTimeStatement) -> Value {
    let mut rows = vec![
        (
            "schema",
            string(amiss_wire::controls::TRUSTED_TIME_STATEMENT_SCHEMA),
        ),
        (
            "controller",
            string(amiss_wire::controls::TRUSTED_TIME_CONTROLLER),
        ),
        ("repository", repository_value(&statement.repository)),
        ("ref", string(statement.ref_name.as_str())),
        (
            "candidate_identity_digest",
            digest_value(statement.candidate_identity_digest),
        ),
    ];
    rows.push(("provider", string(&statement.provider)));
    rows.extend([
        ("provider_run_id", string(&statement.provider_run_id)),
        (
            "provider_run_attempt",
            integer(statement.provider_run_attempt),
        ),
        (
            "evaluation_instant",
            string(statement.evaluation_instant.as_str()),
        ),
        ("valid_until", string(statement.valid_until.as_str())),
    ]);
    object(rows)
}
