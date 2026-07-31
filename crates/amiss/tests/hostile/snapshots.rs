use std::fs;

use tempfile::TempDir;

use crate::support::{amiss, git, payload};

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
