use sha2::Digest as _;
use std::collections::BTreeMap;

use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{
    Comparison, Impact, Observation, Outcome, Reason, SourceChange, TargetChange,
};
use amiss_scan::evaluate::evaluate;
use amiss_scan::policy::{DebtContext, Effects, TimeContext};
use amiss_scan::resolve::Intent;
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::{
    DebtItem, EligibleFindingKind, Fact, FactEvidence, FactEvidenceKind, FactSchema,
    FindingKeyInput, FindingKeyInputSchema, FindingOccurrence, FindingScope, MissingResolution,
    OccurrenceKind, ReferenceScopeKind, SourceConstruct, StructuralResolution, TargetIntent,
    TargetIntentKind, TargetKind, TrustedTimeController, TrustedTimeSchema, TrustedTimeStatement,
};
use amiss_wire::model::{
    Adapter, ArtifactId, BranchRef, ObjectFormat, OwnerId, RepoPath, RepoPathText,
    RepositoryIdentity, TreeIdentity, UtcInstant,
};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{Missing, Resolution};

pub(super) fn exception_fixture(count: usize) -> (Vec<Comparison>, Effects) {
    let mut controls = BTreeMap::new();
    let comparisons: Vec<Comparison> = (0..count)
        .map(|index| {
            let (observation, fact) = exception_observation(index);
            controls.insert(observation.id, fact);
            Comparison {
                outcome: Outcome::None,
                reason: Reason::NewObservation,
                source_change: SourceChange::Added,
                base: None,
                candidate: Some(observation),
                alternatives_base: Vec::new(),
                alternatives_candidate: Vec::new(),
                target_change: TargetChange::NotComparable,
                impact: Impact::NewObservation,
            }
        })
        .collect();
    let findings = evaluate(&[], &comparisons, amiss_wire::controls::Profile::Enforce)
        .unwrap_or_else(|error| panic!("benchmark evaluation: {error:?}"));
    assert_eq!(findings.len(), count, "one structural finding per target");
    let items = findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            let observation_id = finding
                .observation_ids
                .first()
                .copied()
                .unwrap_or_else(|| panic!("benchmark finding observation"));
            let accepted_fact = controls
                .remove(&observation_id)
                .unwrap_or_else(|| panic!("benchmark control projection"));
            let accepted_fact_digest = finding
                .candidate_fact
                .as_ref()
                .map_or_else(|| panic!("benchmark candidate fact"), |fact| fact.digest);
            DebtItem {
                debt_id: artifact_id(format!("bench/debt-{index:05}")),
                finding_key: finding.finding_key,
                accepted_fact,
                accepted_fact_digest,
                owner: owner_id("team:benchmark"),
                reason: "Exception target lookup benchmark.".to_owned(),
                created_at: instant("2026-07-01T00:00:00Z"),
                expires_at: instant("2026-08-01T00:00:00Z"),
            }
        })
        .collect();
    let debt_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/bench-debt-context")
            .chain_update([0_u8])
            .chain_update(b"matching debt items")
            .finalize()
            .0,
    );
    let policy = Effects {
        debt: Some(DebtContext {
            digest: debt_digest,
            trust_source: amiss_wire::requests::RequestTrust::OrganizationPolicy,
            adoption_tree: tree("a"),
            items,
        }),
        time: Some(trusted_time()),
        ..Effects::default()
    };
    (comparisons, policy)
}

fn trusted_time() -> TimeContext {
    let statement = TrustedTimeStatement {
        schema: TrustedTimeSchema::Current,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        repository: RepositoryIdentity::github("bench".to_owned(), "docs".to_owned())
            .unwrap_or_else(|| panic!("benchmark repository identity")),
        ref_name: BranchRef::new("refs/heads/main".to_owned())
            .unwrap_or_else(|| panic!("benchmark branch")),
        candidate_identity_digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/bench-candidate-identity")
                .chain_update([0_u8])
                .chain_update(b"candidate")
                .finalize()
                .0,
        ),
        provider: "github-actions".to_owned(),
        provider_run_id: "1".to_owned(),
        provider_run_attempt: 1,
        evaluation_instant: instant("2026-07-12T10:00:00Z"),
        valid_until: instant("2026-07-12T10:05:00Z"),
    };
    statement
        .validate()
        .unwrap_or_else(|error| panic!("benchmark trusted time: {error}"));
    let mut writer = digest_io::IoWrapper(
        sha2::Sha256::new_with_prefix(amiss_wire::controls::TRUSTED_TIME_STATEMENT_SCHEMA)
            .chain_update([0_u8]),
    );
    serde_json_canonicalizer::to_writer(&statement, &mut writer)
        .unwrap_or_else(|error| panic!("benchmark trusted time: {error}"));
    let time_digest = amiss_wire::model::Digest::from(writer.0.finalize().0);
    TimeContext {
        statement,
        digest: time_digest,
    }
}

fn exception_observation(index: usize) -> (Observation, Fact) {
    let token = format!("{index:05}");
    let document_text = "docs/references.md".to_owned();
    let target_text = format!("targets/{token}.rs");
    let document = repo_path(document_text.clone());
    let target = repo_path(target_text.clone());
    let projection_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-source-projection")
            .chain_update([0_u8])
            .chain_update(b"reference")
            .finalize()
            .0,
    );
    let fact = Fact {
        schema: FactSchema::Current,
        finding_kind: EligibleFindingKind::ExplicitTargetMissing,
        key_input: FindingKeyInput {
            schema: FindingKeyInputSchema::Current,
            finding_kind: EligibleFindingKind::ExplicitTargetMissing,
            scope: FindingScope {
                kind: ReferenceScopeKind::Reference,
                document: repo_path_text(document_text),
                source_construct: SourceConstruct::InlineLink,
                normalized_target_intent: TargetIntent {
                    kind: TargetIntentKind::RepositoryPath,
                    commit_oid: None,
                    path: repo_path_text(target_text.clone()),
                    target_kind: TargetKind::Either,
                    query_digest: None,
                    fragment_digest: None,
                },
                occurrence: FindingOccurrence {
                    kind: OccurrenceKind::SourceProjection,
                    source_projection_digest: projection_digest,
                },
            },
        },
        evidence: FactEvidence {
            kind: FactEvidenceKind::Reference,
            resolution: StructuralResolution::Missing(MissingResolution::PathNotFound {
                path: repo_path_text(target_text.clone()),
                near: None,
                same_object_at: None,
            }),
            occurrence_multiplicity: 1,
        },
    };
    let observation = Observation {
        id: sha2::Sha256::new_with_prefix("amiss/bench-exception-observation")
            .chain_update([0_u8])
            .chain_update(token.as_bytes())
            .finalize()
            .0
            .into(),
        adapter_contract_digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/bench-adapter-contract")
                .chain_update([0_u8])
                .chain_update(b"markdown")
                .finalize()
                .0,
        ),
        document,
        span: (0, 0),
        display: SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
        block_kind: BlockKind::Paragraph,
        node_path: Vec::new(),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        external_destination: None,
        intent: Intent {
            kind: IntentKind::RepositoryPath,
            commit_oid: None,
            repository_path: Some(target.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        raw_destination: String::new(),
        raw_destination_digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-raw-destination")
                .chain_update([0_u8])
                .chain_update(target_text.as_bytes())
                .finalize()
                .0,
        ),
        projection_digest,
        resolution: Resolution::<RepoPath>::Missing(Missing::PathNotFound {
            path: target,
            near: None,
            same_object_at: None,
        }),
        fragment_span: None,
        path_span: None,
    };
    (observation, fact)
}

fn repo_path(raw: String) -> RepoPath {
    RepoPath::new(raw).unwrap_or_else(|| panic!("benchmark repository path"))
}

fn repo_path_text(raw: String) -> RepoPathText {
    RepoPathText::new(raw).unwrap_or_else(|| panic!("benchmark text repository path"))
}

fn artifact_id(raw: String) -> ArtifactId {
    ArtifactId::new(raw).unwrap_or_else(|| panic!("benchmark artifact id"))
}

fn owner_id(raw: &str) -> OwnerId {
    OwnerId::new(raw.to_owned()).unwrap_or_else(|| panic!("benchmark owner"))
}

fn instant(raw: &str) -> UtcInstant {
    UtcInstant::new(raw.to_owned()).unwrap_or_else(|| panic!("benchmark instant"))
}

fn tree(fill: &str) -> TreeIdentity {
    TreeIdentity {
        object_format: ObjectFormat::Sha1,
        tree_oid: amiss_wire::model::Oid::new(ObjectFormat::Sha1, fill.repeat(40))
            .unwrap_or_else(|| panic!("benchmark tree")),
    }
}
