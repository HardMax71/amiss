#![expect(clippy::unwrap_used, reason = "test fixture plumbing")]

use std::fs;
use std::path::Path;

use amiss_fixtures::{commit_worktree, git, init_repository};
use tempfile::TempDir;

use crate::support::amiss;

fn staged_repo(link: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    fs::write(root.join("sections.md"), "# Sections\n").unwrap();
    fs::write(root.join("guide.md"), "# G\n").unwrap();
    let base = commit_worktree(root, &[], "base").unwrap().id;
    fs::write(root.join("guide.md"), link).unwrap();
    git(root, &["add", "."]).unwrap();
    (dir, base)
}

fn fix_args<'a>(root: &'a str, base: &'a str) -> Vec<&'a str> {
    vec![
        "fix",
        "--repo",
        root,
        "--object-format",
        "sha1",
        "--base",
        base,
        "--index",
        "--profile",
        "enforce",
    ]
}

fn run_fix(root: &Path, base: &str) -> (i32, String) {
    let shown = root.to_str().unwrap();
    let (code, stdout, _stderr) = amiss(&fix_args(shown, base));
    (code, String::from_utf8(stdout).unwrap())
}

#[test]
fn byte_named_documents_keep_findings_without_invalid_fixes() {
    use amiss_wire::report::{FindingKind, validate_envelope};

    for (input, expected, kind) in [
        (
            "[a](Sections.md)\n",
            "[a](sections.md)\n",
            FindingKind::ExplicitTargetMissing,
        ),
        (
            "[a](sections.md#Sections)\n",
            "[a](sections.md#sections)\n",
            FindingKind::ExplicitTargetMissing,
        ),
        (
            "[value][amiss:section].\n\n[amiss:section]: <amiss:value?path=sections.md&line=L1> \"old\"\n",
            "[value][amiss:section].\n\n[amiss:section]: <amiss:value?path=sections.md&line=L1> \"# Sections\"\n",
            FindingKind::ClaimBroken,
        ),
    ] {
        let (dir, base) = staged_repo(input);
        let root = dir.path();
        let document = amiss_fixtures::loose_object(root, "blob", input.as_bytes()).unwrap();
        let sections = git(root, &["rev-parse", "HEAD:sections.md"]).unwrap();
        amiss_fixtures::index_file(
            root,
            &[
                (b"guide.md".as_slice(), document.as_str()),
                (b"sections.md".as_slice(), sections.trim()),
                (b"\xff.md".as_slice(), document.as_str()),
            ],
        )
        .unwrap();
        let mut args = fix_args(root.to_str().unwrap(), &base);
        args[0] = "check";
        args.extend(["--format", "json"]);
        let (code, bytes, stderr) = amiss(&args);
        assert_eq!((code, stderr.as_str()), (1, ""));
        let (payload, _, _) = validate_envelope(&bytes).unwrap();
        let rows: Vec<_> = payload
            .findings
            .iter()
            .filter(|row| row.kind == kind)
            .collect();
        assert_eq!(rows.len(), 2, "{input}");
        assert_eq!(rows.iter().filter(|row| row.fix.is_some()).count(), 1);
        assert!(
            rows.iter()
                .any(|row| row.location.path.as_ref().is_some_and(|path| {
                    matches!(path, amiss_wire::report::model::RepoPath::Bytes(_))
                }) && row.fix.is_none())
        );
        let (code, stdout) = run_fix(root, &base);
        assert_eq!(code, 0, "{stdout}");
        assert!(stdout.contains("fixed guide.md"), "{stdout}");
        assert_eq!(
            fs::read(root.join("guide.md")).unwrap(),
            expected.as_bytes()
        );
    }
}

/// The staged case-drifted path is rewritten in place, byte for byte, and a
/// second run finds the repair already present instead of drifting.
#[test]
fn a_case_drifted_path_is_repaired_in_place() {
    let (dir, base) = staged_repo("[a](Sections.md)\n");
    let (code, stdout) = run_fix(dir.path(), &base);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("fixed guide.md"), "{stdout}");
    let repaired = fs::read(dir.path().join("guide.md")).unwrap();
    assert_eq!(repaired, b"[a](sections.md)\n");

    let (code, stdout) = run_fix(dir.path(), &base);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("already fixed guide.md"), "{stdout}");
    assert!(stdout.contains("0 applied, 1 already present"), "{stdout}");
}

/// A worktree that moved past the staged bytes is refused whole, exits 1,
/// and keeps its bytes.
#[test]
fn a_drifted_worktree_refuses_the_repair() {
    let (dir, base) = staged_repo("[a](Sections.md)\n");
    fs::write(dir.path().join("guide.md"), "[a](Sections.md)\nmeddled\n").unwrap();
    let (code, stdout) = run_fix(dir.path(), &base);
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("refused guide.md: worktree differs"),
        "{stdout}"
    );
    let kept = fs::read(dir.path().join("guide.md")).unwrap();
    assert_eq!(kept, b"[a](Sections.md)\nmeddled\n");
}

/// A worktree path that is no longer a regular file is never written through.
#[cfg(unix)]
#[test]
fn a_symlinked_document_refuses_the_repair() {
    let (dir, base) = staged_repo("[a](Sections.md)\n");
    let root = dir.path();
    fs::remove_file(root.join("guide.md")).unwrap();
    std::os::unix::fs::symlink(root.join("sections.md"), root.join("guide.md")).unwrap();
    let (code, stdout) = run_fix(root, &base);
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("refused guide.md: not a regular worktree file"),
        "{stdout}"
    );
}

/// Two disjoint fixes in one document land together in one pass.
#[test]
fn two_fixes_in_one_document_apply_together() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    init_repository(root).unwrap();
    fs::write(root.join("sections.md"), "# Sections\n").unwrap();
    fs::write(root.join("extra.md"), "# Extra\n").unwrap();
    fs::write(root.join("guide.md"), "# G\n").unwrap();
    let base = commit_worktree(root, &[], "base").unwrap().id;
    fs::write(root.join("guide.md"), "[a](Sections.md)\n[b](Extra.md)\n").unwrap();
    git(root, &["add", "."]).unwrap();
    let (code, stdout) = run_fix(root, &base);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("fixed guide.md: 2"), "{stdout}");
    let repaired = fs::read(root.join("guide.md")).unwrap();
    assert_eq!(repaired, b"[a](sections.md)\n[b](extra.md)\n");
}

/// Findings that carry no fix leave the tree untouched and exit 0, saying so.
#[test]
fn findings_without_fixes_apply_nothing() {
    let (dir, base) = staged_repo("[a](absent.md)\n");
    let (code, stdout) = run_fix(dir.path(), &base);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("no fixes to apply"), "{stdout}");
    let kept = fs::read(dir.path().join("guide.md")).unwrap();
    assert_eq!(kept, b"[a](absent.md)\n");
}

/// An evaluation the engine cannot trust applies nothing and exits 2.
#[test]
fn an_untrusted_evaluation_applies_nothing() {
    let (dir, _base) = staged_repo("[a](Sections.md)\n");
    let missing = "a".repeat(40);
    let (code, _stdout) = run_fix(dir.path(), &missing);
    assert_eq!(code, 2);
    let kept = fs::read(dir.path().join("guide.md")).unwrap();
    assert_eq!(kept, b"[a](Sections.md)\n");
}

/// The repair form owns no output or candidate flags: each is refused as an
/// invalid invocation rather than ignored.
#[test]
fn the_fix_form_refuses_check_only_flags() {
    let (dir, base) = staged_repo("[a](Sections.md)\n");
    let root = dir.path().to_str().unwrap().to_owned();
    for extra in [
        vec!["--format", "json"],
        vec!["--explain-scope"],
        vec!["--candidate", &base],
    ] {
        let mut args = fix_args(&root, &base);
        if extra == ["--candidate", base.as_str()] {
            args.retain(|argument| *argument != "--index");
        }
        args.extend(extra.iter());
        let (code, _stdout, stderr) = amiss(&args);
        assert_eq!(code, 2, "{stderr}");
        assert!(
            stderr.contains("INVALID_INVOCATION") || stderr.is_empty(),
            "{stderr}"
        );
    }
}
