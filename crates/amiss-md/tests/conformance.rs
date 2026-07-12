use std::fs;
use std::path::Path;

use amiss_md::corpus::{self, Case};
use amiss_md::profile::parse_options;
use amiss_wire::model::Adapter;
use markdown::{CompileOptions, Options, ParseOptions, to_html_with_options};
use sha2::{Digest as _, Sha256};

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn input(name: &str, pin: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/inputs")
        .join(name);
    let bytes = fs::read(path).unwrap();
    let mut hex = String::from("sha256:");
    for byte in Sha256::digest(&bytes) {
        hex.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
        hex.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap());
    }
    assert_eq!(hex, pin, "{name} drifted from its pinned digest");
    bytes
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn cases() -> Vec<Case> {
    let commonmark = corpus::commonmark(&input(
        "commonmark-0.31.2.spec.json",
        corpus::COMMONMARK_PIN,
    ))
    .unwrap();
    let gfm_text = String::from_utf8(input("gfm-0.29.spec.txt", corpus::GFM_PIN)).unwrap();
    let mut all = commonmark;
    all.extend(corpus::gfm(&gfm_text));
    all
}

fn render(source: &str, parse: ParseOptions, tagfilter: bool) -> String {
    to_html_with_options(
        source,
        &Options {
            parse,
            compile: CompileOptions {
                allow_dangerous_html: true,
                allow_dangerous_protocol: true,
                gfm_tagfilter: tagfilter,
                ..CompileOptions::default()
            },
        },
    )
    .unwrap_or_default()
}

/// The core half of the grammar pin: with the extensions off, the parser
/// reproduces every executable `CommonMark` 0.31.2 example.
#[test]
fn reproduces_commonmark_0_31_2() {
    let cases: Vec<Case> = cases()
        .into_iter()
        .filter(|case| case.family == corpus::COMMONMARK_FAMILY)
        .collect();
    assert_eq!(
        cases.len(),
        652,
        "the pinned CommonMark corpus is 652 examples"
    );

    let mut broken = Vec::new();
    for case in &cases {
        if render(&case.source, ParseOptions::default(), false) != case.html {
            broken.push(case.case_id());
        }
    }
    assert!(
        broken.is_empty(),
        "{} of {} CommonMark examples differ: {broken:?}",
        broken.len(),
        cases.len()
    );
}

/// GFM 0.29 is `cmark-gfm`'s text, while the pinned parse additions are the
/// `remark-gfm` bundle, so one example is expected to differ. Example 628
/// autolinks `ftp://`; the pinned bundle recognizes only `www.`, `http://`,
/// `https://`, and email, which is what github.com itself does. The set is
/// asserted exactly, so a second divergence fails the run.
const KNOWN_DIVERGENCE: [&str; 1] = ["gfm-0.29/628"];

/// The extension half: every example GFM 0.29 marks with an extension and
/// executes, under the pinned `commonmark-gfm-v1` options. That document's
/// untagged examples are `CommonMark` 0.29, which 0.31.2 supersedes, so they are
/// corpus inputs rather than goldens here.
#[test]
fn reproduces_gfm_0_29_extensions() {
    let Some(pinned) = parse_options(Adapter::Markdown) else {
        panic!("the markdown adapter must pin parse options")
    };
    let cases: Vec<Case> = cases()
        .into_iter()
        .filter(|case| case.family == corpus::GFM_FAMILY)
        .collect();
    assert_eq!(cases.len(), 672, "the pinned GFM corpus is 672 examples");

    let mut checked = 0_usize;
    let mut skipped = 0_usize;
    let mut broken = Vec::new();
    for case in &cases {
        let Some(tag) = case.tag.as_deref() else {
            continue;
        };
        if !case.executable() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        checked = checked.saturating_add(1);
        let options = ParseOptions {
            constructs: pinned.constructs.clone(),
            gfm_strikethrough_single_tilde: pinned.gfm_strikethrough_single_tilde,
            ..ParseOptions::default()
        };
        if render(&case.source, options, tag == "tagfilter") != case.html {
            broken.push(case.case_id());
        }
    }
    assert_eq!(checked, 22, "GFM 0.29 executes 22 extension examples");
    assert_eq!(skipped, 2, "GFM 0.29 disables its 2 task-list examples");
    assert_eq!(
        broken, KNOWN_DIVERGENCE,
        "GFM extension examples diverge from the pinned bundle beyond the recorded case"
    );
}
