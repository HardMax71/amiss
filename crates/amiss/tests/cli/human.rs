use std::collections::BTreeMap;
use std::fs;

use tempfile::TempDir;

use amiss_wire::report::model::occurrences;
use amiss_wire::report::{Disposition, FindingKind};

use crate::support::{amiss, fixture, git, payload, report};

#[test]
fn human_output_projects_the_same_result() {
    let fx = fixture();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.starts_with("amiss: pass (fix 1, check 1, pre-existing 0, errors 0, exit 0)"),
        "got: {text}"
    );
    assert!(
        text.contains(
            "Fix target \"docs/missing.md\" affected places 1 explicit-target-missing path-not-found\n  \"docs/guide.md\":3:23\n"
        ),
        "the row names its target, count, kind, and reason, the place under it the document and position: {text}"
    );
    assert!(
        text.contains(
            "Check target \"docs/guide.md\" affected places 1 dependency-changed-subject-unchanged\n  \"README\":1:5\n"
        ),
        "the unchanged backlink becomes one check with its place: {text}"
    );
    for kind in [
        FindingKind::ExplicitTargetMissing,
        FindingKind::DependencyChangedSubjectUnchanged,
    ] {
        assert_eq!(
            text.matches(&format!("note {}: {}\n", kind.as_ref(), kind.meaning()))
                .count(),
            1,
            "each kind shown is explained once: {text}"
        );
    }
    assert!(
        text.contains("references: extracted "),
        "totals close the projection"
    );
    assert!(!text.contains('\r'), "LF-only stdout");
    assert!(
        !text.contains("feedback overflow"),
        "two items are not an overflow: {text}"
    );

    let (_code, json, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    let mut recorded: BTreeMap<FindingKind, u64> = BTreeMap::new();
    for finding in &report(&json).payload.findings {
        if finding.effective_disposition == Disposition::Record {
            let count = recorded.entry(finding.kind).or_default();
            *count = count.saturating_add(1);
        }
    }
    let listed: Vec<String> = recorded
        .iter()
        .map(|(kind, count)| format!("{} {count}", kind.as_ref()))
        .collect();
    assert!(
        !listed.is_empty() && text.ends_with(&format!("\nrecords: {}\n", listed.join(", "))),
        "the record-only kinds are named with their counts after the totals: {text}"
    );
}

/// One note per code, however many rows carry it.
#[test]
fn repeated_error_codes_are_explained_once() {
    let fx = fixture();
    let root = fx.root();
    let governed = "A claim [here][amiss:claim].\n\n\
         [amiss:claim]: ./subject.md \"claim\"\n\
         [amiss:claim]: ./subject.md \"claim\"\n";
    fs::write(root.join("docs/first.md"), governed).unwrap_or_default();
    fs::write(root.join("docs/second.md"), governed).unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "two governed documents"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 2, "reserved directives leave the run incomplete");
    let text = String::from_utf8_lossy(&stdout);
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("error policy UNSUPPORTED_CAPABILITY"))
            .count(),
        2,
        "both documents report their own error row: {text}"
    );
    assert_eq!(
        text.matches("note UNSUPPORTED_CAPABILITY:").count(),
        1,
        "the meaning is stated once for the code, not once per row: {text}"
    );
}

/// The run says when identity absence, not reality, made URLs external.
#[test]
fn undeclared_identity_is_named_beside_the_external_count() {
    let fx = fixture();
    let root = fx.root();
    fs::write(
        root.join("docs/links.md"),
        "See [the widget docs](https://github.com/acme/widgets).\n",
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "an external link"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let bare = [
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
    ];
    let (code, stdout, _stderr) = amiss(&bare);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains(
            "references: without --repository <host>/<owner>/<name> --ref refs/heads/<branch> --default-branch-ref refs/heads/<default> (and --forge <dialect> on a self-hosted host) a same-repository URL counts as external"
        ),
        "the missing flags are named beside the count: {text}"
    );
    let declared: Vec<&str> = bare
        .iter()
        .copied()
        .chain([
            "--repository",
            "github.com/acme/other",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
        ])
        .collect();
    let (code, stdout, _stderr) = amiss(&declared);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains(" external 1 "),
        "the foreign URL stays external: {text}"
    );
    assert!(
        !text.contains("--default-branch-ref refs/heads/<default>"),
        "a declared identity silences the line: {text}"
    );
}

#[test]
fn pre_existing_findings_render_as_pre_existing_rows_with_the_kind_note() {
    let fx = fixture();
    let root = fx.root();
    fs::write(root.join("source.rs"), "pub fn untouched() {}\n").unwrap_or_default();
    git(root, &["add", "source.rs"]);
    git(root, &["commit", "-qm", "unrelated"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.starts_with("amiss: pass (fix 0, check 0, pre-existing 1, errors 0, exit 0)"),
        "got: {text}"
    );
    assert!(
        text.contains(
            "Pre-existing target \"docs/missing.md\" affected places 1 explicit-target-missing path-not-found\n  \"docs/guide.md\":3:23\n"
        ),
        "the backlog renders under its own label with its place: {text}"
    );
    assert!(!text.lines().any(|line| line.starts_with("Fix ")), "{text}");
    assert_eq!(
        text.matches("note explicit-target-missing:").count(),
        1,
        "the kind shown is explained once: {text}"
    );
    assert!(
        text.contains("findings: total "),
        "raw totals still expose the inventory: {text}"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the window, its places, and the full replay in one run"
)]
fn human_feedback_stops_at_ten_items_with_explicit_overflow() {
    let fx = fixture();
    let root = fx.root();
    let mut links = Vec::new();
    for index in 0..201 {
        links.push(format!("[l{index}](absent-{index}.md)"));
    }
    let body = format!("# Many\n\n{}\n", links.join("\n\n"));
    fs::write(root.join("docs/many.md"), body).unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "many"]);
    let many = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &many,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    let detail_lines = text.lines().filter(|line| line.starts_with("Fix ")).count();
    assert_eq!(
        detail_lines, 10,
        "only the first ten grouped feedback items are shown"
    );
    assert!(
        text.starts_with("amiss: pass (fix 201, check 0, pre-existing 1, errors 0, exit 0)"),
        "the header counts the complete grouped projection: {text}"
    );
    assert!(
        text.contains("feedback overflow: 191 more in the full report"),
        "the fix window overflows without counting the backlog: {text}"
    );
    assert!(
        text.contains("Pre-existing target \"docs/missing.md\" affected places 1"),
        "the backlog window survives two hundred introduced items: {text}"
    );
    assert!(
        !text.contains("pre-existing overflow"),
        "one backlog item is not an overflow: {text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("  \"docs/"))
            .count(),
        11,
        "one place under each shown row and no more: {text}"
    );
    assert_eq!(
        text.matches("note explicit-target-missing:").count(),
        1,
        "the one kind is explained once however many rows show it: {text}"
    );

    let (_code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &many,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    let payload = payload(&stdout);
    assert_eq!(
        payload["feedback"]["items"].as_array().map(Vec::len),
        Some(202),
        "the report retains every item; only presentation is capped"
    );
    let report_path = format!("{}/many-report.json", fx.repo);
    assert!(
        fs::write(&report_path, stdout).is_ok(),
        "write the canonical report"
    );
    let (code, stdout, stderr) = amiss(&[
        "render",
        "--report",
        &report_path,
        "--format",
        "human",
        "--full",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let text = String::from_utf8_lossy(&stdout);
    let fixes = text.lines().filter(|line| line.starts_with("Fix ")).count();
    let existing = text
        .lines()
        .filter(|line| line.starts_with("Pre-existing "))
        .count();
    assert_eq!(
        (fixes, existing),
        (201, 1),
        "full replay prints every feedback item"
    );
    assert!(!text.contains(" overflow:"), "{text}");
}

/// The carried backlog is listed, not only counted: a pre-existing broken
/// reference renders one Existing item after Fixes and Checks, under observe
/// where it warns and under enforce where it blocks the run.
#[test]
fn pre_existing_findings_render_as_existing_items() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "See [setup](docs/setup.md).\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("NOTES.md"), "# Notes\n\n[readme](README.md)\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = amiss_fixtures::path_arg(root);
    for (profile, expected_exit) in [("observe", 0), ("enforce", 1)] {
        let (code, stdout, _stderr) = amiss(&[
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
            profile,
        ]);
        assert_eq!(code, expected_exit, "profile {profile}");
        let text = String::from_utf8_lossy(&stdout);
        assert!(
            text.contains("Pre-existing target \"docs/setup.md\" affected places 1"),
            "the backlog names its target under {profile}: {text}"
        );
        assert!(
            text.contains("pre-existing 1,"),
            "the header count agrees with the listed item under {profile}: {text}"
        );
    }
}

/// The backlog window is its own: ten Existing rows and an existing overflow
/// line, whatever the introduced volume beside them.
#[test]
fn the_backlog_window_caps_at_ten_with_its_own_overflow() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    for index in 0..11 {
        fs::write(
            root.join(format!("doc-{index}.md")),
            format!("[x](gone-{index}.md)\n"),
        )
        .unwrap_or_default();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("NOTES.md"), "# Notes\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout, _stderr) = amiss(&[
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
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains("pre-existing 11,"),
        "the header counts all: {text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("Pre-existing "))
            .count(),
        10,
        "ten backlog rows and no more: {text}"
    );
    assert!(
        text.contains("pre-existing overflow: 1 more in the full report"),
        "{text}"
    );
}

/// The one-commit README with a dead anchor and a dead path, checked over
/// its own staged state under enforce: two rows, one place each with its
/// position and reason, the kind explained once, and nothing else.
#[test]
fn every_row_names_its_places_reasons_and_meaning_verbatim() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(
        root.join("README.md"),
        "# Setup\n\nSee [steps](#setup-steps) and [the guide](docs/guide.md).\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout, stderr) = amiss(&[
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
    ]);
    assert_eq!((code, stderr.as_str()), (1, ""));
    let expected = format!(
        "amiss: fail (fix 0, check 0, pre-existing 2, errors 0, exit 1)\n\
         Pre-existing target \"README.md\" affected places 1 explicit-target-missing heading-anchor-not-found\n\
         \x20 \"README.md\":3:5\n\
         Pre-existing target \"docs/guide.md\" affected places 1 explicit-target-missing path-not-found\n\
         \x20 \"README.md\":3:31\n\
         note explicit-target-missing: {}\n\
         documents: discovered 1 scanned 1 unsupported 0 excluded 0 unlinked 0\n\
         references: extracted 2 local 2 same-repo 0 external 0 unsupported 0 missing 2\n\
         findings: total 2 fail 2 warn 0 record 0\n",
        FindingKind::ExplicitTargetMissing.meaning()
    );
    assert_eq!(String::from_utf8(stdout).unwrap(), expected);
}

/// The documents a run did not scan are named under the same ten-line window
/// the feedback rows take, and a full replay names all of them. Without the
/// list a reader gets a total and no way to learn which files it counts.
#[test]
fn the_documents_a_run_did_not_scan_are_named_under_their_own_window() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    for index in 0..12 {
        fs::write(root.join(format!("plan-{index}.org")), "* Plan\n").unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = amiss_fixtures::path_arg(root);
    let check = |format: &str| {
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
            format,
        ])
    };

    let (code, stdout, stderr) = check("human");
    assert_eq!((code, stderr.as_str()), (0, ""));
    let text = String::from_utf8(stdout).unwrap();
    let named = |text: &str| {
        text.lines()
            .filter(|line| line.starts_with("unsupported \"plan-"))
            .count()
    };
    assert_eq!(named(&text), 10, "ten rows and no more: {text}");
    assert!(
        text.contains("unsupported overflow: 2 more in the full report"),
        "the window states what it hid: {text}"
    );
    assert!(
        text.contains("unsupported 12 "),
        "the total counts every one of them: {text}"
    );

    let (_code, wire, _stderr) = check("json");
    let report_path = format!("{repo}/unscanned.json");
    fs::write(&report_path, wire).unwrap();
    let (code, replayed, stderr) = amiss(&[
        "render",
        "--report",
        &report_path,
        "--format",
        "human",
        "--full",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        named(&String::from_utf8(replayed).unwrap()),
        12,
        "the full replay hides none of them"
    );
}

/// One target many documents point at is one row, and its places carry
/// their own ten-line window; a full replay prints all of them.
#[test]
fn the_places_under_one_row_window_independently_of_the_rows() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    for index in 0..12 {
        fs::write(
            root.join(format!("doc-{index}.md")),
            "# D\n\n[gone](gone.md)\n",
        )
        .unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = amiss_fixtures::path_arg(root);
    let args = |format: &'static str| {
        vec![
            "check".to_owned(),
            "--repo".to_owned(),
            repo.clone(),
            "--object-format".to_owned(),
            "sha1".to_owned(),
            "--base".to_owned(),
            base.clone(),
            "--candidate".to_owned(),
            candidate.clone(),
            "--profile".to_owned(),
            "observe".to_owned(),
            "--format".to_owned(),
            format.to_owned(),
        ]
    };
    let human = args("human");
    let shown: Vec<&str> = human.iter().map(String::as_str).collect();
    let (code, stdout, stderr) = amiss(&shown);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let text = String::from_utf8(stdout).unwrap();
    assert!(
        text.contains("Fix target \"gone.md\" affected places 12"),
        "twelve documents naming one target are one row: {text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("  \"doc-"))
            .count(),
        10,
        "ten places under the row and no more: {text}"
    );
    assert!(
        text.contains("  places overflow: 2 more in the full report"),
        "the place window states what it hid: {text}"
    );
    assert!(
        !text.contains("\"doc-8.md\"") && !text.contains("\"doc-9.md\""),
        "the window keeps the ten lowest documents, not ten arbitrary ones: {text}"
    );

    let json = args("json");
    let shown: Vec<&str> = json.iter().map(String::as_str).collect();
    let (_code, report, _stderr) = amiss(&shown);
    let report_path = format!("{repo}/one-target.json");
    fs::write(&report_path, report).unwrap();
    let (code, stdout, stderr) = amiss(&[
        "render",
        "--report",
        &report_path,
        "--format",
        "human",
        "--full",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let text = String::from_utf8(stdout).unwrap();
    let replayed: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  \"doc-"))
        .collect();
    assert_eq!(
        replayed.len(),
        12,
        "the full replay prints every place: {text}"
    );
    let mut ascending = replayed.clone();
    ascending.sort_unstable();
    assert_eq!(
        replayed, ascending,
        "the places read in document order, so a reader can scan down them: {text}"
    );
    assert!(!text.contains(" overflow:"), "{text}");
}

/// Anchors into one target are their own findings, so a row lists them by
/// the line a reader would scan to, not by the key that identifies them.
#[test]
fn places_in_one_document_read_by_line() {
    let fx = amiss_fixtures::commit_pair(
        &[("target.md", "# Target\n"), ("guide.md", "# Guide\n")],
        &[(
            "guide.md",
            "# Guide\n\n[e](target.md#epsilon)\n[d](target.md#delta)\n[c](target.md#gamma)\n[b](target.md#beta)\n[a](target.md#alpha)\n",
        )],
    )
    .unwrap();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8(stdout).unwrap();
    assert!(
        text.contains(
            "Fix target \"target.md\" affected places 5 explicit-target-missing heading-anchor-not-found\n  \"guide.md\":3:1\n  \"guide.md\":4:1\n  \"guide.md\":5:1\n  \"guide.md\":6:1\n  \"guide.md\":7:1\n"
        ),
        "the lines climb: {text}"
    );
}

/// A path that exists under another case is missing, and the place says
/// which spelling it nearly matched.
#[test]
fn a_case_mismatch_names_the_nearby_spelling() {
    let fx = amiss_fixtures::commit_pair(
        &[("docs/guide.md", "# Guide\n"), ("README.md", "# R\n")],
        &[("README.md", "# R\n\n[g](docs/Guide.md)\n")],
    )
    .unwrap();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "enforce",
    ]);
    assert_eq!(code, 1);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains(
            "Fix target \"docs/Guide.md\" affected places 1 explicit-target-missing path-not-found near \"docs/guide.md\"\n  \"README.md\":3:1\n"
        ),
        "{text}"
    );
}

/// A row heads the kind and reason its places all carry, so a place under it
/// is a position and nothing else. A row whose places disagree heads neither,
/// and every place there spells its own, so no heading ever speaks for a
/// place that differs from it.
#[test]
fn a_row_heads_the_tokens_its_places_share_and_leaves_the_ones_they_do_not() {
    let fx = amiss_fixtures::commit_pair(
        &[
            ("uniform.md", "# Uniform\n"),
            ("mixed.md", "# Mixed\n\n## Alpha Step\n"),
            ("guide.md", "# Guide\n"),
        ],
        &[(
            "guide.md",
            "# Guide\n\n[a](uniform.md#gone-one)\n[b](uniform.md#gone-two)\n[c](mixed.md#Alpha_Step)\n[d](mixed.md#nothing-alike)\n",
        )],
    )
    .unwrap();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains(
            "Fix target \"uniform.md\" affected places 2 explicit-target-missing heading-anchor-not-found\n  \"guide.md\":3:1\n  \"guide.md\":4:1\n"
        ),
        "two places carrying one kind and reason read them once, off the heading: {text}"
    );
    assert!(
        text.contains(
            "Fix target \"mixed.md\" affected places 2\n  \"guide.md\":5:1 explicit-target-missing heading-anchor-not-found near \"alpha-step\"\n  \"guide.md\":6:1 explicit-target-missing heading-anchor-not-found\n"
        ),
        "one nearby spelling and no nearby spelling disagree, so both places keep their own: {text}"
    );
}

/// Commits one document beside the fixture base, runs enforce over the pair,
/// and returns the exit code, the human text, and the sorted candidate-side
/// constructs matching the prefix from the JSON report.
fn enforced_document(name: &str, body: &str, prefix: &str) -> (i32, String, Vec<String>) {
    let fx = fixture();
    let root = fx.root();
    fs::write(root.join(name), body).unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "case"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let base_args = [
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
        "--candidate",
        &candidate,
        "--profile",
        "enforce",
    ];
    let (code, stdout, _stderr) = amiss(&base_args);
    let text = String::from_utf8_lossy(&stdout).into_owned();
    let mut json_args = base_args.to_vec();
    json_args.extend(["--format", "json"]);
    let (_code, stdout, _stderr) = amiss(&json_args);
    let report = report(&stdout);
    let mut constructs: Vec<String> = report
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .map(|side| side.observation_id_input.source_construct.as_ref())
        .filter(|construct| construct.starts_with(prefix))
        .map(str::to_owned)
        .collect();
    constructs.sort_unstable();
    (code, text, constructs)
}

/// An orphaned definition and a raw-HTML destination each maintain a target
/// the way a markdown link does: the dead one blocks under enforce and names
/// its target, the live neighbour in the same document is no finding, and
/// each extracts under its own construct in the report.
#[test]
fn orphan_definitions_and_html_destinations_gate_like_links() {
    let cases = [
        (
            "docs/orphans.md",
            "# Notes\n\nSee [kept][live].\n\n[live]: guide.md\n\n[api]: gone.md\n",
            "markdown-link-reference-definition",
            "docs/gone.md",
            vec!["markdown-link-reference-definition"],
        ),
        (
            "docs/media.md",
            "# Media\n\n<a href=\"guide.md\">ok</a>\n\n<img src=\"logo.png\">\n",
            "html-",
            "docs/logo.png",
            vec!["html-anchor", "html-image"],
        ),
    ];
    for (name, body, prefix, dead_target, expected) in cases {
        let (code, text, constructs) = enforced_document(name, body, prefix);
        assert_eq!(code, 1, "{name}: the dead destination blocks under enforce");
        assert!(
            text.contains(&format!("Fix target \"{dead_target}\" affected places 1")),
            "{name} names its dead target: {text}"
        );
        assert!(
            !text.contains("Fix target \"docs/guide.md\""),
            "{name}: the live destination is no finding: {text}"
        );
        assert_eq!(constructs, expected, "{name} constructs");
    }
}

/// A repository path is untrusted bytes, and the human projection is a place those
/// bytes could become terminal control sequences, a forged workflow command, or a
/// second log line. Feedback prints a grouped target instead of every source path,
/// and every repository-derived value it does print still passes through the
/// `human-atom` law. This drives a genuinely hostile source path through the binary
/// and proves it cannot leak control bytes into the focused projection.
#[test]
fn a_hostile_document_path_is_rendered_inert_and_round_trips_in_json() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    // ESC, an ANSI colour run, a forged GitHub Actions command, a bell, and a
    // carriage return, all valid UTF-8 and all valid in a RepoPath.
    let hostile = "docs/\u{1b}[31m::error::forged\u{7}\u{d}.md";
    let name = hostile.as_bytes().strip_prefix(b"docs/").unwrap();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# X\n\n[b](nowhere.md)\n").unwrap();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let docs = amiss_fixtures::tree_object(root, &[("100644", name, blob.as_str())]).unwrap();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("40000", b"docs".as_slice(), docs.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "hostile").unwrap();

    let repo = amiss_fixtures::path_arg(root);
    let (code, human, _stderr) = amiss(&[
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
        "human",
    ]);
    assert_eq!(code, 0, "a hostile path is still an ordinary document");
    for raw in [0x1b_u8, 0x0d, 0x07] {
        assert!(
            !human.contains(&raw),
            "raw control byte {raw:#04x} reached the human output"
        );
    }
    let human_text = String::from_utf8(human).expect("human output is utf-8");
    assert!(
        human_text.contains("Fix target \"docs/nowhere.md\" affected places 1"),
        "the feedback names the normalized target, not the hostile source: {human_text}"
    );

    let (code, json, _stderr) = amiss(&[
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
    let payload = payload(&json);
    let paths: Vec<&str> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["path"].as_str())
        .collect();
    assert!(
        paths.contains(&hostile),
        "json carries the exact bytes as a string, losing nothing: {paths:?}"
    );
}

/// The README and the quickstart quote the verdict line a clean run prints, so
/// a change to the human header has to move them too.
#[test]
fn the_documented_verdict_line_is_the_one_a_clean_run_prints() {
    let directory = TempDir::new().expect("temporary directory");
    let repository = directory.path().to_str().expect("utf-8 path");
    git(directory.path(), &["init", "-q"]);
    fs::write(directory.path().join("README.md"), "# Demo\n").expect("write");
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "one"]);
    let head = git(directory.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        repository,
        "--object-format",
        "sha1",
        "--base",
        &head,
        "--index",
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let printed = String::from_utf8(stdout).expect("utf-8 output");
    let verdict = printed.lines().next().expect("a verdict line");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for page in ["README.md", "docs/src/quickstart.md"] {
        let document = fs::read_to_string(root.join(page)).expect("documentation is readable");
        assert!(
            document.contains(verdict),
            "{page} quotes the verdict line a clean run prints, which is now {verdict}"
        );
    }
}
