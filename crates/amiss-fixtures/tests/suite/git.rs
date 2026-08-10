#![expect(
    clippy::expect_used,
    reason = "integration harness over asserted fixture and process shapes"
)]

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn local_environment(repository: &Path) -> Vec<String> {
    amiss_fixtures::git(repository, &["rev-parse", "--local-env-vars"])
        .expect("list repository-local environment")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn fixture_git_clears_every_local_variable_known_to_git() {
    let fixture = TempDir::new().expect("fixture directory");
    amiss_fixtures::git(fixture.path(), &["init", "-q"]).expect("initialize fixture repository");
    let names = local_environment(fixture.path());

    let mut command = Command::new("git");
    amiss_fixtures::configure_git_command(&mut command, fixture.path());
    let removed: BTreeSet<OsString> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_owned())
        .collect();
    let missing: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !removed.contains(OsStr::new(name)))
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn fixture_git_ignores_repository_local_environment() {
    let intended = TempDir::new().expect("intended fixture directory");
    let foreign = TempDir::new().expect("foreign fixture directory");
    amiss_fixtures::git(intended.path(), &["init", "-q"]).expect("initialize intended repository");
    amiss_fixtures::git(foreign.path(), &["init", "-q"]).expect("initialize foreign repository");

    let poison = foreign.path().join(".git");
    let mut command = Command::new("git");
    for name in local_environment(intended.path()) {
        command.env(name, &poison);
    }
    amiss_fixtures::configure_git_command(&mut command, intended.path());
    let output = command
        .args(["rev-parse", "--show-toplevel", "--absolute-git-dir"])
        .output()
        .expect("inspect configured repository");
    assert!(
        output.status.success(),
        "configured git command: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let paths = String::from_utf8(output.stdout).expect("git paths are utf-8 fixture paths");
    let mut paths = paths.lines();
    let actual_worktree = paths.next().expect("git worktree path");
    let actual_git_dir = paths.next().expect("git directory path");
    assert_eq!(
        fs::canonicalize(actual_worktree).expect("canonical actual worktree"),
        fs::canonicalize(intended.path()).expect("canonical intended worktree")
    );
    assert_eq!(
        fs::canonicalize(actual_git_dir).expect("canonical actual git directory"),
        fs::canonicalize(intended.path().join(".git")).expect("canonical intended git directory")
    );
    assert_eq!(paths.next(), None);
}
