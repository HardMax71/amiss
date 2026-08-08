use std::fs;

use amiss_fixtures::{commit_worktree, init_repository};
use tempfile::TempDir;

use crate::support::{amiss, payload};

fn author_args<'a>(root: &'a str, path: &'a str, line: &'a str, name: &'a str) -> Vec<&'a str> {
    vec![
        "claim", "--repo", root, "--path", path, "--line", line, "--name", name,
    ]
}

/// A plain line prints the double-quoted definition, byte for byte, and
/// nothing else on stdout.
#[test]
fn a_plain_line_prints_the_double_quoted_definition() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("subject.txt"), "alpha\nbeta\n").unwrap();
    let root = dir.path().to_str().unwrap();
    let (code, stdout, stderr) = amiss(&author_args(root, "subject.txt", "2", "subject-line"));
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "[amiss:subject-line]: <amiss:value?path=subject.txt&line=L2> \"beta\"\n"
    );
}

/// A line holding double quotes falls to the single-quoted spelling.
#[test]
fn a_double_quoted_line_falls_to_single_quotes() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("pin.toml"),
        "[toolchain]\nchannel = \"1.97.0\"\n",
    )
    .unwrap();
    let root = dir.path().to_str().unwrap();
    let (code, stdout, _stderr) = amiss(&author_args(root, "pin.toml", "2", "channel"));
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "[amiss:channel]: <amiss:value?path=pin.toml&line=L2> 'channel = \"1.97.0\"'\n"
    );
}

/// A bare-CR file authors through the evaluation's own line scanner, so the
/// printed claim names the line a check will actually compare.
#[test]
fn a_cr_delimited_file_authors_the_engine_line() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("legacy.txt"), b"alpha\rbeta\rgamma\r").unwrap();
    let root = dir.path().to_str().unwrap();
    let (code, stdout, _stderr) = amiss(&author_args(root, "legacy.txt", "2", "cr-line"));
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "[amiss:cr-line]: <amiss:value?path=legacy.txt&line=L2> \"beta\"\n"
    );
}

/// The money test: the printed definition, pasted into a document, evaluates
/// as one attested claim under the full check.
#[test]
fn a_printed_definition_evaluates_attested() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    fs::write(root.join("subject.txt"), "alpha\n").unwrap();
    fs::write(root.join("docs.md"), "# D\n").unwrap();
    let base = commit_worktree(root, &[], "base").unwrap().id;

    let shown = root.to_str().unwrap();
    let (code, stdout, _stderr) = amiss(&author_args(shown, "subject.txt", "1", "subject-line"));
    assert_eq!(code, 0);
    let definition = String::from_utf8(stdout).unwrap();

    fs::write(root.join("docs.md"), format!("# D\n\n{definition}")).unwrap();
    let candidate = commit_worktree(root, &[], "claimed").unwrap().id;
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        shown,
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
    ]);
    let report = payload(&stdout);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["summary"]["governed_claims"], 1, "{report}");
    assert_eq!(report["summary"]["unattested_claims"], 0, "{report}");
}

/// Every refusal answers alone: a line past the file names the count, a line
/// both quotings cannot carry refuses unspelled, and a missing file says so.
#[test]
fn refusals_answer_alone() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("mixed.txt"),
        "clean\nboth \"quote\" and 'tick'\n",
    )
    .unwrap();
    let root = dir.path().to_str().unwrap();

    let (code, _stdout, stderr) = amiss(&author_args(root, "mixed.txt", "9", "past"));
    assert_eq!(code, 1);
    assert!(stderr.contains("holds 2 lines"), "{stderr}");

    let (code, _stdout, stderr) = amiss(&author_args(root, "mixed.txt", "2", "unspellable"));
    assert_eq!(code, 1);
    assert!(stderr.contains("neither title quoting"), "{stderr}");

    let (code, _stdout, stderr) = amiss(&author_args(root, "absent.txt", "1", "gone"));
    assert_eq!(code, 1);
    assert!(stderr.contains("unreadable"), "{stderr}");

    fs::write(dir.path().join("entity.txt"), "a&amp;b\n").unwrap();
    let (code, _stdout, stderr) = amiss(&author_args(root, "entity.txt", "1", "entity"));
    assert_eq!(code, 1, "an entity-bearing line decodes away from itself");
    assert!(stderr.contains("neither title quoting"), "{stderr}");
}

/// The authoring form owns exactly four flags: scan flags, a zero line, a
/// leading-zero line, a bad name, and a spaced path are each refused by the
/// grammar.
#[test]
fn the_claim_form_refuses_foreign_and_malformed_values() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "x\n").unwrap();
    let root = dir.path().to_str().unwrap().to_owned();
    let base = author_args(&root, "a.txt", "1", "fine");
    let with = |extra: &[&str]| {
        let mut args: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args
    };
    let swapped = |flag: &str, value: &str| {
        let mut args: Vec<String> = base.iter().map(|s| (*s).to_owned()).collect();
        let at = args.iter().position(|a| a == flag).unwrap();
        args[at + 1] = value.to_owned();
        args
    };
    for args in [
        with(&["--profile", "enforce"]),
        with(&["--explain-scope"]),
        with(&["--base", &"a".repeat(40)]),
        with(&["--index"]),
        swapped("--line", "0"),
        swapped("--line", "007"),
        swapped("--name", "-bad"),
        swapped("--path", "has space.txt"),
        swapped("--path", "a?b.txt"),
        swapped("--path", "a#b.txt"),
        swapped("--line", "9007199254740992"),
    ] {
        let shown: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, _stdout, stderr) = amiss(&shown);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains("INVALID_INVOCATION"), "{stderr}");
    }
}
