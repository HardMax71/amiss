use std::fs;

use tempfile::TempDir;

use crate::support::{amiss, byte_named_index, git, hidden_entry, payload};

/// A name that is raw bytes rather than text is a document, not a defect. The
/// entry is scanned, the report carries its path as the `bytes_hex` object,
/// the run completes, and nothing is hidden; re-adding a spelling gate
/// anywhere in discovery fails this test.
#[test]
fn a_document_named_in_bytes_is_scanned_not_refused() {
    let name = b"docs/bad-\xff-name.md".as_slice();
    let hex = "646f63732f6261642dff2d6e616d652e6d64";
    for index_mode in [false, true] {
        let (code, payload) = hidden_entry(name, index_mode);
        let where_from = if index_mode { "index" } else { "tree" };
        assert_eq!(
            code, 0,
            "{where_from}: a byte-named document is not an error"
        );
        assert_eq!(payload["result"]["complete"], true, "{where_from}");
        assert!(
            payload["errors"].as_array().unwrap().is_empty(),
            "{where_from}: nothing to disclose, nothing hidden"
        );
        let documents = payload["documents"].as_array().unwrap();
        let row = documents
            .iter()
            .find(|row| row["path"]["bytes_hex"] == hex)
            .unwrap_or_else(|| panic!("{where_from}: the byte-named document is published"));
        assert_eq!(
            row["classification"], "structured-markdown",
            "{where_from}: bytes classify by the same suffix rows as text"
        );
        if index_mode {
            let candidate = &payload["evaluation"]["candidate"];
            assert_eq!(candidate["kind"], "index", "{where_from}");
            assert_eq!(
                candidate["entry_count"], 3,
                "{where_from}: the identity counts every row, the byte-named one included"
            );
            assert!(
                candidate["snapshot_digest"].as_str().is_some(),
                "{where_from}: the identity is complete and digestible"
            );
        }
    }
}

/// Distinct refused names are distinct disclosures. Before the bytes rode
/// along, every refused name collapsed into one identical error row and the
/// deduplicated set said "one problem" no matter how many entries were
/// hidden. Refusals remain structural.
#[test]
fn every_unnameable_entry_is_disclosed_separately() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# Hidden\n").unwrap();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("100644", b"bad\\one.md".as_slice(), blob.as_str()),
            ("100644", b"bad\\two.md".as_slice(), blob.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    let disclosed: Vec<&str> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .map(|row| row["path_bytes_hex"].as_str().unwrap())
        .collect();
    assert_eq!(
        disclosed,
        vec!["6261645c6f6e652e6d64", "6261645c74776f2e6d64"],
        "two hidden entries are two rows, in byte order, each naming its bytes"
    );

    amiss_fixtures::index_file(
        root,
        &[
            (b"README.md".as_slice(), readme.as_str()),
            (b"bad\\one.md".as_slice(), blob.as_str()),
            (b"bad\\two.md".as_slice(), blob.as_str()),
        ],
    )
    .unwrap();
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--index",
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let staged = serde_json::from_slice::<serde_json::Value>(&stdout).unwrap()["payload"].clone();
    let disclosed: Vec<&str> = staged["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .map(|row| row["path_bytes_hex"].as_str().unwrap())
        .collect();
    assert_eq!(
        disclosed,
        vec!["6261645c6f6e652e6d64", "6261645c74776f2e6d64"],
        "the staged gate discloses every unspellable row too, not just the first"
    );
}

/// A name can be both unspellable and past the length ceiling. The ceiling
/// is charged first, on the raw bytes, and the crossing row carries no hex:
/// the field's frozen cap is the path ceiling itself, so bytes past it can
/// never be disclosed without breaking the report's own schema.
#[test]
fn an_over_length_unspellable_name_is_a_crossing_with_no_bytes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# Hidden\n").unwrap();
    let long_name = [b"bad-".as_slice(), &[0xff_u8; 5000], b".md"].concat();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("100644", &long_name, blob.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    let row = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "RESOURCE_LIMIT_EXCEEDED")
        .unwrap();
    assert_eq!(row["resource"], "raw-path-bytes");
    assert_eq!(row["configured_limit"], 4096);
    assert_eq!(row["observed_lower_bound"], 5007);
    assert_eq!(
        row["path_bytes_hex"],
        serde_json::Value::Null,
        "bytes past the ceiling are stated by figure, never by hex the schema forbids"
    );
    assert!(
        !payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "UNREPRESENTABLE_PATH"),
        "the ceiling is charged first; the spelling question is never reached"
    );

    amiss_fixtures::index_file(
        root,
        &[
            (b"README.md".as_slice(), readme.as_str()),
            (&long_name, blob.as_str()),
        ],
    )
    .unwrap();
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--index",
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let staged = serde_json::from_slice::<serde_json::Value>(&stdout).unwrap()["payload"].clone();
    let row = staged["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .unwrap();
    assert_eq!(
        row["path_bytes_hex"],
        serde_json::Value::Null,
        "the identity gate answers the spelling question, and omits hex past the field's cap"
    );
}

/// A link may percent-escape bytes no text can hold, and under the second
/// contract those bytes name a real target: `%FF` decodes to the byte and the
/// reference resolves bytewise against the tree. The sibling that decodes to
/// bytes nothing in the tree carries stays a missing target whose normalized
/// intent names the bytes exactly.
#[test]
fn a_percent_escaped_byte_reference_resolves_against_the_byte_named_target() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = amiss_fixtures::loose_object(
        root,
        "blob",
        b"# R\n\n[found](docs/bad-%FF-name.md) and [gone](docs/bad-%FE-name.md)\n",
    )
    .unwrap();
    let hidden = amiss_fixtures::loose_object(root, "blob", b"# Hidden\n").unwrap();
    let docs = amiss_fixtures::tree_object(
        root,
        &[("100644", b"bad-\xff-name.md".as_slice(), hidden.as_str())],
    )
    .unwrap();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("40000", b"docs".as_slice(), docs.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0);
    let payload = payload(&stdout);
    assert_eq!(payload["summary"]["references"]["extracted"], 2);
    assert_eq!(
        payload["summary"]["references"]["resolved"], 1,
        "the byte-named target is found bytewise"
    );
    assert_eq!(payload["summary"]["references"]["missing"], 1);
    let finding = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        finding["key_input"]["scope"]["normalized_target_intent"]["path"]["bytes_hex"],
        "646f63732f6261642dfe2d6e616d652e6d64",
        "the missing target's identity names the bytes exactly"
    );
}

/// A policy tree include covers byte-named documents under it: the include is
/// text, the tree-prefix rule is bytewise, and a byte-named file with no
/// native classification becomes policy-included rather than invisible.
#[test]
fn a_policy_tree_include_covers_byte_named_documents() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let policy = amiss_fixtures::loose_object(
        root,
        "blob",
        br#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"specs"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap();
    let hidden = amiss_fixtures::loose_object(root, "blob", b"included bytes\n").unwrap();
    let amiss_dir = amiss_fixtures::tree_object(
        root,
        &[("100644", b"scanner-policy.json".as_slice(), policy.as_str())],
    )
    .unwrap();
    let specs = amiss_fixtures::tree_object(
        root,
        &[("100644", b"design-\xff.tex".as_slice(), hidden.as_str())],
    )
    .unwrap();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("40000", b".amiss".as_slice(), amiss_dir.as_str()),
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("40000", b"specs".as_slice(), specs.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0);
    let payload = payload(&stdout);
    let row = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"]["bytes_hex"] == "73706563732f64657369676e2dff2e746578")
        .expect("the included byte-named document is published");
    assert_eq!(row["classification"], "policy-included");
}

/// Two runs over a tree interleaving text and byte names produce the
/// identical wire, and the documents array sorts by raw path bytes, so the
/// 0xFF name lands after every ASCII name rather than clustering by form.
#[test]
fn byte_and_text_paths_interleave_deterministically_in_byte_order() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# D\n").unwrap();
    let docs = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"a.md".as_slice(), blob.as_str()),
            ("100644", b"m-\xfe.md".as_slice(), blob.as_str()),
            ("100644", b"z.md".as_slice(), blob.as_str()),
            ("100644", b"\xff.md".as_slice(), blob.as_str()),
        ],
    )
    .unwrap();
    let tree =
        amiss_fixtures::tree_object(root, &[("40000", b"docs".as_slice(), docs.as_str())]).unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap();
    let repo = amiss_fixtures::path_arg(root);
    let args = [
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ];
    let (first_code, first) = amiss(&args);
    let (second_code, second) = amiss(&args);
    assert_eq!((first_code, second_code), (0, 0));
    assert_eq!(first, second, "identical inputs, identical wire");
    let payload = payload(&first);
    let order: Vec<String> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row["path"].as_str().map_or_else(
                || format!("hex:{}", row["path"]["bytes_hex"].as_str().unwrap()),
                str::to_owned,
            )
        })
        .collect();
    assert_eq!(
        order,
        vec![
            "README.md".to_owned(),
            "docs/a.md".to_owned(),
            "hex:646f63732f6d2dfe2e6d64".to_owned(),
            "docs/z.md".to_owned(),
            "hex:646f63732fff2e6d64".to_owned(),
        ],
        "raw byte order, not form-clustered"
    );
}

/// GitLab requires a path on every issue, so a byte-named document answers
/// the Code Quality projection with the wire's own hex spelling.
#[test]
fn a_bytes_located_code_quality_issue_carries_the_wire_hex_spelling() {
    let (dir, base) = byte_named_index(b"# H\n\n[g](gone.md)\n");
    let repo = amiss_fixtures::path_arg(dir.path());
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--index",
        "--profile",
        "enforce",
        "--format",
        "codequality",
    ]);
    assert_eq!(code, 1);
    let issues: Vec<serde_json::Value> = serde_json::from_slice(&stdout).unwrap();
    let issue = issues
        .iter()
        .find(|issue| issue["check_name"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        issue.pointer("/location/path").unwrap(),
        crate::support::BYTE_NAME_HEX,
    );
    assert!(
        issue
            .pointer("/location/lines/begin")
            .is_some_and(|line| line.as_i64().is_some_and(|value| value >= 1),),
        "{issue}"
    );
    assert!(
        issue["fingerprint"]
            .as_str()
            .is_some_and(|key| key.starts_with("sha256:")),
        "{issue}"
    );
}

/// A finding located in a byte-named document keeps its fingerprint in the
/// SARIF projection and simply carries no artifact location, because raw
/// bytes name no URI.
#[test]
fn a_bytes_located_sarif_result_keeps_its_fingerprint_without_a_location() {
    let (dir, base) = byte_named_index(b"# H\n\n[g](gone.md)\n");
    let repo = amiss_fixtures::path_arg(dir.path());
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--index",
        "--profile",
        "enforce",
        "--format",
        "sarif",
    ]);
    assert_eq!(code, 1);
    let log: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let results = log.pointer("/runs/0/results").unwrap().as_array().unwrap();
    let row = results
        .iter()
        .find(|result| result["ruleId"] == "explicit-target-missing")
        .unwrap();
    assert!(row.get("locations").is_none(), "{row}");
    assert!(
        row.pointer("/partialFingerprints/amissFindingKey~1v1")
            .and_then(|key| key.as_str())
            .is_some_and(|key| key.starts_with("sha256:")),
        "{row}"
    );
}
