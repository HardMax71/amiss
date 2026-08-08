#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::support::repository_root;

fn summary_chapters(root: &Path) -> Vec<String> {
    let summary =
        fs::read_to_string(root.join("docs/src/SUMMARY.md")).expect("the book summary is readable");
    let mut chapters = Vec::new();
    for line in summary.lines() {
        let Some((_, after)) = line.split_once("](") else {
            continue;
        };
        let (target, _) = after.split_once(')').expect("a summary link closes");
        let chapter = target
            .strip_suffix(".md")
            .expect("a summary link names a chapter source");
        chapters.push(chapter.to_owned());
    }
    chapters
}

#[test]
fn the_llms_index_names_real_chapters_on_the_published_book() {
    let root = repository_root();
    let path = root.join("docs/src/llms.txt");
    let document = fs::read_to_string(&path).expect("the llms index is readable");
    let mut indexed = Vec::new();
    for line in document.lines() {
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let (title, after) = rest
            .split_once("](")
            .expect("an index row is a markdown link");
        let (url, tail) = after.split_once(')').expect("an index link closes");
        assert!(tail.starts_with(": "), "each row explains its page: {line}");
        let chapter = url
            .strip_prefix("https://hardmax71.github.io/amiss/")
            .and_then(|page| page.strip_suffix(".html"))
            .expect("an index link names a chapter on the published book");
        let source = fs::read_to_string(root.join(format!("docs/src/{chapter}.md")))
            .expect("an index link names a chapter that exists");
        let heading = source
            .lines()
            .find_map(|chapter_line| chapter_line.strip_prefix("# "))
            .expect("a chapter opens with a level-one heading");
        assert_eq!(
            title,
            heading,
            "{} titles {chapter} differently from its own heading",
            path.display(),
        );
        indexed.push(chapter.to_owned());
    }
    assert_eq!(
        indexed,
        summary_chapters(&root),
        "the index lists every chapter of the book in reading order",
    );
}

#[test]
fn published_ci_examples_expose_every_moving_release_choice() {
    let root = repository_root();
    let sources = [
        (root.join("README.md"), 1_usize),
        (root.join("docs/src/ci.md"), 3_usize),
    ];
    let workspace_major = env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .expect("a Cargo package version has a major component");
    let expected_action = format!("v{workspace_major}");

    for (path, expected_upstream_references) in &sources {
        let document = fs::read_to_string(path).expect("published CI example is readable");
        let mut amiss_references = 0_usize;
        let mut upstream_references = 0_usize;
        for (line_index, line) in document.lines().enumerate() {
            let trimmed = line.trim();
            let Some(specification) = trimmed.strip_prefix("- uses: ") else {
                continue;
            };
            if specification.starts_with("./") {
                continue;
            }
            let Some((action, reference)) = specification
                .split_whitespace()
                .next()
                .and_then(|token| token.split_once('@'))
            else {
                panic!(
                    "{}:{} has an external Action without a reference",
                    path.display(),
                    line_index + 1,
                );
            };

            if action == "HardMax71/amiss" {
                assert_eq!(
                    reference,
                    expected_action,
                    "{}:{} advertises the wrong moving Amiss release major",
                    path.display(),
                    line_index + 1,
                );
                amiss_references = amiss_references.saturating_add(1);
            } else {
                assert!(
                    reference.len() == 40
                        && reference
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
                    "{}:{} must pin the upstream action to a full reviewed commit, found {reference}",
                    path.display(),
                    line_index + 1,
                );
                upstream_references = upstream_references.saturating_add(1);
            }
        }

        assert_eq!(
            amiss_references,
            1,
            "{} must advertise the supported Amiss Action exactly once",
            path.display(),
        );
        assert_eq!(
            upstream_references,
            *expected_upstream_references,
            "{} must keep every published upstream Action dependency explicit",
            path.display(),
        );
        assert_eq!(
            document
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    line == "profile: observe" || line.contains("--profile observe")
                })
                .count(),
            2,
            "{} must make every first-run CI form observe-first",
            path.display(),
        );
    }

    let ci = fs::read_to_string(root.join("docs/src/ci.md")).expect("CI documentation is readable");
    let installs: Vec<&str> = ci
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("cargo install") && line.ends_with(" amiss"))
        .collect();
    assert_eq!(
        installs,
        [
            "- run: cargo install --locked --registry crates-io --version '=<reviewed-version>' amiss"
        ],
        "the direct CI form must demand an exact reviewed version without copying the current patch release"
    );
}

#[test]
fn release_smokes_every_runtime_before_promoting_the_major_ref() {
    let workflow = fs::read_to_string(repository_root().join(".github/workflows/release.yml"))
        .expect("release workflow is readable");
    let (_, after_publish_heading) = workflow
        .split_once("\n  publish-action:")
        .expect("release workflow publishes the exact Action ref");
    let (publish_action, after_smoke_heading) = after_publish_heading
        .split_once("\n  smoke-action:")
        .expect("release workflow has an Action smoke gate");
    let (smoke_action, after_assets_heading) = after_smoke_heading
        .split_once("\n  publish-assets:")
        .expect("release workflow attaches assets to the draft");
    let (publish_assets, publish_release) = after_assets_heading
        .split_once("\n  publish-release:")
        .expect("release workflow publishes only after smoke tests");

    assert!(publish_action.contains("\"$commit:$exact_ref\""));
    assert!(publish_action.contains("group: action-publication-${{ github.ref_name }}"));
    assert!(
        !publish_action.contains("\"$commit:$major_ref\""),
        "exact Action publication must not move the major ref"
    );
    assert!(
        smoke_action.contains("os: [ubuntu-latest, macos-latest, macos-15-intel, windows-latest]")
    );
    assert!(smoke_action.contains("ref: action/${{ github.ref_name }}"));
    assert!(smoke_action.contains("uses: ./action-under-test"));
    assert!(smoke_action.contains("uses: ./\n"));
    assert!(publish_assets.contains("needs: [publish-action, smoke-action]"));
    assert!(publish_assets.contains("sha256sum -- amiss-* > SHA256SUMS"));
    assert!(publish_assets.contains("subject-checksums: assets/SHA256SUMS"));
    assert!(publish_assets.contains("gh release upload \"$TAG\" --clobber assets/*"));
    assert!(
        !publish_assets.contains("bootstrap-"),
        "the constraint tooling is built from the reviewed source commit, not downloaded"
    );
    assert!(publish_release.contains("needs: [publish-action, smoke-action, publish-assets]"));
    assert!(publish_release.contains("group: action-major-promotion"));
    assert!(publish_release.contains(
        "current=\"$(git ls-remote --heads \"$remote\" \"$major_ref\" | awk '{print $1}')\""
    ));
    assert!(publish_release.contains("if \"${push[@]}\" \"$commit:$major_ref\"; then"));
    assert!(publish_release.contains("git commit-tree \"$exact_tree\" -p \"$current\""));
    assert!(publish_release.contains("for attempt in 1 2 3 4 5; do"));
    assert!(publish_release.contains("steps.promote.outputs.major-is-latest"));
    assert!(publish_release.contains("release(tagName: $tag)"));
    assert!(publish_release.contains("${TAG} has no GitHub release"));
    assert!(publish_release.contains(".data.repository.release.databaseId"));
    assert!(publish_release.contains(".data.repository.release.isDraft"));
    assert!(publish_release.contains(".data.repository.release.isPrerelease"));
    assert!(
        !publish_release.contains("releases/tags/${TAG}"),
        "draft releases are not visible through the REST lookup by tag"
    );
}

#[test]
fn third_party_material_keeps_its_attribution() {
    let root = repository_root();
    let notices = fs::read_to_string(root.join("THIRD_PARTY_NOTICES.md"))
        .expect("third-party notices are readable");
    for source in [
        "commonmark-0.31.2.spec.json",
        "gfm-0.29.spec.txt",
        "0.29.0.gfm.13",
        "ad0a49c",
        "2891b75",
        "7cc9131",
        "df527f5",
        "a3a75cc",
        "2de5cc58d87b3a58413020f9f15bd8c261c29e13",
        "mdBook 0.5.4",
        "Highlight.js 10.1.1",
        "Font Awesome Free 6.2.0",
    ] {
        assert!(
            notices.contains(source),
            "third-party notices omit {source}"
        );
    }

    for (file, expected) in [
        (
            "LM-bold-italic.woff2",
            "3d41e67617603684e0353953f9460893cd441049398be31857c9fbaaa2521811",
        ),
        (
            "LM-bold.woff2",
            "449ad146efbd630d36e08f956b1249e862463797a26b61f5fe7999513c328c03",
        ),
        (
            "LM-italic.woff2",
            "3eb5daf8d26e6f882207633b8f45a27b389ac1b2a6713562fdef4d982f24b192",
        ),
        (
            "LM-regular.woff2",
            "c2e0d602fee55a45e44f8ab3f4f561d73d2c23db1efee295865d79f9307977db",
        ),
    ] {
        let bytes = fs::read(root.join("docs/src/fonts").join(file)).expect("font is readable");
        let mut actual = String::with_capacity(64);
        for byte in Sha256::digest(bytes) {
            write!(&mut actual, "{byte:02x}").expect("writing to a string is infallible");
        }
        assert_eq!(actual, expected);
    }

    let introduction = fs::read_to_string(root.join("docs/src/introduction.md"))
        .expect("book introduction is readable");
    assert!(introduction.contains("blob/main/LICENSE.md"));
    assert!(introduction.contains("blob/main/THIRD_PARTY_NOTICES.md"));
    assert!(introduction.contains("fonts/GUST-FONT-LICENSE.txt"));
}

#[test]
fn repository_relative_documentation_links_resolve() {
    let documentation_directory = repository_root().join("docs/src");
    let mut checked = 0_u64;

    for entry in
        fs::read_dir(&documentation_directory).expect("documentation directory is readable")
    {
        let path = entry.expect("documentation entry is readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        let document = fs::read_to_string(&path).expect("documentation source is readable");
        let mut fenced = false;
        for (line_index, line) in document.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }

            let mut remainder = line;
            while let Some(open) = remainder.find("](") {
                let after_open = remainder
                    .get(open + 2..)
                    .expect("the ASCII link opener ends at a UTF-8 boundary");
                let Some(close) = after_open.find(')') else {
                    break;
                };
                let destination = after_open
                    .get(..close)
                    .expect("the ASCII link closer starts at a UTF-8 boundary");
                let tree_target = if destination.starts_with("../../") {
                    Some(
                        path.parent()
                            .expect("documentation source has a parent")
                            .join(destination),
                    )
                } else {
                    destination
                        .strip_prefix("https://github.com/HardMax71/amiss/blob/main/")
                        .map(|target| repository_root().join(target))
                };
                if let Some(resolved) = tree_target {
                    let resolved = resolved
                        .to_str()
                        .and_then(|text| text.split(['#', '?']).next())
                        .map(PathBuf::from)
                        .expect("documentation link paths are UTF-8");
                    assert!(
                        resolved.exists(),
                        "{}:{} links to missing repository path {destination}",
                        path.display(),
                        line_index + 1,
                    );
                    checked = checked.saturating_add(1);
                }
                remainder = after_open
                    .get(close + 1..)
                    .expect("the ASCII link closer ends at a UTF-8 boundary");
            }
        }
    }

    assert!(
        checked > 0,
        "documentation contains no repository-relative implementation links"
    );
}
