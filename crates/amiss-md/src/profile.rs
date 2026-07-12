use amiss_wire::model::Adapter;
use markdown::{Constructs, ParseOptions};

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

/// The grammar pin. `commonmark-gfm-v1` is `CommonMark` plus exactly the
/// `remark-gfm` bundle with single-tilde strikethrough; `mdx-source-v1` is that
/// profile plus MDX ESM, JSX, and expressions, with the constructs MDX removes.
/// `plain-zero-lexer-v1` runs no grammar at all.
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
            ..ParseOptions::default()
        }),
        Adapter::PlainAdvisory => None,
    }
}
