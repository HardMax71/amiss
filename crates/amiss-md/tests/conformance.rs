mod fixtures;

use amiss_md::corpus::{self, Case, Expect};
use amiss_md::profile::parse_options;
use amiss_wire::model::Adapter;
use markdown::{CompileOptions, Options, ParseOptions, to_html_with_options};

use fixtures::{github_pairs, harvest};

/// GFM 0.29 is `cmark-gfm`'s text, while the pinned parse additions are the
/// `remark-gfm` bundle, so one example is expected to differ. Example 628
/// autolinks `ftp://`; the pinned bundle recognizes only `www.`, `http://`,
/// `https://`, and email, which is what github.com itself does.
const GFM_DIVERGENCE: [&str; 1] = ["gfm-0.29/628"];

/// The MDX suites test one extension at a time, so a few of their expectations
/// belong to a construct set that is not `mdx-source`, and their throwaway
/// HTML extensions do not drop line endings the way a real MDX compiler does.
/// Neither kind of difference is a grammar difference, and asserting the set
/// exactly means a real one cannot hide among them.
///
/// `micromark-mdx-expression-3.0.1/50` indents `{}` by four spaces and expects
/// an indented code block. MDX removes indented code, and this profile removes
/// it too, so the expression is an expression.
///
/// The other five differ only in surviving line endings, with identical
/// content: the suites' extensions buffer and drop a tag, while a compiler that
/// understands MDX also slurps the line ending the tag left behind.
const MDX_HTML_DIVERGENCE: [&str; 6] = [
    "micromark-mdx-expression-3.0.1/50",
    "micromark-mdx-jsx-3.0.2/107",
    "micromark-mdx-jsx-3.0.2/128",
    "micromark-mdx-jsx-3.0.2/129",
    "micromark-mdx-jsx-3.0.2/140",
    "micromark-mdx-jsx-3.0.2/141",
];

/// Every reason the pinned bundle gives for a rejection this profile does not
/// make. Each one needs a JavaScript parser or its syntax tree, and this
/// profile reads only the lexical grammar of embedded code, never its syntax.
/// None of them moves an opaque interval, so extraction is unaffected: the
/// scanner reads a document that MDX itself would refuse to compile, and its
/// code regions stay opaque either way.
const JAVASCRIPT_REASONS: [&str; 7] = [
    "with acorn",
    "acorn` instance",
    "empty expression",
    "only spread elements are supported",
    "expected an object spread",
    "only a single spread is supported",
    "only import/exports are supported",
];

fn render(source: &str, parse: ParseOptions, tagfilter: bool) -> Option<String> {
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
    .ok()
}

fn family(name: &str) -> Vec<Case> {
    let (cases, _skipped) = harvest();
    cases
        .into_iter()
        .filter(|case| case.family == name)
        .collect()
}

/// The core half of the grammar pin: with the extensions off, the parser
/// reproduces every executable `CommonMark` 0.31.2 example.
#[test]
fn reproduces_commonmark_0_31_2() {
    let cases = family(corpus::COMMONMARK_FAMILY);
    assert_eq!(
        cases.len(),
        652,
        "the pinned CommonMark corpus is 652 examples"
    );

    let mut broken = Vec::new();
    for case in &cases {
        let Expect::Html(want) = &case.expect else {
            panic!("every CommonMark example publishes HTML")
        };
        if render(&case.source, ParseOptions::default(), false).as_ref() != Some(want) {
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

/// The extension half: every example GFM 0.29 marks with an extension and
/// executes, under the pinned `commonmark-gfm` options. That document's
/// untagged examples are `CommonMark` 0.29, which 0.31.2 supersedes, so they
/// are corpus inputs rather than goldens here.
#[test]
fn reproduces_gfm_0_29_extensions() {
    let mut checked = 0_usize;
    let mut skipped = 0_usize;
    let mut broken = Vec::new();
    for case in &family(corpus::GFM_FAMILY) {
        let (Some(tag), Expect::Html(want)) = (case.tag.as_deref(), &case.expect) else {
            continue;
        };
        if !case.executable() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        checked = checked.saturating_add(1);
        let Some((options, _meter)) = parse_options(Adapter::Markdown, u64::MAX) else {
            panic!("the markdown adapter must pin parse options")
        };
        if render(&case.source, options, tag == "tagfilter").as_ref() != Some(want) {
            broken.push(case.case_id());
        }
    }
    assert_eq!(checked, 22, "GFM 0.29 executes 22 extension examples");
    assert_eq!(skipped, 2, "GFM 0.29 disables its 2 task-list examples");
    assert_eq!(
        broken, GFM_DIVERGENCE,
        "GFM extension examples diverge from the pinned bundle beyond the recorded case"
    );
}

/// The MDX half. The suites are the grammar's own fixtures, so this profile
/// must accept everything they accept, produce their HTML, and reject what they
/// reject except where rejecting needs a JavaScript syntax tree.
#[test]
fn reproduces_mdx_syntax_and_errors() {
    let (all, skipped) = harvest();
    let cases: Vec<Case> = all
        .into_iter()
        .filter(|case| {
            matches!(
                case.family,
                corpus::MDX_JSX_FAMILY | corpus::MDX_EXPRESSION_FAMILY | corpus::MDX_ESM_FAMILY
            )
        })
        .collect();
    assert_eq!(cases.len(), 257, "the pinned MDX suites hold 257 fixtures");
    let dropped: usize = skipped
        .iter()
        .filter(|(family, _)| {
            matches!(
                *family,
                corpus::MDX_JSX_FAMILY | corpus::MDX_EXPRESSION_FAMILY | corpus::MDX_ESM_FAMILY
            )
        })
        .map(|(_, count)| *count)
        .sum();
    assert_eq!(
        dropped, 8,
        "the only dropped MDX fixtures pass a variable rather than a literal source"
    );

    let mut over_rejected = Vec::new();
    let mut html_differs = Vec::new();
    let mut accepted_anyway = Vec::new();
    let mut agreed = 0_usize;

    for case in &cases {
        let Some((options, _meter)) = parse_options(Adapter::Mdx, u64::MAX) else {
            panic!("the mdx adapter must pin parse options")
        };
        let ours = render(&case.source, options, false);
        match (&case.expect, ours) {
            (Expect::Rejected(reason), Some(_accepted)) => {
                assert!(
                    JAVASCRIPT_REASONS
                        .iter()
                        .any(|known| reason.contains(known)),
                    "{} is accepted here and rejected upstream for a reason that is not about \
                     JavaScript syntax: {reason}",
                    case.case_id()
                );
                accepted_anyway.push(case.case_id());
            }
            (Expect::Rejected(_reason), None) => agreed = agreed.saturating_add(1),
            (Expect::Accepted | Expect::Html(_), None) => over_rejected.push(case.case_id()),
            (Expect::Accepted, Some(_accepted)) => agreed = agreed.saturating_add(1),
            (Expect::Html(want), Some(got)) => {
                if &got == want {
                    agreed = agreed.saturating_add(1);
                } else {
                    html_differs.push(case.case_id());
                }
            }
        }
    }

    assert!(
        over_rejected.is_empty(),
        "this profile rejects what the pinned grammar accepts: {over_rejected:?}"
    );
    html_differs.sort();
    assert_eq!(
        html_differs, MDX_HTML_DIVERGENCE,
        "MDX output differs beyond the recorded cases"
    );
    assert_eq!(
        accepted_anyway.len(),
        26,
        "the JavaScript-syntax rejections this profile does not make"
    );
    assert_eq!(agreed, 225, "fixtures that agree exactly");
}

/// A back-reference's `aria-label` is a compile option with no parse meaning.
/// micromark 2.1.0 writes one per reference; `markdown-rs` has a single static
/// string and cannot express that. The scanner renders no HTML, so the label is
/// erased on both sides rather than compared.
fn erase_labels(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(at) = rest.find("aria-label=\"") {
        let (before, after) = rest.split_at(at);
        out.push_str(before);
        out.push_str("aria-label=\"\"");
        let inside = after.get("aria-label=\"".len()..).unwrap_or_default();
        let end = inside
            .find('"')
            .map_or(inside.len(), |at| at.saturating_add(1));
        rest = inside.get(end..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// github.com drops the source of an image that points at nothing but a search
/// or a hash, so the suite drops it on the rendered side too.
fn erase_search_sources(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut at = 0_usize;
    while let Some(&byte) = bytes.get(at) {
        let opens = bytes
            .get(at..)
            .is_some_and(|rest| rest.starts_with(b"src=\""));
        let empty = matches!(bytes.get(at.saturating_add(5)), Some(&(b'?' | b'#')));
        let quote = bytes
            .get(at.saturating_add(5)..)
            .and_then(|rest| rest.iter().position(|byte| *byte == b'"'));
        if let Some(end) = quote.filter(|_at| opens && empty) {
            out.extend_from_slice(b"src=\"\"");
            at = at.saturating_add(6).saturating_add(end);
            continue;
        }
        out.push(byte);
        at = at.saturating_add(1);
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The compensations the suite itself applies for bugs in github.com's renderer,
/// so that what remains is a difference in this implementation rather than in
/// GitHub. Each is keyed to the document it belongs to, exactly as upstream
/// keys them.
fn compensate(name: &str, expected: &str) -> String {
    let mut html = expected.to_owned();
    if name == "calls" {
        html = html.replace("%5e", "%5E");
    }
    if name.starts_with("constructs-in-footnotes") {
        html = html.replacen(
            "<pre lang=\"js\"><code>",
            "<pre><code class=\"language-js\">",
            1,
        );
    }
    if name == "constructs-in-identifiers" {
        html = html.replacen(
            "<a id=\"user-content-fnref-https://example.com\"",
            "<a href=\"#user-content-fn-https://example.com\" id=\"user-content-fnref-https://example.com\"",
            1,
        );
        html = html.replacen(
            "<a id=\"user-content-fnref-://example.com\"",
            "<a href=\"#user-content-fn-://example.com\" id=\"user-content-fnref-://example.com\"",
            1,
        );
        html = html.replacen(
            "<li id=\"user-content-fn-https://example.com\">\n<p>a \u{21a9}</p>",
            "<li id=\"user-content-fn-https://example.com\">\n<p>a <a href=\"#user-content-fnref-https://example.com\" data-footnote-backref=\"\" aria-label=\"\" class=\"data-footnote-backref\">\u{21a9}</a></p>",
            1,
        );
        html = html.replacen(
            "<li id=\"user-content-fn-://example.com\">\n<p>a \u{21a9}</p>",
            "<li id=\"user-content-fn-://example.com\">\n<p>a <a href=\"#user-content-fnref-://example.com\" data-footnote-backref=\"\" aria-label=\"\" class=\"data-footnote-backref\">\u{21a9}</a></p>",
            1,
        );
        html = html.replace("![image](#)", "<img src=\"\" alt=\"image\" />");
    }
    if name == "footnotes-in-constructs" {
        html = html.replacen(
            "<a href=\"#\">link<sup></sup></a><a href=\"#user-content-fn-5\" id=\"user-content-fnref-5\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">4</a>",
            "<a href=\"#\">link<sup><a href=\"#user-content-fn-5\" id=\"user-content-fnref-5\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">4</a></sup></a>",
            1,
        );
    }
    html
}

/// A suite that configures the extension away from what this profile pins is
/// testing another profile, so its HTML is not a golden for this one. Those
/// documents stay in the corpus as inputs; only the comparison is skipped.
fn other_profile(config: &str) -> bool {
    [
        "singleTilde: false",
        "gfmFootnote({",
        "gfmFootnoteHtml({",
        "disable",
    ]
    .iter()
    .any(|marker| config.contains(marker))
}

fn rendered(source: &str) -> Option<String> {
    let (options, _meter) = parse_options(Adapter::Markdown, u64::MAX)?;
    let mut html = render(source, options, false)?;
    if !html.is_empty() && !html.ends_with('\n') {
        html.push('\n');
    }
    Some(html)
}

/// Footnotes and single-tilde strikethrough are the pinned bundle's additions
/// beyond formal GFM 0.29, so they carry their own suites.
#[test]
fn reproduces_the_footnote_and_tilde_suites() {
    let (all, _skipped) = harvest();
    let mut checked = 0_usize;
    let mut elsewhere = 0_usize;
    let mut broken = Vec::new();
    for case in &all {
        if !matches!(
            case.family,
            corpus::FOOTNOTE_FAMILY | corpus::STRIKETHROUGH_FAMILY
        ) {
            continue;
        }
        let Expect::Html(want) = &case.expect else {
            continue;
        };
        if other_profile(&case.config) {
            elsewhere = elsewhere.saturating_add(1);
            continue;
        }
        checked = checked.saturating_add(1);
        let want = erase_labels(want);
        let want = if want.is_empty() || want.ends_with('\n') {
            want
        } else {
            format!("{want}\n")
        };
        if rendered(&case.source).map(|html| erase_labels(&html)) != Some(want) {
            broken.push(case.case_id());
        }
    }
    assert_eq!(checked, 22, "fixtures under the pinned configuration");
    assert_eq!(elsewhere, 7, "fixtures that configure another profile");
    assert!(
        broken.is_empty(),
        "footnote or tilde fixtures differ: {broken:?}"
    );
}

/// The footnote suite also renders 29 documents against the HTML github.com
/// itself produces, which is where the interactions the spec names live: a
/// footnote call against a link, an image, a duplicate definition, a reference
/// definition, and nesting inside every container.
///
/// One of them differs, and it is this implementation that is wrong.
/// `markdown-rs` 1.0.0 does not form a link whose label holds a footnote call,
/// so `[link[^1]](#)` stays literal where the pinned grammar makes it a link.
/// The scanner would miss that reference. It is recorded here, and it is worth
/// reporting upstream.
const GITHUB_DIVERGENCE: [&str; 1] = ["footnotes-in-constructs"];

#[test]
fn reproduces_githubs_own_footnote_rendering() {
    let pairs = github_pairs();
    assert_eq!(pairs.len(), 29, "the pinned footnote fixture directory");

    let mut broken = Vec::new();
    for (name, source, html) in &pairs {
        let want = erase_labels(&compensate(name, html));
        let ours = rendered(source)
            .map(|rendered| erase_labels(&erase_search_sources(&rendered)))
            .unwrap_or_default();
        if ours != want {
            broken.push(name.clone());
        }
    }
    assert_eq!(
        broken, GITHUB_DIVERGENCE,
        "github footnote rendering differs beyond the recorded case"
    );
}
