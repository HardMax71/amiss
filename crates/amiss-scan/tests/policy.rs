use std::collections::BTreeSet;

use amiss_scan::policy::{InventoryState, effects};
use amiss_scan::{Includes, PolicySide};
use amiss_wire::controls::{DocumentInclude, IncludeKind, ScannerPolicy};
use amiss_wire::digest::hb;
use amiss_wire::model::{RepoPath, RepoPathText};

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
