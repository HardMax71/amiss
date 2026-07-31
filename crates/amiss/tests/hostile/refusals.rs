use std::fs;

use tempfile::TempDir;

use crate::support::{BYTE_NAME_HEX, amiss, byte_named_index, git, hidden_entry, payload};

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

/// A name the byte grammar refuses, such as one carrying a backslash, still
/// voids the run: dropping the entry quietly would be the worst bug this tool
/// could have, because the report would come back complete and passing with a
/// document simply absent from it. The defect is a retained analysis error
/// with the exact bytes, the run is incomplete, and the exit is 2. This
/// refusal is structural only; spelling is not a reason.
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

/// A byte-named document whose bytes will not decode ends the run at exit 2,
/// the human note teaches the code, and the wire carries the name as hex.
#[test]
fn a_byte_named_invalid_document_refuses_with_its_name_in_hex() {
    let (dir, base) = byte_named_index(b"# \xff\n");
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
        "observe",
    ]);
    assert_eq!(code, 2);
    let text = String::from_utf8(stdout).unwrap();
    assert!(
        text.contains(r#"error parse DOCUMENT_INVALID "bad-\u00ff-doc.md""#),
        "the error line speaks the name through the bytes atom: {text:?}"
    );
    assert!(
        text.contains("cannot be decoded"),
        "the note teaches the meaning: {text:?}"
    );

    let (json_code, wire) = amiss(&[
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
    assert_eq!(json_code, 2);
    let errors = payload(&wire)["errors"].clone();
    let row = errors
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"]["bytes_hex"] == BYTE_NAME_HEX)
        .unwrap_or_else(|| panic!("no bytes row in {errors}"));
    assert_eq!(row["code"], "DOCUMENT_INVALID");
}
