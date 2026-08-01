#![expect(
    clippy::unwrap_used,
    reason = "a fixture that cannot be built is a test failure"
)]

amiss_fixtures::bounded_memory!();

use amiss_fixtures::{commit_pair, git};

#[test]
fn built_repositories_hash_like_git() {
    let built = commit_pair(
        &[("README.md", "base\n"), ("docs/guide.md", "one\n")],
        &[("README.md", "candidate\n")],
    )
    .unwrap();

    let reference = tempfile::TempDir::new().unwrap();
    let root = reference.path();
    git(root, &["init", "-q"]).unwrap();
    stage(root, "README.md", "base\n");
    stage(root, "docs/guide.md", "one\n");
    git(root, &["add", "."]).unwrap();
    git(root, &["commit", "-q", "--allow-empty", "-m", "base"]).unwrap();
    let base = git(root, &["rev-parse", "HEAD"]).unwrap().trim().to_owned();
    stage(root, "README.md", "candidate\n");
    git(root, &["add", "."]).unwrap();
    git(root, &["commit", "-q", "--allow-empty", "-m", "candidate"]).unwrap();
    let candidate = git(root, &["rev-parse", "HEAD"]).unwrap().trim().to_owned();

    assert_eq!(built.base, base, "base commit");
    assert_eq!(built.candidate, candidate, "candidate commit");
}

fn stage(root: &std::path::Path, path: &str, body: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, body).unwrap();
}
