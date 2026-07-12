use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::corpus::{self, Case, Fixtures};
use sha2::{Digest as _, Sha256};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn input(name: &str, pin: &str) -> Vec<u8> {
    let bytes = fs::read(root().join("corpus/inputs").join(name)).unwrap();
    let mut hex = String::from("sha256:");
    for byte in Sha256::digest(&bytes) {
        hex.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
        hex.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap());
    }
    assert_eq!(hex, pin, "{name} drifted from its pinned digest");
    bytes
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn text(name: &str, pin: &str) -> String {
    String::from_utf8(input(name, pin)).unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn harvest() -> (Vec<Case>, Vec<(&'static str, usize)>) {
    let mut cases = corpus::commonmark(&input(
        "commonmark-0.31.2.spec.json",
        corpus::COMMONMARK_PIN,
    ))
    .unwrap();
    cases.extend(corpus::gfm(&text("gfm-0.29.spec.txt", corpus::GFM_PIN)));

    let suites = [
        (
            corpus::MDX_JSX_FAMILY,
            "micromark-mdx-jsx-3.0.2.test.js",
            corpus::MDX_JSX_PIN,
        ),
        (
            corpus::MDX_EXPRESSION_FAMILY,
            "micromark-mdx-expression-3.0.1.test.js",
            corpus::MDX_EXPRESSION_PIN,
        ),
        (
            corpus::MDX_ESM_FAMILY,
            "micromark-mdxjs-esm-3.0.0.test.js",
            corpus::MDX_ESM_PIN,
        ),
    ];
    let mut skipped = Vec::new();
    for (family, file, pin) in suites {
        let Fixtures {
            cases: harvested,
            skipped: dropped,
        } = corpus::micromark_fixtures(family, &text(file, pin));
        cases.extend(harvested);
        skipped.push((family, dropped));
    }

    let gfm_suites = [
        (
            corpus::FOOTNOTE_FAMILY,
            "micromark-gfm-footnote-2.1.0.test.js",
            corpus::FOOTNOTE_PIN,
        ),
        (
            corpus::STRIKETHROUGH_FAMILY,
            "micromark-gfm-strikethrough-2.1.0.test.js",
            corpus::STRIKETHROUGH_PIN,
        ),
    ];
    for (family, file, pin) in gfm_suites {
        let Fixtures {
            cases: harvested,
            skipped: dropped,
        } = corpus::micromark_fixtures(family, &text(file, pin));
        cases.extend(harvested);
        skipped.push((family, dropped));
    }

    cases.extend(corpus::github_fixtures(&github_pairs()));
    skipped.push((corpus::GITHUB_FOOTNOTE_FAMILY, 0));
    (cases, skipped)
}

/// The footnote fixture directory: one document and the HTML github.com renders
/// for it, per name, in sorted order.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn github_pairs() -> Vec<(String, String, String)> {
    let dir = root().join("corpus/inputs/gfm-footnote-fixtures");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            (name, fs::read_to_string(&path).unwrap())
        })
        .collect();
    files.sort();
    assert_eq!(
        corpus::directory_digest(&files),
        corpus::GITHUB_FOOTNOTE_PIN,
        "the footnote fixture directory drifted from its pin"
    );

    let mut pairs: Vec<(String, String, String)> = files
        .iter()
        .filter_map(|(name, body)| {
            let stem = name.strip_suffix(".md")?;
            let html = files
                .iter()
                .find(|(other, _)| other == &format!("{stem}.html"))
                .map(|(_, html)| html.clone())?;
            Some((stem.to_owned(), body.clone(), html))
        })
        .collect();
    pairs.sort();
    pairs
}
