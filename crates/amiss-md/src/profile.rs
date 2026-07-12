use amiss_wire::model::Adapter;
use markdown::{Constructs, MdxExpressionKind, MdxSignal, ParseOptions};

use crate::js::{Completeness, completeness};

fn gfm() -> Constructs {
    Constructs {
        gfm_autolink_literal: true,
        gfm_footnote_definition: true,
        gfm_label_start_footnote: true,
        gfm_strikethrough: true,
        gfm_table: true,
        gfm_task_list_item: true,
        ..Constructs::default()
    }
}

/// Answers the parser's question at every candidate close of an embedded code
/// region. Incomplete code keeps the region open, which is what keeps a `}`
/// inside a string or comment from cutting it short. The code is never judged
/// valid or invalid, so this never rejects a document.
fn code_ends_here(source: &str) -> MdxSignal {
    match completeness(source) {
        Completeness::Complete => MdxSignal::Ok,
        Completeness::Incomplete => MdxSignal::Eof(
            "Unexpected end of file in embedded code".to_owned(),
            Box::new("amiss".to_owned()),
            Box::new("incomplete-code".to_owned()),
        ),
    }
}

/// The grammar pin. `commonmark-gfm-v1` is `CommonMark` plus exactly the
/// `remark-gfm` bundle with single-tilde strikethrough; `mdx-source-v1` is that
/// profile plus MDX ESM, JSX, and expressions, minus the constructs MDX removes
/// (indented code, raw HTML, and plain autolinks). `plain-zero-lexer-v1` runs
/// no grammar at all.
#[must_use]
pub fn parse_options(adapter: Adapter) -> Option<ParseOptions> {
    match adapter {
        Adapter::Markdown => Some(ParseOptions {
            constructs: gfm(),
            gfm_strikethrough_single_tilde: true,
            ..ParseOptions::default()
        }),
        Adapter::Mdx => Some(ParseOptions {
            constructs: Constructs {
                autolink: false,
                code_indented: false,
                html_flow: false,
                html_text: false,
                mdx_esm: true,
                mdx_expression_flow: true,
                mdx_expression_text: true,
                mdx_jsx_flow: true,
                mdx_jsx_text: true,
                ..gfm()
            },
            gfm_strikethrough_single_tilde: true,
            mdx_expression_parse: Some(Box::new(|source, _kind: &MdxExpressionKind| {
                code_ends_here(source)
            })),
            mdx_esm_parse: Some(Box::new(code_ends_here)),
            ..ParseOptions::default()
        }),
        Adapter::PlainAdvisory => None,
    }
}
