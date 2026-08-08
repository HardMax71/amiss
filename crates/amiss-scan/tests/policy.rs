use std::collections::BTreeSet;

use amiss_scan::policy::{
    DebtInput, InventoryState, TrustSource, WaiverInput, effects, verify_debt, verify_waiver,
};
use amiss_scan::{Includes, PolicySide};
use amiss_wire::controls::ResourceName;
use amiss_wire::controls::{DebtSnapshot, Fact, FindingKeyInput, FindingScope, TargetIntent};
use amiss_wire::controls::{
    Disposition, DocumentInclude, FindingDisposition, IncludeKind, PromotableFindingKind,
    ScannerPolicy, SourceConstruct, TargetKind,
};
use amiss_wire::controls::{WaiverBundle, WaiverItem};
use amiss_wire::digest::hb;
use amiss_wire::model::{
    BranchRef, ObjectFormat, RepoPath, RepoPathText, RepositoryIdentity, TreeIdentity, UtcInstant,
};
use amiss_wire::report::AnalysisErrorCode;
use amiss_wire::resolution::{Missing, Resolution};

#[expect(clippy::expect_used, reason = "test fixture paths are valid")]
fn path(raw: &str) -> RepoPath {
    RepoPath::new(raw.to_owned()).expect("valid repository path")
}

#[test]
fn includes_match_exact_documents_and_tree_ancestors_at_slash_boundaries() {
    let byte_root = RepoPath::from_bytes(vec![b'r', 0xff]).expect("valid byte path");
    let mut byte_child = byte_root.as_bytes().to_vec();
    byte_child.extend_from_slice(b"/page.md");
    let byte_child = RepoPath::from_bytes(byte_child).expect("valid byte child");
    let includes = Includes {
        documents: BTreeSet::from([path("one.md")]),
        trees: BTreeSet::from([path("docs/specs"), byte_root.clone()]),
    };

    assert!(includes.matches(&path("one.md")));
    assert!(!includes.matches(&path("one.md/child")));
    assert!(includes.matches(&path("docs/specs")));
    assert!(includes.matches(&path("docs/specs/api/reference.md")));
    assert!(!includes.matches(&path("docs/spec")));
    assert!(!includes.matches(&path("docs/specs-old/page.md")));
    assert!(includes.matches(&byte_root));
    assert!(includes.matches(&byte_child));
}

#[test]
fn policy_comparison_indexes_kind_path_and_inventory_membership() {
    let base = policy(
        &[
            ("same", IncludeKind::Document),
            ("same", IncludeKind::Tree),
            ("z", IncludeKind::Tree),
        ],
        &["b.md", "a.md"],
    );
    let candidate = policy(
        &[("z", IncludeKind::Tree), ("same", IncludeKind::Document)],
        &["a.md"],
    );

    let got = effects(&base, &candidate, &|_path| InventoryState::Scanned);
    let rules: Vec<(&str, Option<&[u8]>)> = got
        .controls
        .iter()
        .map(|row| {
            (
                row.rule_id.as_str(),
                row.control_path.as_ref().map(RepoPath::as_bytes),
            )
        })
        .collect();
    assert_eq!(
        rules,
        [
            ("policy/include-tree-removed", Some(b"same".as_slice())),
            ("policy/inventory-removed", Some(b"b.md".as_slice())),
        ]
    );
}

#[expect(clippy::expect_used, reason = "test fixture paths are valid")]
fn policy(includes: &[(&str, IncludeKind)], inventory: &[&str]) -> PolicySide {
    let document_includes = includes
        .iter()
        .map(|(raw, kind)| DocumentInclude {
            path: RepoPathText::new((*raw).to_owned()).expect("valid include path"),
            kind: *kind,
        })
        .collect();
    let protected_inventory = inventory
        .iter()
        .map(|raw| RepoPathText::new((*raw).to_owned()).expect("valid inventory path"))
        .collect();
    let policy = ScannerPolicy {
        digest: hb("amiss/raw-evidence", b"policy fixture"),
        document_includes,
        protected_inventory,
        finding_dispositions: Vec::new(),
    };
    PolicySide {
        digest: Some(policy.digest),
        policy: Some(policy),
    }
}

#[test]
fn the_union_carries_both_sides_includes() {
    let base = policy(
        &[
            ("docs/a.md", IncludeKind::Document),
            ("docs", IncludeKind::Tree),
        ],
        &[],
    );
    let candidate = policy(&[("guides/b.md", IncludeKind::Document)], &[]);
    let union = Includes::union(&base, &candidate);
    assert_eq!(union.documents.len(), 2);
    assert_eq!(union.trees.len(), 1);
}

fn disposition_side(rows: &[(PromotableFindingKind, Disposition)]) -> PolicySide {
    let policy = ScannerPolicy {
        digest: hb("amiss/raw-evidence", b"disposition fixture"),
        document_includes: Vec::new(),
        protected_inventory: Vec::new(),
        finding_dispositions: rows
            .iter()
            .map(|(finding_kind, disposition)| FindingDisposition {
                finding_kind: *finding_kind,
                disposition: *disposition,
            })
            .collect(),
    };
    PolicySide {
        digest: Some(policy.digest),
        policy: Some(policy),
    }
}

#[test]
fn a_disposition_weakens_only_by_dropping_below_the_base() {
    let raised = &[(
        PromotableFindingKind::ExplicitTargetMissing,
        Disposition::Fail,
    )];
    let weakened_rule = "policy/disposition/explicit-target-missing";
    let holds = effects(
        &disposition_side(raised),
        &disposition_side(raised),
        &|_path| InventoryState::Scanned,
    );
    assert!(
        holds
            .controls
            .iter()
            .all(|row| row.rule_id != weakened_rule),
        "an equal disposition is not weakening"
    );

    let softened = effects(
        &disposition_side(raised),
        &disposition_side(&[(
            PromotableFindingKind::ExplicitTargetMissing,
            Disposition::Warn,
        )]),
        &|_path| InventoryState::Scanned,
    );
    assert!(
        softened
            .controls
            .iter()
            .any(|row| row.rule_id == weakened_rule)
    );

    let dropped = effects(
        &disposition_side(raised),
        &disposition_side(&[]),
        &|_path| InventoryState::Scanned,
    );
    assert!(
        dropped
            .controls
            .iter()
            .any(|row| row.rule_id == weakened_rule)
    );
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn fixture_fact() -> Fact {
    let scope = FindingScope {
        document: RepoPathText::new("README.md".to_owned()).expect("path"),
        source_construct: SourceConstruct::InlineLink,
        normalized_target_intent: TargetIntent {
            path: RepoPathText::new("docs/x.md".to_owned()).expect("path"),
            target_kind: TargetKind::Either,
            query_digest: None,
            fragment_digest: None,
        },
        source_projection_digest: hb("amiss/raw-evidence", b"projection"),
    };
    let key_input = FindingKeyInput {
        finding_kind: amiss_wire::controls::EligibleFindingKind::ExplicitTargetMissing,
        scope,
    };
    Fact::new(
        key_input,
        Resolution::Missing(Missing::PathNotFound {
            path: RepoPathText::new("docs/x.md".to_owned()).expect("path"),
            near: None,
        }),
    )
    .expect("a structural fact")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn instant() -> UtcInstant {
    UtcInstant::new("2026-07-01T00:00:00Z".to_owned()).expect("instant")
}

#[test]
fn debt_and_waiver_item_ceilings_are_exact() {
    let repository =
        RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).expect("identity");
    let debt_item = |id: &str| amiss_wire::controls::DebtItem {
        debt_id: amiss_wire::model::ArtifactId::new(id.to_owned()).expect("id"),
        finding_key: hb("amiss/raw-evidence", id.as_bytes()),
        accepted_fact: fixture_fact(),
        accepted_fact_digest: hb("amiss/raw-evidence", b"fact"),
        owner: amiss_wire::model::OwnerId::new("team:docs".to_owned()).expect("owner"),
        reason: "accepted".to_owned(),
        created_at: instant(),
        expires_at: UtcInstant::new("2026-08-01T00:00:00Z".to_owned()).expect("instant"),
    };
    let snapshot = |count: usize| DebtInput {
        snapshot: DebtSnapshot {
            digest: hb("amiss/raw-evidence", b"snapshot"),
            repository: repository.clone(),
            ref_name: BranchRef::new("refs/heads/main".to_owned()).expect("ref"),
            organization_floor_digest: hb("amiss/raw-evidence", b"floor"),
            adoption_tree: TreeIdentity {
                object_format: ObjectFormat::Sha1,
                tree_oid: "a".repeat(40),
            },
            adoption_report_payload_digest: hb("amiss/raw-evidence", b"report"),
            created_at: instant(),
            items: (0..count)
                .map(|index| debt_item(&format!("debt/{index}")))
                .collect(),
        },
        trust_source: TrustSource::ExternalRequiredCheck,
    };
    let at_cap = verify_debt(&snapshot(1), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(
        at_cap.code,
        AnalysisErrorCode::ControlBindingMismatch,
        "one item under a ceiling of one is within it; only the binding fails"
    );
    let over = verify_debt(&snapshot(2), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(over.code, AnalysisErrorCode::ResourceLimitExceeded);
    assert_eq!(over.resource, Some((ResourceName::DebtItems, 1, 2)));

    let waiver_item = |id: &str| WaiverItem {
        waiver_id: amiss_wire::model::ArtifactId::new(id.to_owned()).expect("id"),
        finding_key: hb("amiss/raw-evidence", id.as_bytes()),
        authorized_fact: fixture_fact(),
        authorized_fact_digest: hb("amiss/raw-evidence", b"fact"),
        candidate_tree: TreeIdentity {
            object_format: ObjectFormat::Sha1,
            tree_oid: "b".repeat(40),
        },
        owner: amiss_wire::model::OwnerId::new("team:docs".to_owned()).expect("owner"),
        issuer: amiss_wire::model::OwnerId::new("team:release".to_owned()).expect("owner"),
        reason: "window".to_owned(),
        created_at: instant(),
        not_before: instant(),
        expires_at: UtcInstant::new("2026-08-01T00:00:00Z".to_owned()).expect("instant"),
    };
    let bundle = |count: usize| WaiverInput {
        bundle: WaiverBundle {
            digest: hb("amiss/raw-evidence", b"bundle"),
            repository: repository.clone(),
            ref_name: BranchRef::new("refs/heads/main".to_owned()).expect("ref"),
            organization_floor_digest: hb("amiss/raw-evidence", b"floor"),
            created_at: instant(),
            items: (0..count)
                .map(|index| waiver_item(&format!("waiver/{index}")))
                .collect(),
        },
        trust_source: TrustSource::ExternalRequiredCheck,
    };
    let at_cap = verify_waiver(&bundle(1), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(at_cap.code, AnalysisErrorCode::ControlBindingMismatch);
    let over = verify_waiver(&bundle(2), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(over.code, AnalysisErrorCode::ResourceLimitExceeded);
    assert_eq!(over.resource, Some((ResourceName::WaiverItems, 1, 2)));
}
