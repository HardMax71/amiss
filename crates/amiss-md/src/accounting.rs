use std::panic::{AssertUnwindSafe, catch_unwind};

use amiss_wire::model::Adapter;
use amiss_wire::report::AnalysisErrorCode;
use markdown::mdast::Node;
use markdown::to_mdast;

use crate::frontmatter;
use crate::lines::scan;
use crate::profile::parse_options;

/// The frozen node resources of `parser-work-accounting-v1`: `nodes` is the
/// logical node count of one document and `nesting` its maximum node depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Work {
    pub nodes: u64,
    pub nesting: u64,
}

/// The parse-phase faults an adapter can raise, in the contract's precedence.
/// A grammar rejection is attributable to the source and is therefore
/// `DOCUMENT_INVALID`, not a parser failure. `PARSER_PANIC` is reserved for a
/// panic that bypasses the parser's own result, which the engine must catch
/// rather than abort on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    DocumentInvalid,
    ParserPanic,
}

impl From<Fault> for AnalysisErrorCode {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::DocumentInvalid => Self::DocumentInvalid,
            Fault::ParserPanic => Self::ParserPanic,
        }
    }
}

/// Charges one document against the adapter's grammar. Frontmatter is
/// recognized first and contributes no node; only the suffix reaches the
/// parser. An empty document still charges one root node at depth one.
///
/// # Errors
///
/// `DocumentInvalid` when the bytes are not UTF-8 under a parsing adapter or
/// the grammar rejects the source, and `ParserPanic` when the parser panics.
pub fn charge(adapter: Adapter, source: &[u8]) -> Result<Work, Fault> {
    let Some(options) = parse_options(adapter) else {
        return Ok(plain(source));
    };
    let text = str::from_utf8(source).map_err(|_invalid| Fault::DocumentInvalid)?;
    let suffix_offset = frontmatter::recognize(source).map_or(0, |region| region.suffix_offset);
    let suffix = text.get(suffix_offset..).ok_or(Fault::DocumentInvalid)?;
    let parsed = catch_unwind(AssertUnwindSafe(|| to_mdast(suffix, &options)))
        .map_err(|_panic| Fault::ParserPanic)?;
    let tree = parsed.map_err(|_rejected| Fault::DocumentInvalid)?;
    Ok(walk(&tree))
}

/// Counts the root and every node reachable through the ordered `children` of
/// the logical tree. Iterative because a hostile document may nest deeper than
/// the stack allows.
fn walk(root: &Node) -> Work {
    let mut work = Work {
        nodes: 0,
        nesting: 0,
    };
    let mut pending = vec![(root, 1_u64)];
    while let Some((node, depth)) = pending.pop() {
        work.nodes = work.nodes.saturating_add(1);
        work.nesting = work.nesting.max(depth);
        if let Some(children) = node.children() {
            let below = depth.saturating_add(1);
            pending.extend(children.iter().map(|child| (child, below)));
        }
    }
    work
}

/// One synthetic root plus one synthetic paragraph for every maximal run of
/// nonblank lines. Depth is one with no run and two otherwise.
fn plain(source: &[u8]) -> Work {
    let mut runs: u64 = 0;
    let mut inside = false;
    for line in scan(source) {
        if line.is_blank(source) {
            inside = false;
        } else if !inside {
            inside = true;
            runs = runs.saturating_add(1);
        }
    }
    Work {
        nodes: runs.saturating_add(1),
        nesting: if runs == 0 { 1 } else { 2 },
    }
}
