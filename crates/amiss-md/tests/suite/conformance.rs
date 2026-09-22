use amiss_md::analyze;
use amiss_md::profile::mdx_options;
use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::Adapter;
use markdown::{CompileOptions, Options, ParseOptions, to_html_with_options};
use pulldown_cmark::{Options as CmarkOptions, Parser, html};

use crate::corpus_support::{self as corpus, Case, Expect};
use crate::fixtures::{github_pairs, harvest};

/// GFM 0.29 is `cmark-gfm`'s text; autolink literals here come from linkify
/// under the profile's protocol filter, so three examples differ. 625 keeps
/// `(business))+ok` whole because the run does not end in a parenthesis,
/// where linkify stops at the unbalanced one. 628 autolinks `ftp://`, which
/// github.com does not and the profile does not. 631 links `a.b-c_d@a.b`,
/// whose one-letter top-level domain linkify refuses.
const GFM_DIVERGENCE: [&str; 3] = ["gfm-0.29/625", "gfm-0.29/628", "gfm-0.29/631"];

/// The footnote and tilde suites publish no link the profile misses.
const FOOTNOTE_DIVERGENCE: [&str; 0] = [];

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

fn render_mdx(source: &str, parse: ParseOptions) -> Option<String> {
    to_html_with_options(
        source,
        &Options {
            parse,
            compile: CompileOptions {
                allow_dangerous_html: true,
                allow_dangerous_protocol: true,
                ..CompileOptions::default()
            },
        },
    )
    .ok()
}

/// The `CommonMark` half renders through the Markdown profile's own parser with
/// every extension off, which is the configuration the specification text is
/// written for.
fn render_commonmark(source: &str) -> String {
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(source, CmarkOptions::empty()));
    out
}

/// The two serialization choices the specification's own test runner does not
/// distinguish and pulldown's spec suite normalizes the same way: a double
/// quote in text may be written as itself or as `&quot;`, and a line break
/// between two tags is not content.
fn standardized(html: &str) -> String {
    html.replace("&quot;", "\"").replace(">\n<", "><")
}

fn family(name: &str) -> Vec<Case> {
    let (cases, _skipped) = harvest();
    cases
        .into_iter()
        .filter(|case| case.family == name)
        .collect()
}

/// The link surface one upstream rendering publishes: every anchor `href` and
/// image `src`, in order, as decoded bytes.
fn published(html: &str) -> Vec<Vec<u8>> {
    corpus::html_destinations(html)
        .iter()
        .map(|value| corpus::percent_decoded(value))
        .collect()
}

fn is_image(construct: SourceConstruct) -> bool {
    matches!(
        construct,
        SourceConstruct::InlineImage
            | SourceConstruct::FullReferenceImage
            | SourceConstruct::CollapsedReferenceImage
            | SourceConstruct::ShortcutReferenceImage
            | SourceConstruct::HtmlImage
    )
}

/// The link surface the Markdown profile extracts from one source: every
/// occurrence's semantic destination in document order, as decoded bytes. A
/// definition nobody consumes renders nothing, so it is not on the surface.
/// `github` applies the normalization the footnote suite applies to
/// github.com's own output: the source of an image that points at nothing but
/// a search or a hash is erased.
fn extracted(source: &str, github: bool) -> Option<Vec<Vec<u8>>> {
    let analysis = analyze(Adapter::Markdown, source.as_bytes(), u64::MAX).ok()?;
    let extraction = analysis.extraction?;
    Some(
        extraction
            .occurrences
            .iter()
            .filter(|entry| entry.construct != SourceConstruct::LinkReferenceDefinition)
            .map(|entry| {
                let destination = entry.semantic_destination.as_str();
                if github && is_image(entry.construct) && destination.starts_with(['?', '#']) {
                    return Vec::new();
                }
                corpus::percent_decoded(destination)
            })
            .collect(),
    )
}

fn readable(surface: &[Vec<u8>]) -> Vec<String> {
    surface
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect()
}

/// Compares one case's two surfaces and describes a mismatch for the report.
fn mismatch(case_id: &str, want: &[Vec<u8>], got: Option<&[Vec<u8>]>) -> Option<String> {
    if got == Some(want) {
        return None;
    }
    Some(format!(
        "{case_id}: upstream publishes {:?}, the profile extracts {:?}",
        readable(want),
        got.map(readable)
    ))
}

/// The core half of the grammar pin: with the extensions off, the parser
/// reproduces every executable `CommonMark` 0.31.2 example, up to the two
/// serialization choices `standardized` names.
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
        let got = render_commonmark(&case.source);
        if standardized(&got) != standardized(want) {
            broken.push(format!("{}: want {want:?}, got {got:?}", case.case_id()));
        }
    }
    assert!(
        broken.is_empty(),
        "{} of {} CommonMark examples differ: {broken:#?}",
        broken.len(),
        cases.len()
    );
}

/// The extension half: every example GFM 0.29 marks with an extension and
/// executes. Renderers disagree on the markup around a table or a footnote,
/// so the comparison is the link surface: the destinations the specification's
/// HTML publishes must be the destinations the Markdown profile extracts, in
/// order. That document's untagged examples are `CommonMark` 0.29, which
/// 0.31.2 supersedes, so they are corpus inputs rather than goldens here.
#[test]
fn reproduces_gfm_0_29_extensions() {
    let mut checked = 0_usize;
    let mut skipped = 0_usize;
    let mut surface = 0_usize;
    let mut broken = Vec::new();
    let mut details = Vec::new();
    for case in &family(corpus::GFM_FAMILY) {
        let (Some(_tag), Expect::Html(want)) = (case.tag.as_deref(), &case.expect) else {
            continue;
        };
        if case.tag.as_deref() == Some("disabled") {
            skipped = skipped.saturating_add(1);
            continue;
        }
        checked = checked.saturating_add(1);
        let want = published(want);
        surface = surface.saturating_add(want.len());
        let got = extracted(&case.source, false);
        if let Some(detail) = mismatch(&case.case_id(), &want, got.as_deref()) {
            broken.push(case.case_id());
            details.push(detail);
        }
    }
    assert_eq!(checked, 22, "GFM 0.29 executes 22 extension examples");
    assert_eq!(skipped, 2, "GFM 0.29 disables its 2 task-list examples");
    assert_eq!(surface, 19, "destinations the extension examples publish");
    assert_eq!(
        broken, GFM_DIVERGENCE,
        "GFM extension examples diverge from the pinned bundle beyond the recorded case: {details:#?}"
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
        let (options, _meter) = mdx_options(u64::MAX);
        let ours = render_mdx(&case.source, options);
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

/// Footnotes and single-tilde strikethrough are the pinned bundle's additions
/// beyond formal GFM 0.29, so they carry their own suites. The comparison is
/// the link surface, with the footnote machinery a renderer adds left out, and
/// under the pinned configuration that surface is empty: the value of the
/// suites here is that no footnote call or definition is read as a link.
#[test]
fn reproduces_the_footnote_and_tilde_suites() {
    let (all, _skipped) = harvest();
    let mut checked = 0_usize;
    let mut elsewhere = 0_usize;
    let mut surface = 0_usize;
    let mut broken = Vec::new();
    let mut details = Vec::new();
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
        let want = published(want);
        surface = surface.saturating_add(want.len());
        let got = extracted(&case.source, false);
        if let Some(detail) = mismatch(&case.case_id(), &want, got.as_deref()) {
            broken.push(case.case_id());
            details.push(detail);
        }
    }
    assert_eq!(checked, 22, "fixtures under the pinned configuration");
    assert_eq!(elsewhere, 7, "fixtures that configure another profile");
    assert_eq!(surface, 0, "nothing on these fixtures is a link");
    assert_eq!(
        broken, FOOTNOTE_DIVERGENCE,
        "footnote or tilde fixtures publish links the profile does not extract: {details:#?}"
    );
}

/// The one compensation the footnote suite applies to github.com's output that
/// still touches the link surface: github.com leaves `![image](#)` as text
/// inside a footnote-looking bracket, and the suite restores the image, so the
/// restored image is what the source is measured against. Keyed to the
/// document exactly as upstream keys it.
fn compensate(name: &str, expected: &str) -> String {
    if name == "constructs-in-identifiers" {
        return expected.replace("![image](#)", "<img src=\"\" alt=\"image\" />");
    }
    expected.to_owned()
}

/// The footnote suite also renders 29 documents against the HTML github.com
/// itself produces, which is where the interactions the spec names live: a
/// footnote call against a link, an image, a duplicate definition, a reference
/// definition, and nesting inside every container.
///
/// One of them differs, and it is this implementation that is wrong.
/// pulldown-cmark 0.13.4 forms neither a link nor an image whose label holds
/// a footnote call once a footnote definition exists, so `[link[^5]](#)` and
/// `![image[^4]](#)` stay literal where github.com makes them a link and an
/// image. The scanner would miss both references. It is recorded here, and it
/// is worth reporting upstream.
const GITHUB_DIVERGENCE: [&str; 1] = ["footnotes-in-constructs"];

#[test]
fn reproduces_githubs_own_footnote_rendering() {
    let pairs = github_pairs();
    assert_eq!(pairs.len(), 29, "the pinned footnote fixture directory");

    let mut surface = 0_usize;
    let mut broken = Vec::new();
    let mut details = Vec::new();
    for (name, source, html) in &pairs {
        let want = published(&compensate(name, html));
        surface = surface.saturating_add(want.len());
        let got = extracted(source, true);
        if let Some(detail) = mismatch(name, &want, got.as_deref()) {
            broken.push(name.clone());
            details.push(detail);
        }
    }
    assert_eq!(surface, 20, "destinations github.com publishes");
    assert_eq!(
        broken, GITHUB_DIVERGENCE,
        "github footnote rendering differs beyond the recorded case: {details:#?}"
    );
}
