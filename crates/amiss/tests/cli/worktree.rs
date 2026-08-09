use std::fs;

use crate::support::{amiss, fixture, git, payload};

fn check_args<'a>(repo: &'a str, base: &'a str, selector: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "check",
        "--repo",
        repo,
        "--object-format",
        "sha1",
        "--base",
        base,
    ];
    args.extend_from_slice(selector);
    args.extend(["--profile", "enforce", "--format", "json"]);
    args
}

/// A linked worktree answers the commit pair exactly as the primary checkout
/// does: same exit class, same findings, byte-comparable evidence.
#[test]
fn a_worktree_check_matches_the_primary_checkout() {
    let fx = fixture();
    let worktree = fx.root().join("linked-wt");
    let worktree_repo = amiss_fixtures::path_arg(&worktree);
    git(
        fx.root(),
        &["worktree", "add", "-q", &worktree_repo, &fx.candidate],
    );

    let (primary_code, primary, _stderr) = amiss(&check_args(
        &fx.repo,
        &fx.base,
        &["--candidate", &fx.candidate],
    ));
    let (worktree_code, from_worktree, stderr) = amiss(&check_args(
        &worktree_repo,
        &fx.base,
        &["--candidate", &fx.candidate],
    ));
    assert_eq!((worktree_code, stderr.as_str()), (primary_code, ""));
    assert_eq!(
        payload(&from_worktree)["findings"],
        payload(&primary)["findings"],
        "the two roots see one repository"
    );
}

/// The staged gate inside a worktree reads the worktree's own index: an edit
/// staged only there blocks there, while the primary stays clean.
#[test]
fn a_worktree_staged_check_reads_the_private_index() {
    let fx = fixture();
    let worktree = fx.root().join("staged-wt");
    let worktree_repo = amiss_fixtures::path_arg(&worktree);
    git(
        fx.root(),
        &["worktree", "add", "-q", &worktree_repo, &fx.base],
    );
    fs::write(
        worktree.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [dead](nowhere.md)\n",
    )
    .unwrap();
    git(&worktree, &["add", "docs/guide.md"]);

    let targets = |bytes: &[u8]| -> Vec<String> {
        payload(bytes)["findings"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter(|row| row["kind"] == "explicit-target-missing")
                    .filter_map(|row| {
                        row["key_input"]["scope"]["normalized_target_intent"]["path"]
                            .as_str()
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let (code, stdout, stderr) = amiss(&check_args(&worktree_repo, &fx.base, &["--index"]));
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert_eq!(
        targets(&stdout),
        ["docs/nowhere.md"],
        "the worktree's staged break blocks, and only it"
    );

    let (_primary_code, primary, _stderr) = amiss(&check_args(&fx.repo, &fx.base, &["--index"]));
    assert!(
        !targets(&primary).contains(&"docs/nowhere.md".to_owned()),
        "the primary index never saw the worktree's staged edit"
    );
}
