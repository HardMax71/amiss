#![expect(
    clippy::unwrap_used,
    reason = "a fixture that cannot be built is a test failure"
)]

use amiss_fixtures::{commit_pair, git, real_git};

#[test]
fn built_repositories_hash_like_git() {
    let built = commit_pair(
        &[("README.md", "base\n"), ("docs/guide.md", "one\n")],
        &[("README.md", "candidate\n")],
    )
    .unwrap();

    let reference = tempfile::TempDir::new().unwrap();
    let root = reference.path();
    real_git(root, &["init", "-q"]).unwrap();
    stage(root, "README.md", "base\n");
    stage(root, "docs/guide.md", "one\n");
    real_git(root, &["add", "."]).unwrap();
    real_git(root, &["commit", "-q", "--allow-empty", "-m", "base"]).unwrap();
    let base = real_git(root, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_owned();
    stage(root, "README.md", "candidate\n");
    real_git(root, &["add", "."]).unwrap();
    real_git(root, &["commit", "-q", "--allow-empty", "-m", "candidate"]).unwrap();
    let candidate = real_git(root, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_owned();

    assert_eq!(built.base, base, "base commit");
    assert_eq!(built.candidate, candidate, "candidate commit");
}

fn stage(root: &std::path::Path, path: &str, body: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, body).unwrap();
}

/// The same history written twice: once through the fixture vocabulary,
/// which answers in process, and once by git itself.
fn twice(
    steps: impl Fn(&std::path::Path, &dyn Fn(&std::path::Path, &[&str]) -> String),
) -> [tempfile::TempDir; 2] {
    let fixture = tempfile::TempDir::new().unwrap();
    let reference = tempfile::TempDir::new().unwrap();
    steps(fixture.path(), &|root, args| git(root, args).unwrap());
    steps(reference.path(), &|root, args| {
        real_git(root, args).unwrap()
    });
    [fixture, reference]
}

fn history(root: &std::path::Path, run: &dyn Fn(&std::path::Path, &[&str]) -> String) {
    run(root, &["init", "-q"]);
    stage(root, "README.md", "# R\n");
    stage(root, "docs/guide.md", "one\n");
    stage(root, "docs/nested/deep.md", "two\n");
    run(root, &["add", "."]);
    run(root, &["commit", "-qm", "base"]);
    stage(root, "README.md", "# R2\n");
    std::fs::remove_file(root.join("docs/guide.md")).unwrap();
    stage(root, "docs/added.md", "three\n");
    run(root, &["add", "."]);
    run(root, &["commit", "-q", "-m", "candidate"]);
}

#[test]
fn the_fixture_vocabulary_writes_what_git_writes() {
    let [fixture, reference] = twice(history);

    for args in [
        ["rev-parse", "HEAD"].as_slice(),
        &["rev-parse", "HEAD~1"],
        &["ls-files", "-s"],
        &["ls-tree", "-r", "HEAD"],
        &["status", "--porcelain"],
    ] {
        assert_eq!(
            real_git(fixture.path(), args).unwrap(),
            real_git(reference.path(), args).unwrap(),
            "{args:?}"
        );
    }
    assert_eq!(
        git(fixture.path(), &["rev-parse", "HEAD"]).unwrap(),
        real_git(fixture.path(), &["rev-parse", "HEAD"]).unwrap()
    );
    real_git(fixture.path(), &["fsck", "--strict"]).unwrap();
}

#[test]
fn git_edits_between_add_and_commit_are_honored() {
    let [fixture, reference] = twice(|root, run| {
        run(root, &["init", "-q"]);
        stage(root, "tool.sh", "#!/bin/sh\n");
        run(root, &["add", "."]);
        real_git(root, &["update-index", "--chmod=+x", "tool.sh"]).unwrap();
        run(root, &["commit", "-qm", "executable"]);
    });

    let listing = real_git(fixture.path(), &["ls-tree", "HEAD"]).unwrap();
    assert!(listing.starts_with("100755 blob"), "{listing}");
    assert_eq!(
        real_git(fixture.path(), &["rev-parse", "HEAD"]).unwrap(),
        real_git(reference.path(), &["rev-parse", "HEAD"]).unwrap()
    );
}

#[test]
fn an_ignore_file_hands_the_add_back_to_git() {
    let [fixture, reference] = twice(|root, run| {
        run(root, &["init", "-q"]);
        stage(root, ".gitignore", "*.log\n");
        stage(root, "kept.md", "kept\n");
        stage(root, "noise.log", "noise\n");
        run(root, &["add", "."]);
        run(root, &["commit", "-qm", "ignored"]);
    });

    let names = real_git(fixture.path(), &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap();
    assert_eq!(names, ".gitignore\nkept.md\n");
    assert_eq!(
        real_git(fixture.path(), &["rev-parse", "HEAD"]).unwrap(),
        real_git(reference.path(), &["rev-parse", "HEAD"]).unwrap()
    );
}

#[test]
fn an_unchanged_tree_refuses_to_commit_like_git() {
    let [fixture, reference] = twice(|root, run| {
        run(root, &["init", "-q"]);
        stage(root, "README.md", "# R\n");
        run(root, &["add", "."]);
        run(root, &["commit", "-qm", "base"]);
    });

    assert!(git(fixture.path(), &["commit", "-qm", "again"]).is_err());
    assert!(real_git(reference.path(), &["commit", "-qm", "again"]).is_err());
}
