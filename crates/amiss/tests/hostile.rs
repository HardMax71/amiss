#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn amiss(args: &[&str]) -> (i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (output.status.code().unwrap_or(-1), output.stdout)
}

fn payload(stdout: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    envelope["payload"].clone()
}

/// The repository under evaluation is the attacker. It writes the policy the
/// scanner reads, so the one thing that policy may never do is widen what the
/// scanner is allowed to do. A field naming a command or a plugin is not a
/// feature request the scanner declines politely: it is an unknown field, the
/// configuration is invalid, the run is incomplete, and there is no report to
/// mistake for a pass. The sentinel proves the obvious thing anyway, because the
/// obvious thing is the whole product.
#[test]
fn a_policy_that_names_a_command_or_a_plugin_is_refused_and_nothing_runs() {
    let sentinel = std::env::temp_dir().join("amiss-policy-execution-sentinel");
    let _absent = fs::remove_file(&sentinel);

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n\n[self](guide.md)\n").unwrap();
    // a Windows temp path carries backslashes, which JSON must see escaped
    let command = format!("touch {}", sentinel.display()).replace('\\', "\\\\");
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        format!(
            r#"{{
  "schema": "amiss/scanner-policy",
  "document_includes": [],
  "protected_inventory": [],
  "finding_dispositions": [],
  "command": "{command}",
  "plugin": "./evil.so"
}}"#
        ),
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[self](guide.md)\n\nmore\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);

    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        git(root, &["rev-parse", "HEAD~1"]).trim(),
        "--candidate",
        git(root, &["rev-parse", "HEAD"]).trim(),
        "--profile",
        "observe",
        "--format",
        "json",
    ]);

    assert_eq!(
        code, 2,
        "a policy it cannot read is not a policy it ignores"
    );
    let payload = payload(&stdout);
    let mut codes: Vec<&str> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| error["code"].as_str().unwrap())
        .collect();
    codes.sort_unstable();
    assert_eq!(codes, vec!["CONFIGURATION_INVALID", "UNKNOWN_FIELD"]);
    assert_eq!(payload["result"]["complete"], false);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert!(
        !sentinel.exists(),
        "the policy's command ran and wrote {}",
        sentinel.display()
    );
}

/// In index mode the candidate is the staged index, and the staged index is the
/// whole of it. A file sitting in the worktree that nobody staged is not part of
/// the tree being evaluated, so a reference to it does not resolve, and the
/// finding stands. Getting this wrong needs only one `fs::metadata` call
/// somewhere in resolution, and it would be invisible: every reference would
/// still resolve, the report would still pass, and the tool would be answering a
/// question about the developer's disk instead of the commit under review.
#[test]
fn an_untracked_file_cannot_satisfy_an_index_mode_reference() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[later](arriving.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join("docs/arriving.md"),
        "# Arriving\n\nbut never staged\n",
    )
    .unwrap();
    assert!(
        root.join("docs/arriving.md").exists(),
        "the target is on disk, and only on disk"
    );

    let repo = amiss_fixtures::path_arg(root);
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

    assert_eq!(code, 0, "observe reports rather than blocks");
    let payload = payload(&stdout);
    assert_eq!(
        payload["summary"]["references"]["missing"], 1,
        "the reference is still missing, because the file it names is not staged"
    );
    let documents: Vec<&str> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert!(
        !documents.contains(&"docs/arriving.md"),
        "an untracked file is not a document either: {documents:?}"
    );
}

/// Runs a repository whose tree or index carries one entry named `name`,
/// alongside two documents the scanner can read. The entry is written straight
/// into the store or index bytes, past any git port's opinion of the name.
fn hidden_entry(name: &[u8], index_mode: bool) -> (i32, serde_json::Value) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# R\n\n[g](docs/guide.md)\n").unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let guide = git(root, &["rev-parse", "HEAD:docs/guide.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# Hidden\n").unwrap();

    let candidate = if index_mode {
        amiss_fixtures::index_file(
            root,
            &[
                (b"README.md".as_slice(), readme.as_str()),
                (b"docs/guide.md".as_slice(), guide.as_str()),
                (name, blob.as_str()),
            ],
        )
        .unwrap();
        String::new()
    } else {
        let docs_entries: Vec<(&str, &[u8], &str)> = match name.strip_prefix(b"docs/") {
            Some(inner) => vec![
                ("100644", b"guide.md".as_slice(), guide.as_str()),
                ("100644", inner, blob.as_str()),
            ],
            None => vec![("100644", b"guide.md".as_slice(), guide.as_str())],
        };
        let docs = amiss_fixtures::tree_object(root, &docs_entries).unwrap();
        let mut root_entries: Vec<(&str, &[u8], &str)> = vec![
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("40000", b"docs".as_slice(), docs.as_str()),
        ];
        if !name.contains(&b'/') {
            root_entries.push(("100644", name, blob.as_str()));
        }
        let tree = amiss_fixtures::tree_object(root, &root_entries).unwrap();
        amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap()
    };
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = if index_mode {
        amiss(&[
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
        ])
    } else {
        amiss(&[
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
        ])
    };
    (code, payload(&stdout))
}

/// A name the byte grammar refuses, such as one carrying a backslash, still
/// voids the run: dropping the entry quietly would be the worst bug this tool
/// could have, because the report would come back complete and passing with a
/// document simply absent from it. The defect is a retained analysis error
/// with the exact bytes, the run is incomplete, and the exit is 2. Under the
/// second contract this refusal is structural only; spelling is no longer a
/// reason.
#[test]
fn a_document_the_grammar_refuses_is_still_refused_rather_than_dropped() {
    let name = b"docs\\hidden.md".as_slice();
    for index_mode in [false, true] {
        let (code, payload) = hidden_entry(name, index_mode);
        let where_from = if index_mode { "index" } else { "tree" };
        assert_eq!(
            code, 2,
            "{where_from}: an unnameable document is not a pass"
        );
        assert_eq!(payload["result"]["complete"], false, "{where_from}");
        assert_eq!(payload["result"]["status"], "incomplete", "{where_from}");
        let row = payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["code"] == "UNREPRESENTABLE_PATH")
            .unwrap_or_else(|| panic!("{where_from}: the defect is disclosed, not swallowed"));
        let hex: String = name.iter().fold(String::new(), |mut out, byte| {
            let _infallible = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
            out
        });
        assert_eq!(
            row["path"],
            serde_json::Value::Null,
            "{where_from}: a name the grammar refuses is not a path value"
        );
        assert_eq!(
            row["path_bytes_hex"].as_str(),
            Some(hex.as_str()),
            "{where_from}: the refused bytes are disclosed exactly, not dropped"
        );
        assert!(
            payload["documents"].as_array().unwrap().is_empty(),
            "{where_from}: an incomplete run publishes no document set to mistake for coverage"
        );
        if index_mode {
            let candidate = &payload["evaluation"]["candidate"];
            assert_eq!(
                candidate["kind"], "unavailable",
                "an index with a row the identity cannot spell has no identity"
            );
            assert_eq!(
                candidate["snapshot_digest"],
                serde_json::Value::Null,
                "no digest may claim complete-logical-index over a partial view"
            );
        }
    }
}

/// The second contract's inversion: a name that is raw bytes rather than text
/// is a document, not a defect. The entry is scanned, the report carries its
/// path as the `bytes_hex` object, the run completes, and nothing is hidden;
/// re-adding a spelling gate anywhere in discovery fails this test.
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
/// hidden. Under the second contract the refusals are structural.
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

/// The other way out of the path domain is length. `RepoPath` stops at 4,096 bytes,
/// the snapshot charges a raw-path budget with the same ceiling, and Git will carry
/// a name longer than either. The budget is charged first, so the answer is not a
/// bare refusal but a crossing that names the resource and both numbers, and the run
/// is still incomplete with nothing to mistake for a result.
#[test]
fn a_path_longer_than_the_domain_allows_is_a_charged_crossing_not_a_silent_skip() {
    let long = format!("docs/{}.md", "x".repeat(5000));
    let (code, payload) = hidden_entry(long.as_bytes(), false);

    assert_eq!(code, 2, "an over-long path is not a passing run");
    assert_eq!(payload["result"]["complete"], false);
    let errors = payload["errors"].as_array().unwrap();
    let crossing = errors
        .iter()
        .find(|error| error["code"] == "RESOURCE_LIMIT_EXCEEDED")
        .expect("the crossing is disclosed");
    assert_eq!(crossing["resource"], "raw-path-bytes");
    assert_eq!(crossing["configured_limit"], 4096);
    assert!(
        crossing["observed_lower_bound"].as_u64().unwrap() > 4096,
        "the crossing reports how far over it went"
    );
    assert!(
        payload["documents"].as_array().unwrap().is_empty(),
        "an incomplete run publishes no document set to mistake for coverage"
    );
}

/// A shallow clone hands the scanner a base OID whose object was never fetched.
/// The tempting failure is to treat an absent base as an empty one and report
/// every finding as introduced, which turns the cheapest checkout misconfiguration
/// into a wall of false accusations, or worse, to skip the comparison and pass.
/// The store not holding the object is not a judgment the scanner can make
/// anything of: the run refuses, names the defect, and publishes nothing.
#[test]
fn a_base_the_store_does_not_hold_is_a_refusal_not_a_guess() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "only"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let ghost = "a".repeat(40);

    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &ghost,
        "--candidate",
        &candidate,
        "--profile",
        "enforce",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2, "an absent base is untrustworthy, not empty");
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["complete"], false);
    assert_eq!(payload["result"]["status"], "incomplete");
    let codes: Vec<&str> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| error["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"GIT_OBJECT_MISSING"),
        "the refusal names the absent object: {codes:?}"
    );
    assert!(
        payload["documents"].as_array().unwrap().is_empty(),
        "no document set to mistake for a comparison that never ran"
    );
}

/// The partial-clone twin: the commits and trees are all present and one tracked
/// blob is not, which is exactly what a promisor remote leaves behind. Git would
/// fetch it on demand; this scanner has no network on purpose, so the only honest
/// move is the same refusal, and in commit mode it names the document whose bytes
/// it could not have. The object store is arranged by hand here, staging the blob
/// and then deleting the loose object, because no porcelain command will build a
/// tree it cannot read.
#[test]
fn a_tracked_blob_the_store_does_not_hold_refuses_and_names_the_document() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# R\n\n[g](docs/guide.md)\n").unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("docs/promised.md"), "# Promised\n").unwrap();
    git(root, &["add", "docs/promised.md"]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let blob = git(root, &["rev-parse", "HEAD:docs/promised.md"])
        .trim()
        .to_owned();
    let (dir_part, file_part) = blob.split_at(2);
    fs::remove_file(root.join(".git/objects").join(dir_part).join(file_part)).unwrap();

    let repo = amiss_fixtures::path_arg(root);
    for index_mode in [false, true] {
        let mode = if index_mode { "index" } else { "commit" };
        let args: Vec<&str> = if index_mode {
            vec![
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
                "json",
            ]
        } else {
            vec![
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
                "enforce",
                "--format",
                "json",
            ]
        };
        let (code, stdout) = amiss(&args);
        assert_eq!(code, 2, "{mode}: a blob it cannot read is not a pass");
        let payload = payload(&stdout);
        assert_eq!(payload["result"]["complete"], false, "{mode}");
        let missing: Vec<(&str, Option<&str>)> = payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|error| error["code"] == "GIT_OBJECT_MISSING")
            .map(|error| ("GIT_OBJECT_MISSING", error["path"].as_str()))
            .collect();
        assert!(!missing.is_empty(), "{mode}: the absence is disclosed");
        if !index_mode {
            assert!(
                missing
                    .iter()
                    .any(|(_, path)| *path == Some("docs/promised.md")),
                "commit mode names the document the store cannot produce: {missing:?}"
            );
        }
        assert!(
            payload["documents"].as_array().unwrap().is_empty(),
            "{mode}: an incomplete run publishes no document set"
        );
    }
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
        &[("100644", b"design-\xff.rst".as_slice(), hidden.as_str())],
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
        .find(|row| row["path"]["bytes_hex"] == "73706563732f64657369676e2dff2e727374")
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

/// The rolling preimage law, enforced: a reference-scoped finding key embeds
/// only content-derived values, and the same pinned repository always yields
/// the same identity under the current unversioned domain.
#[test]
fn a_text_repository_has_a_reproducible_finding_key() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# R\n\n[g](docs/guide.md)\n").unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md) and [gone](missing.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        (base.as_str(), candidate.as_str()),
        (
            "989d8153fdf533e0e1eb55b971cafa4b81e4612c",
            "a806e16842c7e4cb686c7f5b9977fb80226b49ca",
        ),
        "the pinned identity and dates make the fixture byte-reproducible"
    );
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
    let finding = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        finding["finding_key"],
        "sha256:2bb58978450a0f6051e47e92a2b8ea777b9e8fc5cea5a6319bff3c2e691262b2",
        "the pinned repository fixes the current rolling-contract identity"
    );
}
