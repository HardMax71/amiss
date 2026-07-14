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
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
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
  "schema": "amiss/scanner-policy/v1",
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

    let repo = root.to_str().unwrap().to_owned();
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

    let repo = root.to_str().unwrap().to_owned();
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
    let repo = root.to_str().unwrap().to_owned();
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

/// A repository can try to hide a document by giving it a name the scanner has no
/// way to write down: bytes that are not UTF-8, or a path `RepoPath` refuses, such
/// as one carrying a backslash. Dropping that entry quietly would be the worst bug
/// this tool could have, because the report would come back complete and passing
/// with a document simply absent from it, and the absence is the thing nobody can
/// see. So there is no report at all: the path defect is a retained analysis error,
/// the run is incomplete, and the exit is 2.
#[test]
fn a_document_the_scanner_cannot_name_is_refused_rather_than_dropped() {
    for name in [
        b"docs\\hidden.md".as_slice(),
        b"docs/bad-\xff-name.md".as_slice(),
    ] {
        for index_mode in [false, true] {
            let (code, payload) = hidden_entry(name, index_mode);
            let where_from = if index_mode { "index" } else { "tree" };
            assert_eq!(
                code, 2,
                "{where_from}: an unnameable document is not a pass"
            );
            assert_eq!(payload["result"]["complete"], false, "{where_from}");
            assert_eq!(payload["result"]["status"], "incomplete", "{where_from}");
            let codes: Vec<&str> = payload["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|error| error["code"].as_str().unwrap())
                .collect();
            assert!(
                codes.contains(&"UNREPRESENTABLE_PATH"),
                "{where_from}: the defect is disclosed, not swallowed: {codes:?}"
            );
            assert!(
                payload["documents"].as_array().unwrap().is_empty(),
                "{where_from}: an incomplete run publishes no document set to mistake for coverage"
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

    let repo = root.to_str().unwrap().to_owned();
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

    let repo = root.to_str().unwrap().to_owned();
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
