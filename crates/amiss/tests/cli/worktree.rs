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

/// A staged check names the documents whose working copies moved past what
/// is staged, on stderr, since it judged the staged bytes and a pass over
/// them says nothing about the edit still in the worktree.
#[test]
fn a_staged_check_names_the_documents_left_unstaged() {
    let fx = fixture();
    let root = fx.root();
    let staged = check_args(&fx.repo, &fx.candidate, &["--index"]);
    let (code, _stdout, stderr) = amiss(&staged);
    assert_eq!(code, 1, "the fixture's own broken link: {stderr}");
    assert!(
        !stderr.contains("note:"),
        "a clean worktree says nothing: {stderr}"
    );

    fs::write(
        root.join("docs/guide.md"),
        "# Guide\r\n\r\n[home](../README) and [gone](missing.md)\r\n",
    )
    .unwrap();
    let (_code, _stdout, stderr) = amiss(&staged);
    assert!(
        !stderr.contains("note:"),
        "a CRLF checkout of the staged bytes is what git status calls clean: {stderr}"
    );

    fs::write(root.join("docs/guide.md"), "# Guide\n\n[gone](gone.md)\n").unwrap();
    fs::remove_file(root.join("README")).unwrap();
    let (code, _stdout, stderr) = amiss(&staged);
    assert_eq!(code, 1, "the verdict is the staged bytes': {stderr}");
    assert!(
        stderr.contains(
            "amiss: note: 2 staged documents differ from their working copies, which --index does not read; git add them to check them: \"README\", \"docs/guide.md\"\n"
        ),
        "{stderr}"
    );
    let (_code, _stdout, stderr) = amiss(&check_args(
        &fx.repo,
        &fx.base,
        &["--candidate", &fx.candidate],
    ));
    assert!(
        !stderr.contains("note:"),
        "a commit pair reads no worktree: {stderr}"
    );
}
