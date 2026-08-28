use std::collections::{BTreeMap, BTreeSet};

use amiss_scan::policy::{
    DebtInput, InventoryState, WaiverInput, effects, verify_debt, verify_waiver,
};
use amiss_scan::{Includes, PolicySide};
use amiss_wire::controls::{
    BlobLineSelection, Disposition, DocumentInclude, FACT_DOMAIN, FINDING_KEY_DOMAIN,
    FindingDisposition, IncludeKind, ProjectionAssertion, PromotableFindingKind, ResourceName,
    ScannerPolicy, WaiverBundle,
};
use amiss_wire::digest::hj;
use amiss_wire::model::{RepoPath, RepoPathText, UtcInstant};
use amiss_wire::report::AnalysisErrorCode;
use amiss_wire::requests::RequestTrust;

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
        ..Includes::default()
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
fn suffix_selectors_match_one_exact_tail_below_one_exact_root() {
    let byte_path = RepoPath::from_bytes(b"manual/\xff/raw.txt".to_vec())
        .expect("valid non-UTF-8 fixture path");
    let includes = Includes {
        suffix_roots: BTreeMap::from([
            (".spec.txt".to_owned(), BTreeSet::from([path("specs")])),
            (".txt".to_owned(), BTreeSet::from([path("manual")])),
        ]),
        ..Includes::default()
    };

    for selected in [
        path("manual/guide.txt"),
        path("manual/deep/archive.spec.txt"),
        path("specs/archive.spec.txt"),
        byte_path,
    ] {
        assert!(includes.matches(&selected), "selected {selected:?}");
    }
    for outside in [
        path("manual/guide.TXT"),
        path("manual/guide.txt.bak"),
        path("manual-old/guide.txt"),
        path("other/guide.txt"),
        path("specs/archive.txt"),
    ] {
        assert!(!includes.matches(&outside), "outside {outside:?}");
    }
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
            suffix: None,
            adapter: None,
        })
        .collect();
    let protected_inventory = inventory
        .iter()
        .map(|raw| RepoPathText::new((*raw).to_owned()).expect("valid inventory path"))
        .collect();
    let policy = ScannerPolicy::new(
        document_includes,
        Vec::new(),
        protected_inventory,
        Vec::new(),
    )
    .expect("valid policy fixture");
    PolicySide {
        digest: Some(policy.digest()),
        policy: Some(policy),
    }
}

#[test]
fn a_projection_selector_change_keeps_identity_and_removal_weakens() {
    let side = |first_line| {
        let policy = ScannerPolicy::new(
            Vec::new(),
            vec![ProjectionAssertion {
                document: RepoPathText::new("docs/example.md".to_owned())
                    .expect("valid document path"),
                name: "example".to_owned(),
                source: BlobLineSelection {
                    path: RepoPathText::new("src/lib.rs".to_owned()).expect("valid source path"),
                    first_line,
                    last_line: first_line,
                },
            }],
            Vec::new(),
            Vec::new(),
        )
        .expect("valid projection policy");
        PolicySide {
            digest: Some(policy.digest()),
            policy: Some(policy),
        }
    };
    let base = side(1);
    let changed = effects(&base, &side(2), &|_| InventoryState::Scanned);
    assert!(
        changed.controls.iter().all(|row| !row
            .rule_id
            .starts_with("policy/projection-assertion-removed/")),
        "selector state is not identity"
    );
    assert_ne!(changed.base_digest, changed.candidate_digest);

    let removed = effects(&base, &PolicySide::default(), &|_| InventoryState::Scanned);
    let [control] = removed.controls.as_slice() else {
        panic!("one removed projection control: {:?}", removed.controls);
    };
    assert_eq!(
        control.rule_id,
        "policy/projection-assertion-removed/example"
    );
    assert_eq!(control.control_path, Some(path("docs/example.md")));
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

#[test]
fn the_union_carries_both_suffixes_but_the_candidate_binding() {
    let side = |suffix: &str, adapter| {
        let policy = ScannerPolicy::new(
            vec![DocumentInclude {
                path: RepoPathText::new("manual".to_owned()).expect("valid include path"),
                kind: IncludeKind::Tree,
                suffix: Some(suffix.to_owned()),
                adapter: Some(adapter),
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid suffix selector");
        PolicySide {
            digest: Some(policy.digest()),
            policy: Some(policy),
        }
    };
    let base = side(".txt", amiss_wire::model::Adapter::Rst);
    let candidate = side(".guide", amiss_wire::model::Adapter::Markdown);
    let union = Includes::union(&base, &candidate);

    assert!(union.matches(&path("manual/old.txt")));
    assert!(union.matches(&path("manual/current.guide")));
    assert_eq!(union.binding(&path("manual/old.txt")), None);
    assert_eq!(
        union.binding(&path("manual/current.guide")),
        Some(amiss_wire::model::Adapter::Markdown)
    );
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn disposition_side(rows: &[(PromotableFindingKind, Disposition)]) -> PolicySide {
    let policy = ScannerPolicy::new(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        rows.iter()
            .map(|(finding_kind, disposition)| FindingDisposition {
                finding_kind: *finding_kind,
                disposition: *disposition,
            })
            .collect(),
    )
    .expect("valid disposition fixture");
    PolicySide {
        digest: Some(policy.digest()),
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
fn instant() -> UtcInstant {
    UtcInstant::new("2026-07-01T00:00:00Z".to_owned()).expect("instant")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn cloned_first_item(document: &serde_json::Value) -> serde_json::Value {
    document
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .cloned()
        .expect("fixture item")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn replace_json(value: &mut serde_json::Value, pointer: &str, replacement: serde_json::Value) {
    *value.pointer_mut(pointer).expect("fixture field") = replacement;
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn push_item(document: &mut serde_json::Value, item: serde_json::Value) {
    document
        .get_mut("items")
        .and_then(serde_json::Value::as_array_mut)
        .expect("fixture items")
        .push(item);
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn debt_input(item_count: usize) -> DebtInput {
    let mut document: serde_json::Value =
        serde_json::from_slice(&crate::support::fixture_bytes("debt-snapshot.json"))
            .expect("debt fixture JSON");
    if item_count == 2 {
        let mut second = cloned_first_item(&document);
        replace_json(&mut second, "/debt_id", "debt/zz-second-example".into());
        replace_json(
            &mut second,
            "/accepted_fact/key_input/scope/occurrence/source_projection_digest",
            "sha256:8888888888888888888888888888888888888888888888888888888888888888".into(),
        );
        let key_input = serde_json::to_vec(
            second
                .pointer("/accepted_fact/key_input")
                .expect("key input"),
        )
        .expect("key input JSON");
        let key_input = amiss_wire::json::parse(&key_input).expect("key input wire JSON");
        replace_json(
            &mut second,
            "/finding_key",
            hj(FINDING_KEY_DOMAIN, &key_input).to_string().into(),
        );
        let fact =
            serde_json::to_vec(second.pointer("/accepted_fact").expect("fact")).expect("fact JSON");
        let fact = amiss_wire::json::parse(&fact).expect("fact wire JSON");
        replace_json(
            &mut second,
            "/accepted_fact_digest",
            hj(FACT_DOMAIN, &fact).to_string().into(),
        );
        push_item(&mut document, second);
    }
    let bytes = serde_json::to_vec(&document).expect("debt document JSON");
    DebtInput {
        snapshot: amiss_wire::controls::DebtSnapshot::parse(&bytes).expect("valid debt fixture"),
        trust_source: RequestTrust::ExternalRequiredCheck,
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn waiver_input(item_count: usize) -> WaiverInput {
    let mut document: serde_json::Value =
        serde_json::from_slice(&crate::support::fixture_bytes("waiver-bundle.json"))
            .expect("waiver fixture JSON");
    if item_count == 2 {
        let mut second = cloned_first_item(&document);
        replace_json(&mut second, "/waiver_id", "waiver/zz-second-example".into());
        replace_json(
            &mut second,
            "/candidate_tree/tree_oid",
            "c".repeat(40).into(),
        );
        push_item(&mut document, second);
    }
    let bytes = serde_json::to_vec(&document).expect("waiver document JSON");
    WaiverInput {
        bundle: WaiverBundle::parse(&bytes).expect("valid waiver fixture"),
        trust_source: RequestTrust::ExternalRequiredCheck,
    }
}

#[test]
fn debt_and_waiver_item_ceilings_are_exact() {
    let at_cap = verify_debt(&debt_input(1), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(
        at_cap.code,
        AnalysisErrorCode::ControlBindingMismatch,
        "one item under a ceiling of one is within it; only the binding fails"
    );
    let over = verify_debt(&debt_input(2), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(over.code, AnalysisErrorCode::ResourceLimitExceeded);
    assert_eq!(over.resource, Some((ResourceName::DebtItems, 1, 2)));

    let at_cap = verify_waiver(&waiver_input(1), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(at_cap.code, AnalysisErrorCode::ControlBindingMismatch);
    let over = verify_waiver(&waiver_input(2), None, None, None, &instant(), 1).unwrap_err();
    assert_eq!(over.code, AnalysisErrorCode::ResourceLimitExceeded);
    assert_eq!(over.resource, Some((ResourceName::WaiverItems, 1, 2)));
}

/// The binding map answers documents exactly, then the nearest bound
/// ancestor tree, and nothing else.
#[test]
fn a_binding_answers_documents_then_the_nearest_tree() {
    let mut includes = Includes::default();
    includes
        .document_bindings
        .insert(path("docs/a.q"), amiss_wire::model::Adapter::Markdown);
    includes
        .tree_bindings
        .insert(path("man"), amiss_wire::model::Adapter::Rst);
    includes
        .tree_bindings
        .insert(path("man/deep"), amiss_wire::model::Adapter::Mdx);
    includes
        .document_bindings
        .insert(path("man/deep/y.txt"), amiss_wire::model::Adapter::Markdown);
    includes.suffix_bindings.insert(
        path("manual"),
        (".txt".to_owned(), amiss_wire::model::Adapter::Rst),
    );
    assert_eq!(
        includes.binding(&path("docs/a.q")),
        Some(amiss_wire::model::Adapter::Markdown)
    );
    assert_eq!(
        includes.binding(&path("man/x.txt")),
        Some(amiss_wire::model::Adapter::Rst)
    );
    assert_eq!(
        includes.binding(&path("man")),
        Some(amiss_wire::model::Adapter::Rst),
        "a tree binding covers its exact root as its include does"
    );
    assert_eq!(
        includes.binding(&path("man/deep/y.txt")),
        Some(amiss_wire::model::Adapter::Markdown),
        "an exact document binding beats the tree covering it"
    );
    assert_eq!(
        includes.binding(&path("man/deep/z.txt")),
        Some(amiss_wire::model::Adapter::Mdx),
        "the nearest bound ancestor answers the rest of the tree"
    );
    assert_eq!(includes.binding(&path("other/z.txt")), None);
    assert_eq!(
        includes.binding(&path("manual/guide.txt")),
        Some(amiss_wire::model::Adapter::Rst)
    );
    assert_eq!(includes.binding(&path("manual/guide.rst")), None);
}

/// Only a policy-included row under its own path answers the bound-adapter
/// lookup; native classifications never do.
#[test]
fn bound_adapter_answers_only_policy_included_rows() {
    use amiss_scan::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery};
    use amiss_wire::controls::GitMode;
    use amiss_wire::model::{Adapter, ObjectFormat, Oid};

    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).expect("valid fixture oid");
    let record = |raw: &str, classification, adapter| DocumentRecord {
        path: path(raw),
        classification,
        adapter,
        status: DocumentStatus::ExcludedBuiltIn,
        oid: oid.clone(),
        mode: GitMode::RegularFile,
        byte_count: 0,
        raw_digest: None,
    };
    let snapshot = SnapshotDiscovery {
        documents: vec![
            record(
                "docs/r.rst",
                amiss_scan::Classification::StructuredRst,
                Some(Adapter::Rst),
            ),
            record(
                "man/g.txt",
                amiss_scan::Classification::PolicyIncluded,
                Some(Adapter::Rst),
            ),
        ],
        outside_document_set: 0,
        tree_entries: 2,
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
        labels: BTreeMap::new(),
    };
    assert_eq!(
        snapshot.bound_adapter(&path("man/g.txt")),
        Some(Adapter::Rst)
    );
    assert_eq!(
        snapshot.bound_adapter(&path("docs/r.rst")),
        None,
        "a native classification is not a binding"
    );
    assert_eq!(snapshot.bound_adapter(&path("man/other.txt")), None);
}

/// The binding clauses of include weakening, each alone: dropping or changing
/// a binding is weakening, keeping or adding one is not.
#[test]
fn a_binding_drop_or_change_weakens_and_an_addition_does_not() {
    use amiss_wire::model::Adapter;
    let side = |adapter: Option<Adapter>| {
        let policy = ScannerPolicy::new(
            vec![DocumentInclude {
                path: RepoPathText::new("man".to_owned()).expect("valid include path"),
                kind: IncludeKind::Tree,
                suffix: None,
                adapter,
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid binding fixture");
        PolicySide {
            digest: Some(policy.digest()),
            policy: Some(policy),
        }
    };
    let removed = |got: &amiss_scan::policy::Effects| {
        got.controls
            .iter()
            .filter(|row| row.rule_id == "policy/include-binding-removed")
            .count()
    };
    let scanned: fn(&str) -> InventoryState = |_| InventoryState::Scanned;

    let dropped = effects(&side(Some(Adapter::Rst)), &side(None), &scanned);
    assert_eq!(removed(&dropped), 1, "dropping the binding weakens");
    let changed = effects(
        &side(Some(Adapter::Rst)),
        &side(Some(Adapter::Markdown)),
        &scanned,
    );
    assert_eq!(removed(&changed), 1, "changing the grammar weakens");
    let kept = effects(
        &side(Some(Adapter::Rst)),
        &side(Some(Adapter::Rst)),
        &scanned,
    );
    assert_eq!(removed(&kept), 0, "an unchanged binding is not weakening");
    let added = effects(&side(None), &side(Some(Adapter::Rst)), &scanned);
    assert_eq!(removed(&added), 0, "adding a binding is a plain tighten");
}

#[test]
fn suffix_selector_changes_keep_their_stable_root_identity() {
    use amiss_wire::model::Adapter;
    let side = |suffix: Option<&str>, adapter: Option<Adapter>| {
        let policy = ScannerPolicy::new(
            vec![DocumentInclude {
                path: RepoPathText::new("manual".to_owned()).expect("valid include path"),
                kind: IncludeKind::Tree,
                suffix: suffix.map(str::to_owned),
                adapter,
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid selector fixture");
        PolicySide {
            digest: Some(policy.digest()),
            policy: Some(policy),
        }
    };
    let absent = PolicySide::default();
    let selected = side(Some(".txt"), Some(Adapter::Rst));
    let scanned: fn(&str) -> InventoryState = |_| InventoryState::Scanned;
    let rules = |candidate: &PolicySide| {
        effects(&selected, candidate, &scanned)
            .controls
            .into_iter()
            .map(|row| row.rule_id)
            .collect::<Vec<_>>()
    };

    assert_eq!(rules(&absent), ["policy/include-suffix-selector-removed"]);
    assert_eq!(
        rules(&side(None, Some(Adapter::Rst))),
        ["policy/include-suffix-removed"]
    );
    assert_eq!(
        rules(&side(Some(".rst"), Some(Adapter::Rst))),
        ["policy/include-suffix-removed"]
    );
    assert_eq!(
        rules(&side(Some(".txt"), None)),
        ["policy/include-binding-removed"]
    );
    assert!(rules(&selected).is_empty());

    let narrowed = effects(&side(None, Some(Adapter::Rst)), &selected, &scanned);
    assert_eq!(narrowed.controls[0].rule_id, "policy/include-tree-narrowed");
}
