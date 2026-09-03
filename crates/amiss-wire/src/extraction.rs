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
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::IntoStaticStr,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum BlockKind {
    Paragraph,
    ListItem,
    TableCell,
    DocumentRoot,
}

/// What an include contributes to the document stream when its syntax is
/// closed enough for the scanner to reproduce it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransclusionKind {
    Parsed,
    Literal,
}

/// Why an include cannot participate in the local expansion graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransclusionRefusal {
    Context,
    DynamicTarget,
    Options,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transclusion {
    pub target: String,
    pub span: (usize, usize),
    pub kind: Result<TransclusionKind, TransclusionRefusal>,
}

/// One extracted reference. `raw_destination` is the exact source-token byte
/// slice (without syntactic angle brackets, and from the first winning
/// definition for reference forms); `semantic_destination` is the token after
/// the construct's own decoding, which is exactly what the parser publishes as
/// the node's URL. Spans are zero-based half-open byte offsets into the raw
/// document, while `node_path` is the child-index path from the
/// post-frontmatter root to the syntax node itself; a destination mined out
/// of a block node, raw HTML or an orphaned definition, appends its ordinal
/// within the node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub construct: SourceConstruct,
    pub raw_destination: String,
    pub semantic_destination: String,
    pub span: (usize, usize),
    pub node_path: Vec<usize>,
    pub block_kind: BlockKind,
    pub block_span: (usize, usize),
    /// The document byte range of the destination's fragment text, present
    /// only when the adapter located the raw destination verbatim exactly
    /// once inside the reference and nothing a decoder could alter sits in
    /// the fragment. Absent means no edit may claim those bytes.
    pub fragment_span: Option<(usize, usize)>,
    /// The document byte range of the destination's path part, under the
    /// same certainty rules. Absent means no edit may claim those bytes.
    pub path_span: Option<(usize, usize)>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum HeadingSource {
    Markdown,
    #[strum(serialize = "asciidoc")]
    AsciiDoc,
    Rst,
    RawHtml,
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
    pub transclusions: Vec<Transclusion>,
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

/// One reserved governed definition: its complete node span, from the
/// opening bracket through the exclusive end of the destination and title
/// syntax. `angled` survives because the decoded url no longer shows
/// whether the destination was written in angle brackets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedDefinition {
    pub span: (usize, usize),
    pub url: String,
    pub title: Option<String>,
    pub label: String,
    pub angled: bool,
    pub previous_code: Option<SemanticCodeBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticCodeBlock {
    pub span: (usize, usize),
    pub value: String,
}

#[must_use]
pub fn governed_name_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.len() <= 120
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// One reserved carrier line, the non-Markdown spelling of the governed
/// channel: `[amiss:name]: <dest> "title"`, double or single quotes, bytes
/// taken literally with no entity decoding, nothing else on the line.
#[must_use]
pub fn governed_carrier_line(line: &str) -> Option<(String, String, String)> {
    let rest = line.strip_prefix("[amiss:")?;
    let (name, rest) = rest.split_once("]: <")?;
    if name.is_empty() || name.contains('[') || name.contains(']') {
        return None;
    }
    let (dest, rest) = rest.split_once("> ")?;
    if dest.is_empty() || dest.contains('<') || dest.contains('>') {
        return None;
    }
    let title = match rest.strip_prefix('"') {
        Some(tail) => tail.strip_suffix('"').filter(|body| !body.contains('"'))?,
        None => rest
            .strip_prefix('\'')?
            .strip_suffix('\'')
            .filter(|body| !body.contains('\''))?,
    };
    Some((format!("amiss:{name}"), dest.to_owned(), title.to_owned()))
}

/// The document byte range of a destination's fragment: present only when
/// the raw destination appears verbatim exactly once inside the reference
/// span, carries a single `#`, and holds nothing a decoder could alter on
/// either side. Anything less certain names no bytes. Adapters add their own
/// construct gates in front of this core.
#[must_use]
pub fn fragment_span(
    source: &[u8],
    span: (usize, usize),
    raw_destination: &str,
) -> Option<(usize, usize)> {
    let (prefix, fragment) = raw_destination.split_once('#')?;
    if fragment.is_empty()
        || fragment.contains(['#', '%', '&', '\\'])
        || prefix.contains(['%', '&', '\\'])
    {
        return None;
    }
    let start = locate_destination(source, span, raw_destination)?
        .checked_add(prefix.len())?
        .checked_add(1)?;
    Some((start, start.checked_add(fragment.len())?))
}

/// The document byte range of a destination's path part: the destination
/// located verbatim exactly once, up to its first `#` or its whole text,
/// holding nothing a decoder could alter. Anything less names no bytes.
#[must_use]
pub fn path_span(
    source: &[u8],
    span: (usize, usize),
    raw_destination: &str,
) -> Option<(usize, usize)> {
    let part = raw_destination
        .split_once('#')
        .map_or(raw_destination, |(prefix, _)| prefix);
    if part.is_empty() || part.contains(['%', '&', '\\']) {
        return None;
    }
    let at = locate_destination(source, span, raw_destination)?;
    Some((at, at.checked_add(part.len())?))
}

/// The one place a raw destination's own text begins inside its reference
/// span, or None on zero hits, two hits, or a span past the source. An empty
/// destination is refused here, where a zero-width window would otherwise
/// match everywhere.
fn locate_destination(source: &[u8], span: (usize, usize), raw_destination: &str) -> Option<usize> {
    let slice = source.get(span.0..span.1)?;
    let needle = raw_destination.as_bytes();
    if needle.is_empty() {
        return None;
    }
    let mut hits = slice
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(at, _)| at);
    let at = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    span.0.checked_add(at)
}
