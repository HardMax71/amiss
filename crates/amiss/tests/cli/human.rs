use std::fs;

use tempfile::TempDir;

use crate::support::{amiss, fixture, git, payload};

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
        text.starts_with("amiss: pass (fix 1, check 1, existing 0, errors 0, exit 0)"),
        "got: {text}"
    );
    assert!(
        text.contains("Fix target \"docs/missing.md\" affected places 1"),
        "the grouped item names its target and affected-place count: {text}"
    );
    assert!(
        text.contains("Check target \"docs/guide.md\" affected places 1"),
        "the unchanged backlink becomes one check: {text}"
    );
    assert!(
        !text.contains("explicit-target-missing"),
        "internal finding kinds stay out of the focused human projection: {text}"
    );
    assert!(
        text.contains("references: extracted "),
        "totals close the projection"
    );
    assert!(!text.contains('\r'), "LF-only stdout");
    assert!(
        !text.contains("feedback overflow"),
        "two items are not an overflow: {text}"
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
        text.contains("references: external counts any same-repository URL"),
        "an undeclared identity is named beside the count: {text}"
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
        !text.contains("since no forge identity was declared"),
        "a declared identity silences the line: {text}"
    );
}

#[test]
fn pre_existing_findings_render_as_existing_rows_without_fix_or_notes() {
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
        text.starts_with("amiss: pass (fix 0, check 0, existing 1, errors 0, exit 0)"),
        "got: {text}"
    );
    assert!(
        text.contains("Existing target \"docs/missing.md\" affected places 1"),
        "the backlog renders under its own label: {text}"
    );
    assert!(!text.lines().any(|line| line.starts_with("Fix ")), "{text}");
    assert!(
        !text.contains("note explicit-target-missing:"),
        "finding kinds stay out of the note lines: {text}"
    );
    assert!(
        text.contains("findings: total "),
        "raw totals still expose the inventory: {text}"
    );
}

#[test]
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
        text.starts_with("amiss: pass (fix 201, check 0, existing 1, errors 0, exit 0)"),
        "the header counts the complete grouped projection: {text}"
    );
    assert!(
        text.contains("feedback overflow: 191 more in the full report"),
        "the fix window overflows without counting the backlog: {text}"
    );
    assert!(
        text.contains("Existing target \"docs/missing.md\" affected places 1"),
        "the backlog window survives two hundred introduced items: {text}"
    );
    assert!(
        !text.contains("existing overflow"),
        "one backlog item is not an overflow: {text}"
    );
    assert_eq!(
        text.matches("explicit-target-missing").count(),
        0,
        "machine finding kinds stay out of the focused human projection"
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
            text.contains("Existing target \"docs/setup.md\" affected places 1"),
            "the backlog names its target under {profile}: {text}"
        );
        assert!(
            text.contains("existing 1,"),
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
        text.contains("existing 11,"),
        "the header counts all: {text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("Existing "))
            .count(),
        10,
        "ten backlog rows and no more: {text}"
    );
    assert!(
        text.contains("existing overflow: 1 more in the full report"),
        "{text}"
    );
}

/// Commits one document beside the fixture base, runs enforce over the pair,
/// and returns the exit code, the human text, and the sorted candidate-side
/// constructs matching the prefix from the JSON report.
#[expect(clippy::indexing_slicing, reason = "test assertion helper")]
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
    let payload = payload(&stdout);
    let mut constructs: Vec<String> = payload["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["candidate"]["source_construct"].as_str())
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
