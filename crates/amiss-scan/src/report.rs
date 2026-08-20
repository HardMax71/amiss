mod build;
mod documents;
mod summary;

pub use build::{construct, construct_incomplete};

use amiss_wire::controls::Profile;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::RepoPath;
use amiss_wire::report::{EngineProvenance, FindingScope, sandbox_descriptor};
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;

use crate::correlate::{Comparison, Observation};
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{DocumentInput, DocumentSide, Finding, FindingFact, FindingFix};
use crate::feedback;
use crate::{SpanDisplay, observe};

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot";

/// The canonical logical-index projection and the synthetic snapshot input
/// built over it, with both digests.
#[must_use]
pub fn synthetic_candidate(
    base_object_format: &'static str,
    base_commit_oid: &str,
    entries: &[(RepoPath, amiss_wire::controls::GitMode, String, bool)],
    skip_worktree_paths: u64,
) -> IndexCandidate {
    let rows: Vec<Value> = entries
        .iter()
        .map(|(path, mode, oid, skip)| {
            let entry_kind = match mode {
                amiss_wire::controls::GitMode::Symlink => "symlink",
                amiss_wire::controls::GitMode::Gitlink => "gitlink",
                amiss_wire::controls::GitMode::RegularFile
                | amiss_wire::controls::GitMode::ExecutableFile
                | amiss_wire::controls::GitMode::Tree => "blob",
            };
            object(vec![
                ("path", path.to_value()),
                ("entry_kind", string(entry_kind)),
                ("git_mode", string(mode.as_str())),
                ("object_format", string(base_object_format)),
                ("object_oid", string(oid)),
                ("skip_worktree", Value::Bool(*skip)),
            ])
        })
        .collect();
    let projection = object(vec![
        ("schema", string(INDEX_PROJECTION_SCHEMA)),
        ("entries", Value::array(rows)),
    ]);
    let projection_digest = hj(INDEX_PROJECTION_SCHEMA, &projection);
    let snapshot_input = object(vec![
        ("schema", string(SNAPSHOT_SCHEMA)),
        ("kind", string("index")),
        ("identity_scope", string("complete-logical-index")),
        ("base_object_format", string(base_object_format)),
        ("base_commit_oid", string(base_commit_oid)),
        ("index_projection_digest", digest_value(projection_digest)),
    ]);
    let snapshot_digest = hj(SNAPSHOT_SCHEMA, &snapshot_input);
    IndexCandidate {
        base_object_format,
        base_commit_oid: base_commit_oid.to_owned(),
        projection_digest,
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        snapshot_digest,
        skip_worktree_paths,
    }
}

/// One snapshot's identity in the evaluation block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotIdentity {
    pub object_format: &'static str,
    pub commit_oid: String,
    pub tree_oid: String,
}

/// The candidate side of the evaluation identity: a Git commit, the
/// synthetic complete logical staged index, or the unavailable projection an
/// incomplete index run reports with its closed reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateBlock {
    Commit(SnapshotIdentity),
    Index(IndexCandidate),
    Unavailable(Vec<&'static str>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexCandidate {
    pub base_object_format: &'static str,
    pub base_commit_oid: String,
    pub projection_digest: Digest,
    pub entry_count: u64,
    pub snapshot_digest: Digest,
    pub skip_worktree_paths: u64,
}

/// The diagnostic request digests of the wrapper lane: present exactly for
/// streams captured completely, and rendered only inside unavailable
/// snapshot and controls values. The in-process CLI has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestDigests {
    pub evaluation: Option<Digest>,
    pub snapshot: Option<Digest>,
    pub controls: Option<Digest>,
}

/// The run identity a complete local report carries, plus the acquired
/// policy effects and, for an invalid-policy run, the unavailable-controls
/// reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub engine: EngineProvenance,
    pub profile: Profile,
    pub repository: Option<amiss_wire::model::RepositoryIdentity>,
    pub forge: Option<amiss_wire::model::ForgeDialect>,
    pub candidate_ref: Option<String>,
    pub target_ref: Option<String>,
    pub default_branch_ref: Option<String>,
    pub base: SnapshotIdentity,
    pub candidate: CandidateBlock,
    pub policy: crate::policy::Effects,
    pub controls_unavailable: Option<&'static str>,
    pub requests: RequestDigests,
}

/// A constructed report: the envelope value, the payload digest, and the
/// result the process must exit with. The wire is never materialized here;
/// a binary streams the envelope through its reserved fatal serializer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub envelope: Value,
    pub payload_digest: Digest,
    pub status: &'static str,
    pub exit_code: i64,
}

impl Built {
    /// The exact report wire, `JCS(envelope) || LF`, for callers that must
    /// hold the bytes.
    #[must_use]
    pub fn wire(&self) -> Vec<u8> {
        let mut wire = canonical(&self.envelope);
        wire.push(b'\n');
        wire
    }
}

fn string(text: &str) -> Value {
    Value::string(text.to_owned())
}

fn nullable(text: Option<&str>) -> Value {
    text.map_or(Value::Null, string)
}

fn nullable_path(path: Option<&RepoPath>) -> Value {
    path.map_or(Value::Null, RepoPath::to_value)
}

fn integer(value: u64) -> Value {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

fn digest_value(digest: Digest) -> Value {
    Value::string(digest.to_string())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn source_span_value(span: (usize, usize), display: SpanDisplay) -> Value {
    object(vec![
        (
            "start_byte",
            integer(u64::try_from(span.0).unwrap_or(u64::MAX)),
        ),
        (
            "end_byte",
            integer(u64::try_from(span.1).unwrap_or(u64::MAX)),
        ),
        ("start_line", integer(display.start_line)),
        ("start_column", integer(display.start_column)),
        ("end_line", integer(display.end_line)),
        ("end_column", integer(display.end_column)),
    ])
}

fn occurrence_value(observation: &Observation) -> Value {
    let identity = observe::ObservationIdentity {
        adapter: observation.adapter,
        contract_digest: observation.adapter_contract_digest,
        document: &observation.document,
        construct: observation.construct,
        node_path: &observation.node_path,
        projection_digest: observation.projection_digest,
        intent: &observation.intent,
        raw_destination_digest: observation.raw_destination_digest,
    };
    let id = observe::observation_digest(&identity);
    let input = observe::observation_input(&identity);
    let resolution = crate::evaluate::resolution_row(&observation.resolution);
    let mut members = vec![
        ("observation_id", digest_value(id)),
        ("observation_id_input", input),
        ("adapter_id", string(observation.adapter.adapter_id())),
        ("document", observation.document.to_value()),
        ("source_construct", string(observation.construct.as_str())),
        (
            "source_span",
            source_span_value(observation.span, observation.display),
        ),
        ("block_kind", string(observation.block_kind.as_str())),
        (
            "source_projection_digest",
            digest_value(observation.projection_digest),
        ),
        (
            "intent",
            observe::intent_value(&observation.intent, observation.raw_destination_digest),
        ),
        ("resolution", resolution),
    ];
    if let Some(destination) = &observation.external_destination {
        members.push(("external_destination", string(destination)));
    }
    object(members)
}

fn comparison_value(comparison: &Comparison) -> Value {
    let side = |observation: &Option<Observation>| {
        observation.as_ref().map_or(Value::Null, occurrence_value)
    };
    let list =
        |members: &[Observation]| Value::array(members.iter().map(occurrence_value).collect());
    object(vec![
        ("base", side(&comparison.base)),
        ("candidate", side(&comparison.candidate)),
        ("correlation", string(comparison.outcome.as_ref())),
        ("correlation_reason", string(comparison.reason.as_str())),
        (
            "alternatives",
            object(vec![
                ("base", list(&comparison.alternatives_base)),
                ("candidate", list(&comparison.alternatives_candidate)),
            ]),
        ),
        ("source_change", string(comparison.source_change.as_ref())),
        ("target_change", string(comparison.target_change.as_ref())),
        ("impact", string(comparison.impact.as_ref())),
    ])
}

fn document_input(paired: &documents::PairedDocument<'_>) -> DocumentInput {
    let side = |record: Option<&DocumentRecord>| {
        record.map(|record| match &record.status {
            DocumentStatus::Scanned(scanned) => DocumentSide::Scanned {
                mdx_regions: u64::try_from(scanned.opaque.mdx.len()).unwrap_or(u64::MAX),
                html_regions: u64::try_from(scanned.opaque.html.len()).unwrap_or(u64::MAX),
                extracted_references: u64::try_from(scanned.occurrences.len()).unwrap_or(u64::MAX),
            },
            DocumentStatus::Unsupported(_) | DocumentStatus::Failed(_) => DocumentSide::Unsupported,
            DocumentStatus::ExcludedBuiltIn => DocumentSide::ExcludedBuiltIn,
        })
    };
    DocumentInput {
        path: paired.path.clone(),
        base: side(paired.base),
        candidate: side(paired.candidate),
    }
}

fn finding_value(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, Value)]; 2],
    document_rows: &[(RepoPath, Value)],
) -> Value {
    let kind = finding.kind();
    let scope = kind.scope();
    let coverage = match scope {
        FindingScope::Control => "control-plane",
        FindingScope::Reference | FindingScope::Observation | FindingScope::Document => "none",
    };
    let candidate_fact = finding
        .candidate_fact()
        .cloned()
        .or_else(|| nonreference_fact(finding, comparison_runs, document_rows));
    let fact_pair = |fact: Option<&FindingFact>| {
        (
            fact.map_or(Value::Null, |fact| digest_value(fact.digest())),
            fact.map_or(Value::Null, |fact| fact.value().clone()),
        )
    };
    let (base_digest, base_fact) = fact_pair(finding.base_fact());
    let (candidate_digest, candidate_fact_value) = fact_pair(candidate_fact.as_ref());
    let trace = trace_value(finding);
    let location_span = location_span_value(finding);
    object(vec![
        ("key_input", finding.key().to_value()),
        ("finding_key", digest_value(finding.key().digest())),
        ("kind", string(kind.as_ref())),
        ("description", string(kind.meaning())),
        ("fix", finding.fix().map_or(Value::Null, fix_value)),
        ("coverage_requirement", string(coverage)),
        ("evidence_class", string(kind.evidence_class())),
        ("invariant_class", string(kind.invariant_class())),
        ("attribution", string(finding.attribution.as_str())),
        ("base_fact_digest", base_digest),
        ("base_fact", base_fact),
        ("candidate_fact_digest", candidate_digest),
        ("candidate_fact", candidate_fact_value),
        (
            "aggregation",
            object(vec![
                ("strategy", string("one-per-finding-key")),
                ("member_count", integer(finding.member_count)),
                ("locations_omitted", integer(0)),
                (
                    "representative_rule",
                    string("lowest-location-then-observation-id"),
                ),
            ]),
        ),
        (
            "location",
            object(vec![
                ("side", string(finding.location.side.as_ref())),
                ("path", nullable_path(finding.location.path.as_ref())),
                ("span", location_span),
            ]),
        ),
        (
            "observation_ids",
            Value::array(
                finding
                    .observation_ids
                    .iter()
                    .map(|id| digest_value(*id))
                    .collect(),
            ),
        ),
        (
            "configured_disposition",
            string(finding.configured_disposition.as_ref()),
        ),
        (
            "effective_disposition",
            string(finding.effective_disposition.as_ref()),
        ),
        ("policy_trace", Value::array(trace)),
        (
            "debt",
            finding
                .debt
                .as_ref()
                .map_or(Value::Null, debt_application_value),
        ),
        (
            "waiver",
            finding
                .waiver
                .as_ref()
                .map_or(Value::Null, waiver_application_value),
        ),
    ])
}

fn feedback_value(projected: feedback::Feedback) -> Value {
    let items = projected
        .items
        .into_iter()
        .map(|item| {
            let annotation = item.annotation.map_or(Value::Null, |annotation| {
                object(vec![
                    ("path", string(&annotation.path)),
                    (
                        "span",
                        source_span_value(annotation.span, annotation.display),
                    ),
                ])
            });
            object(vec![
                ("action", string(item.action.as_str())),
                ("target", nullable_path(item.target.as_ref())),
                (
                    "finding_kinds",
                    Value::array(
                        item.finding_kinds
                            .into_iter()
                            .map(|kind| string(kind.as_ref()))
                            .collect(),
                    ),
                ),
                ("location_count", integer(item.location_count)),
                (
                    "effective_disposition",
                    string(item.effective_disposition.as_ref()),
                ),
                ("annotation", annotation),
            ])
        })
        .collect();
    object(vec![
        ("status", string("available")),
        ("items", Value::array(items)),
        ("existing_count", integer(projected.existing_count)),
    ])
}

fn unavailable_feedback_value() -> Value {
    object(vec![("status", string("unavailable"))])
}

fn run_feedback_value(complete: bool, findings: &[Finding], comparisons: &[Comparison]) -> Value {
    if complete {
        feedback_value(feedback::project(findings, comparisons))
    } else {
        unavailable_feedback_value()
    }
}

fn tree_identity_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    object(vec![
        ("object_format", string(tree.object_format().as_str())),
        ("tree_oid", string(tree.tree_oid())),
    ])
}

fn debt_application_value(applied: &crate::evaluate::DebtApplied) -> Value {
    object(vec![
        ("debt_id", string(applied.item.debt_id.as_str())),
        (
            "debt_snapshot_digest",
            digest_value(applied.snapshot_digest),
        ),
        ("adoption_tree", tree_identity_value(&applied.adoption_tree)),
        (
            "accepted_fact_digest",
            digest_value(applied.item.accepted_fact_digest),
        ),
        ("owner", string(applied.item.owner.as_str())),
        ("reason", string(&applied.item.reason)),
        ("created_at", string(applied.item.created_at.as_str())),
        ("expires_at", string(applied.item.expires_at.as_str())),
    ])
}

fn waiver_application_value(applied: &crate::evaluate::WaiverApplied) -> Value {
    object(vec![
        ("waiver_id", string(applied.item.waiver_id.as_str())),
        ("waiver_bundle_digest", digest_value(applied.bundle_digest)),
        (
            "candidate_tree",
            tree_identity_value(&applied.item.candidate_tree),
        ),
        (
            "authorized_fact_digest",
            digest_value(applied.item.authorized_fact_digest),
        ),
        ("owner", string(applied.item.owner.as_str())),
        ("issuer", string(applied.item.issuer.as_str())),
        ("reason", string(&applied.item.reason)),
        ("created_at", string(applied.item.created_at.as_str())),
        ("not_before", string(applied.item.not_before.as_str())),
        ("expires_at", string(applied.item.expires_at.as_str())),
        ("residual_disposition", string("warn")),
    ])
}

/// The policy trace renders the finding's exact step chain.
fn trace_value(finding: &Finding) -> Vec<Value> {
    finding
        .steps
        .iter()
        .map(|step| {
            object(vec![
                ("source", string(step.source)),
                ("rule_id", string(&step.rule_id)),
                ("before", string(step.before.as_ref())),
                ("after", string(step.after.as_ref())),
            ])
        })
        .collect()
}

fn location_span_value(finding: &Finding) -> Value {
    finding.location.span.map_or(Value::Null, |span| {
        let display = finding.location.display.unwrap_or(SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        });
        source_span_value(span, display)
    })
}

fn fix_value(fix: &FindingFix) -> Value {
    object(vec![
        ("path", fix.path.to_value()),
        (
            "span",
            object(vec![
                (
                    "start_byte",
                    integer(u64::try_from(fix.span.0).unwrap_or(u64::MAX)),
                ),
                (
                    "end_byte",
                    integer(u64::try_from(fix.span.1).unwrap_or(u64::MAX)),
                ),
            ]),
        ),
        ("replacement", string(&fix.replacement)),
        ("description", string(fix.kind.meaning())),
    ])
}

/// A nonreference finding carries exactly one candidate fact embedding the
/// full constructed comparison or document row it was derived from.
fn nonreference_fact(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, Value)]; 2],
    document_rows: &[(RepoPath, Value)],
) -> Option<FindingFact> {
    let evidence = match finding.kind().scope() {
        FindingScope::Reference | FindingScope::Control => return None,
        FindingScope::Observation => {
            let id = finding.observation_ids.first()?;
            let row = comparison_runs.into_iter().find_map(|rows| {
                rows.binary_search_by_key(&Some(*id), |(primary, _)| *primary)
                    .ok()
                    .and_then(|index| rows.get(index))
                    .map(|(_, value)| value.clone())
            })?;
            object(vec![("kind", string("observation")), ("comparison", row)])
        }
        FindingScope::Document => {
            let path = finding.location.path.as_ref()?;
            let row = document_rows
                .binary_search_by(|(document, _)| document.cmp(path))
                .ok()
                .and_then(|index| document_rows.get(index))
                .map(|(_, value)| value.clone())?;
            object(vec![("kind", string("document")), ("document_result", row)])
        }
    };
    Some(FindingFact::new(finding.key(), evidence))
}

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
            setup.repository.as_ref().map_or(Value::Null, |identity| {
                object(vec![
                    ("host", string(identity.host())),
                    ("owner", string(identity.owner())),
                    ("name", string(identity.name())),
                ])
            }),
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

fn evaluation_value(setup: &Setup) -> Value {
    identity_value(
        setup,
        Vec::new(),
        vec![
            (
                "evaluation_instant",
                setup.policy.time.as_ref().map_or(Value::Null, |time| {
                    string(time.statement.evaluation_instant().as_str())
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
            .map_or(Value::Null, |dialect| string(dialect.as_str())),
    ));
    object(rows)
}

fn verified_provenance(control: Option<(Digest, &'static str)>) -> Value {
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
                ("trust_source", string(trust)),
            ])
        },
    )
}

fn controls_value(setup: &Setup) -> Value {
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
        ("profile", string(setup.profile.as_str())),
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
                        ("trust_source", string(trust)),
                    ])
                },
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

fn constraint_descriptor_value(
    descriptor: &amiss_wire::controls::ExecutionConstraintDescriptor,
) -> Value {
    object(vec![
        ("schema", string("amiss/scanner-execution-constraint")),
        (
            "action_repository",
            object(vec![
                ("host", string(descriptor.action_repository().host())),
                ("owner", string(descriptor.action_repository().owner())),
                ("name", string(descriptor.action_repository().name())),
            ]),
        ),
        (
            "action_object_format",
            string(descriptor.action_object_format().as_str()),
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
            string(descriptor.selected_platform().as_str()),
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
        ("schema", string(statement.schema())),
        ("controller", string(statement.controller())),
        (
            "repository",
            object(vec![
                ("host", string(statement.repository().host())),
                ("owner", string(statement.repository().owner())),
                ("name", string(statement.repository().name())),
            ]),
        ),
        ("ref", string(statement.ref_name().as_str())),
        (
            "candidate_identity_digest",
            digest_value(statement.candidate_identity_digest()),
        ),
    ];
    rows.push(("provider", string(statement.provider())));
    rows.extend([
        ("provider_run_id", string(statement.provider_run_id())),
        (
            "provider_run_attempt",
            integer(statement.provider_run_attempt()),
        ),
        (
            "evaluation_instant",
            string(statement.evaluation_instant().as_str()),
        ),
        ("valid_until", string(statement.valid_until().as_str())),
    ]);
    object(rows)
}
