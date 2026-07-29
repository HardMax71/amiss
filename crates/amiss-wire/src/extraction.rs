use crate::controls::SourceConstruct;
use crate::report::AnalysisErrorCode;

/// The frozen node resources of `parser-work-accounting`: `nodes` is the
/// logical node count of one document and `nesting` its maximum node depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Work {
    pub nodes: u64,
    pub nesting: u64,
}

/// The parse-phase faults an adapter can raise, in the contract's precedence.
/// A grammar rejection is attributable to the source and is therefore
/// `DocumentInvalid`, not a parser failure. `ParserError` is the parser
/// breaking its own tree contract after accepting the source, `ParserPanic` a
/// panic that bypasses its result (which the engine must catch rather than
/// abort on), and `InvalidSourceSpan` a returned tree whose byte spans violate
/// the closed source contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    DocumentInvalid,
    ParserError,
    ParserPanic,
    InvalidSourceSpan,
}

impl From<Fault> for AnalysisErrorCode {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::DocumentInvalid => Self::DocumentInvalid,
            Fault::ParserError => Self::ParserError,
            Fault::ParserPanic => Self::ParserPanic,
            Fault::InvalidSourceSpan => Self::InvalidSourceSpan,
        }
    }
}

/// How one guarded parse fails: a fault in the contract's precedence, or a
/// parse the embedded-code meter ended because the granted allowance was
/// spent. The second is a resource crossing and never an attribution to the
/// document, which is why it does not live in `Fault`; `spent` is the meter's
/// total at the abort, the observed lower bound the caller reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyzeError {
    Fault(Fault),
    EmbeddedCodeAllowance { spent: u64 },
}

impl From<Fault> for AnalyzeError {
    fn from(fault: Fault) -> Self {
        Self::Fault(fault)
    }
}

/// The block owner of one occurrence, selected by the override order: the
/// nearest ancestor list item if any, otherwise the nearest table cell,
/// otherwise the nearest paragraph, otherwise the document root. Raw HTML can
/// never own an extracted construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    ListItem,
    TableCell,
    DocumentRoot,
}

impl BlockKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::ListItem => "list-item",
            Self::TableCell => "table-cell",
            Self::DocumentRoot => "document-root",
        }
    }
}

/// One extracted reference. `raw_destination` is the exact source-token byte
/// slice (without syntactic angle brackets, and from the first winning
/// definition for reference forms); `semantic_destination` is the token after
/// the construct's own decoding, which is exactly what the parser publishes as
/// the node's URL. Spans are zero-based half-open byte offsets into the raw
/// document, while `node_path` is the child-index path from the
/// post-frontmatter root to the syntax node itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub construct: SourceConstruct,
    pub raw_destination: String,
    pub semantic_destination: String,
    pub span: (usize, usize),
    pub node_path: Vec<usize>,
    pub block_kind: BlockKind,
    pub block_span: (usize, usize),
}

/// The opaque partition of one document: the frontmatter region's byte count,
/// then MDX intervals, then raw-HTML intervals on the remaining surface. The
/// three never overlap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opaque {
    pub frontmatter_bytes: usize,
    pub mdx: Vec<(usize, usize)>,
    pub html: Vec<(usize, usize)>,
}

/// The trailing attribute syntax a heading may carry. Renderers disagree about
/// it, so `suffix` keeps the exact bytes removed from the text: one group
/// publishes `id`, the other reads the text and the suffix together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingAttribute {
    pub id: String,
    pub suffix: String,
}

/// Where a heading was written. Only some renderers build an identity from one
/// written as raw HTML, so the two are kept apart in one ordered list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadingSource {
    Markdown,
    RawHtml,
}

impl HeadingSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::RawHtml => "raw-html",
        }
    }
}

/// One heading's rendered text content, in document order with its siblings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub text: String,
    pub attribute: Option<HeadingAttribute>,
    pub source: HeadingSource,
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extraction {
    pub occurrences: Vec<Occurrence>,
    pub opaque: Opaque,
    pub governed: Vec<GovernedDefinition>,
    pub headings: Vec<Heading>,
    pub html_anchors: Vec<String>,
    pub declared_anchors: Vec<String>,
}

/// Everything one parse yields: the work charge, the embedded-code bytes the
/// grammar's candidate-close asks spent, and the extraction for a parsing
/// adapter. The plain adapter has no spans, addresses, or occurrences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub work: Work,
    pub embedded_code_bytes: u64,
    pub extraction: Option<Extraction>,
}

/// One reserved governed definition: its complete node span, from the opening
/// bracket through the exclusive end of the destination and title syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedDefinition {
    pub span: (usize, usize),
}
