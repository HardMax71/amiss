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
    DebtItem, EligibleFindingKind, Fact, FindingKeyInput, FindingScope, SourceConstruct,
    TargetIntent, TargetKind, TrustedTimeStatement,
};
use amiss_wire::digest::hb;
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
    let findings = evaluate(&[], &comparisons, true);
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
                .map_or_else(|| panic!("benchmark candidate fact"), |(_, digest)| *digest);
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
    let debt_digest = hb("amiss/bench-debt-context", b"matching debt items");
    let time_digest = hb("amiss/bench-time-context", b"trusted time");
    let policy = Effects {
        debt: Some(DebtContext {
            digest: debt_digest,
            trust_source: "benchmark",
            adoption_tree: tree("a"),
            items,
        }),
        time: Some(TimeContext {
            statement: TrustedTimeStatement {
                digest: time_digest,
                repository: RepositoryIdentity::github("bench".to_owned(), "docs".to_owned())
                    .unwrap_or_else(|| panic!("benchmark repository identity")),
                ref_name: BranchRef::new("refs/heads/main".to_owned())
                    .unwrap_or_else(|| panic!("benchmark branch")),
                candidate_identity_digest: hb("amiss/bench-candidate-identity", b"candidate"),
                provider: "github-actions".to_owned(),
                provider_run_id: "1".to_owned(),
                provider_run_attempt: 1,
                evaluation_instant: instant("2026-07-12T10:00:00Z"),
                valid_until: instant("2026-07-12T10:05:00Z"),
            },
            digest: time_digest,
        }),
        ..Effects::default()
    };
    (comparisons, policy)
}

fn exception_observation(index: usize) -> (Observation, Fact) {
    let token = format!("{index:05}");
    let document_text = "docs/references.md".to_owned();
    let target_text = format!("targets/{token}.rs");
    let document = repo_path(document_text.clone());
    let target = repo_path(target_text.clone());
    let projection_digest = hb("amiss/scanner-source-projection", b"reference");
    let key_input = FindingKeyInput {
        finding_kind: EligibleFindingKind::ExplicitTargetMissing,
        scope: FindingScope {
            document: repo_path_text(document_text),
            source_construct: SourceConstruct::InlineLink,
            normalized_target_intent: TargetIntent {
                path: repo_path_text(target_text.clone()),
                target_kind: TargetKind::Either,
                query_digest: None,
                fragment_digest: None,
            },
            source_projection_digest: projection_digest,
        },
    };
    let fact = Fact::new(
        key_input,
        Resolution::<RepoPathText>::Missing(Missing::PathNotFound {
            path: repo_path_text(target_text.clone()),
        }),
    )
    .unwrap_or_else(|| panic!("benchmark structural fact"));
    let observation = Observation {
        id: hb("amiss/bench-exception-observation", token.as_bytes()),
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
        intent: Intent {
            kind: IntentKind::RepositoryPath,
            repository_path: Some(target.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        raw_destination_digest: hb("amiss/scanner-raw-destination", target_text.as_bytes()),
        projection_digest,
        resolution: Resolution::<RepoPath>::Missing(Missing::PathNotFound { path: target }),
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
    TreeIdentity::new(ObjectFormat::Sha1, fill.repeat(40))
        .unwrap_or_else(|| panic!("benchmark tree"))
}
