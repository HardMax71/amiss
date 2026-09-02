use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::report::sandbox_descriptor;

use super::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, SNAPSHOT_SCHEMA, Setup, SnapshotIdentity,
    digest_value, integer, nullable, object, string,
};

fn snapshot_value(snapshot: &SnapshotIdentity) -> Value {
    object(vec![
        ("kind", string("git-commit")),
        ("object_format", string(snapshot.object_format)),
        ("commit_oid", string(&snapshot.commit_oid)),
        ("tree_oid", string(&snapshot.tree_oid)),
    ])
}

fn candidate_value(candidate: &CandidateBlock, snapshot_request: Option<Digest>) -> Value {
    match candidate {
        CandidateBlock::Commit(identity) => snapshot_value(identity),
        CandidateBlock::Index(index) => object(vec![
            ("kind", string("index")),
            ("snapshot_schema", string(SNAPSHOT_SCHEMA)),
            ("identity_scope", string("complete-logical-index")),
            ("base_object_format", string(index.base_object_format)),
            ("base_commit_oid", string(&index.base_commit_oid)),
            (
                "index_projection_digest",
                digest_value(index.projection_digest),
            ),
            ("entry_count", integer(index.entry_count)),
            ("snapshot_digest", digest_value(index.snapshot_digest)),
        ]),
        CandidateBlock::Unavailable(reasons) => object(vec![
            ("kind", string("unavailable")),
            (
                "request_digest",
                snapshot_request.map_or(Value::Null, digest_value),
            ),
            (
                "reasons",
                Value::array(reasons.iter().map(|reason| string(reason)).collect()),
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
        ("candidate_ref", nullable(setup.candidate_ref.as_deref())),
        ("target_ref", nullable(setup.target_ref.as_deref())),
        (
            "default_branch_ref",
            nullable(setup.default_branch_ref.as_deref()),
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

/// The rolling candidate identity. The selected forge is resolution-significant,
/// so it is bound alongside the repository and snapshots.
fn candidate_identity_value(setup: &Setup) -> Value {
    identity_value(
        setup,
        vec![("schema", string(CANDIDATE_IDENTITY_DOMAIN))],
        Vec::new(),
    )
}

/// The candidate-identity digest a trusted-time statement must carry: `HJ`
/// over the resolved-evaluation identity, including its forge.
#[must_use]
pub fn candidate_identity_digest(setup: &Setup) -> Digest {
    hj(CANDIDATE_IDENTITY_DOMAIN, &candidate_identity_value(setup))
}

pub(super) fn evaluation_value(setup: &Setup) -> Value {
    identity_value(
        setup,
        Vec::new(),
        vec![
            (
                "evaluation_instant",
                setup.policy.time.as_ref().map_or(Value::Null, |time| {
                    string(time.statement.evaluation_instant.as_str())
                }),
            ),
            ("trusted_time", Value::Bool(setup.policy.time.is_some())),
        ],
    )
}

fn identity_value(
    setup: &Setup,
    mut rows: Vec<(&'static str, Value)>,
    before_forge: Vec<(&'static str, Value)>,
) -> Value {
    rows.extend(identity_rows(setup));
    rows.extend(before_forge);
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
            ("reasons", Value::array(vec![string(reason)])),
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
            setup.policy.constraint.as_ref().map_or_else(
                || object(vec![("status", string("none"))]),
                |(descriptor, trust)| {
                    object(vec![
                        ("status", string("verified")),
                        ("descriptor", constraint_descriptor_value(descriptor)),
                        ("descriptor_digest", digest_value(descriptor.digest())),
                        ("trust_source", string(trust.as_ref())),
                    ])
                },
            ),
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

fn semantic_evidence_value(evidence: &crate::semantic::Provenance) -> Value {
    object(vec![
        ("payload_digest", digest_value(evidence.payload_digest)),
        (
            "producer",
            object(vec![
                ("kind", string(evidence.producer_kind.as_str())),
                ("identity", string(evidence.producer_identity.as_str())),
                ("version", string(&evidence.producer_version)),
                ("input_digest", digest_value(evidence.input_digest)),
            ]),
        ),
    ])
}

fn constraint_descriptor_value(
    descriptor: &amiss_wire::controls::ExecutionConstraintDescriptor,
) -> Value {
    object(vec![
        ("schema", string("amiss/scanner-execution-constraint")),
        (
            "action_repository",
            repository_value(descriptor.action_repository()),
        ),
        (
            "action_object_format",
            string(descriptor.action_object_format().as_ref()),
        ),
        (
            "action_commit_oid",
            string(descriptor.action_commit_oid().as_str()),
        ),
        (
            "action_tree_oid",
            string(descriptor.action_tree_oid().as_str()),
        ),
        ("manifest_path", string(descriptor.manifest_path().as_str())),
        (
            "release_manifest_digest",
            digest_value(descriptor.release_manifest_digest()),
        ),
        (
            "selected_platform",
            string(descriptor.selected_platform().as_ref()),
        ),
        (
            "required_status_name",
            string(descriptor.required_status_name()),
        ),
        ("bootstrap_contract", string("amiss-action-bootstrap")),
        (
            "bootstrap_digest",
            digest_value(descriptor.bootstrap_digest()),
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
