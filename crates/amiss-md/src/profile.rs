use std::cell::Cell;
use std::rc::Rc;

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

#[derive(Debug)]
struct MeterState {
    allowance: u64,
    spent: Cell<u64>,
    tripped: Cell<bool>,
}

/// The in-parse work bound over embedded-code rescans. Every candidate close
/// of a code region charges the whole accumulated region before the lexical
/// scan reads it, so a document whose regions stay open cannot demand more
/// scanning than the granted allowance. A crossing aborts the parse and sets
/// `tripped`, which is how the caller tells a spent allowance from a grammar
/// rejection; `spent` is exact on success and the observed lower bound on a
/// trip.
#[derive(Clone, Debug)]
pub struct EmbeddedCodeMeter(Rc<MeterState>);

impl EmbeddedCodeMeter {
    fn new(allowance: u64) -> Self {
        Self(Rc::new(MeterState {
            allowance,
            spent: Cell::new(0),
            tripped: Cell::new(false),
        }))
    }

    #[must_use]
    pub fn spent(&self) -> u64 {
        self.0.spent.get()
    }

    #[must_use]
    pub fn tripped(&self) -> bool {
        self.0.tripped.get()
    }

    fn ask(&self, source: &str) -> MdxSignal {
        let total = self
            .0
            .spent
            .get()
            .saturating_add(u64::try_from(source.len()).unwrap_or(u64::MAX));
        self.0.spent.set(total);
        if total > self.0.allowance {
            self.0.tripped.set(true);
            return MdxSignal::Error(
                "Embedded code crossed the evaluation-byte allowance".to_owned(),
                0,
                Box::new("amiss".to_owned()),
                Box::new("embedded-code-allowance".to_owned()),
            );
        }
        code_ends_here(source)
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

/// The grammar pin. `commonmark-gfm` is `CommonMark` plus exactly the
/// `remark-gfm` bundle with single-tilde strikethrough; `mdx-source` is that
/// profile plus MDX ESM, JSX, and expressions, minus the constructs MDX removes
/// (indented code, raw HTML, and plain autolinks). `plain-zero-lexer` runs
/// no grammar at all. The returned meter charges every embedded-code ask
/// against `embedded_code_allowance`; only the MDX profile ever spends it.
#[must_use]
pub fn parse_options(
    adapter: Adapter,
    embedded_code_allowance: u64,
) -> Option<(ParseOptions, EmbeddedCodeMeter)> {
    let meter = EmbeddedCodeMeter::new(embedded_code_allowance);
    match adapter {
        Adapter::Markdown => Some((
            ParseOptions {
                constructs: gfm(),
                gfm_strikethrough_single_tilde: true,
                ..ParseOptions::default()
            },
            meter,
        )),
        Adapter::Mdx => {
            let expression = meter.clone();
            let esm = meter.clone();
            Some((
                ParseOptions {
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
                    mdx_expression_parse: Some(Box::new(
                        move |source, _kind: &MdxExpressionKind| expression.ask(source),
                    )),
                    mdx_esm_parse: Some(Box::new(move |source| esm.ask(source))),
                    ..ParseOptions::default()
                },
                meter,
            ))
        }
        Adapter::PlainAdvisory => None,
    }
}
