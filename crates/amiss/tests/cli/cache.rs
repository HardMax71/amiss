use std::fs;
use std::path::Path;

use amiss_fixtures::{commit_worktree, git, init_repository};
use tempfile::TempDir;

use crate::support::amiss;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn staged_repo() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    fs::write(
        root.join("guide.md"),
        "# Guide\n\nSee [the notes](notes.md).\n",
    )
    .unwrap();
    fs::write(root.join("notes.md"), "# Notes\n").unwrap();
    let base = commit_worktree(root, &[], "base").unwrap().id;
    fs::write(
        root.join("guide.md"),
        "# Guide\n\nSee [the notes](notes.md#top).\n",
    )
    .unwrap();
    git(root, &["add", "."]).unwrap();
    (dir, base)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn check(root: &Path, base: &str, store: &Path) -> (i32, Vec<u8>, String) {
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        root.to_str().unwrap(),
        "--object-format",
        "sha1",
        "--base",
        base,
        "--index",
        "--profile",
        "observe",
        "--format",
        "json",
        "--scan-cache",
        store.to_str().unwrap(),
    ]);
    (code, stdout, stderr)
}

#[expect(clippy::unwrap_used, reason = "asserted fixture shape")]
fn rows(root: &Path) -> usize {
    let mut count = 0_usize;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

#[test]
fn a_cached_second_run_prints_the_same_report() {
    let (repo, base) = staged_repo();
    let store = TempDir::new().unwrap();
    let (first_code, first, stderr) = check(repo.path(), &base, store.path());
    assert_eq!(first_code, 0, "{stderr}");
    assert_eq!(rows(store.path()), 3);
    let (second_code, second, stderr) = check(repo.path(), &base, store.path());
    assert_eq!(second_code, 0, "{stderr}");
    assert_eq!(first, second);
}

#[test]
fn the_cache_flag_belongs_to_the_scanning_forms_only() {
    let (code, _stdout, stderr) = amiss(&[
        "render",
        "--report",
        "report.json",
        "--format",
        "junit",
        "--scan-cache",
        "somewhere",
    ]);
    assert_eq!(code, 2);
    assert!(stderr.contains("INVALID_INVOCATION"), "{stderr}");
}
