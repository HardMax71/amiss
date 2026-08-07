use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::{
    Analysis, AnalyzeError, BlockKind, Extraction, Fault, Heading, HeadingSource, Occurrence,
    Opaque, Work,
};

use crate::{Reference, ReferenceKind, Refusal, extract};

/// Reads one reStructuredText document into the vocabulary every adapter
/// speaks.
///
/// # Errors
///
/// `DocumentInvalid` when the bytes are not UTF-8.
pub fn analyze(source: &[u8]) -> Result<Analysis, AnalyzeError> {
    let read = extract(source).map_err(|Refusal::NotUtf8| Fault::DocumentInvalid)?;
    let nodes = read
        .blocks
        .saturating_add(read.references.len())
        .saturating_add(read.titles.len());
    let mut occurrences = Vec::with_capacity(read.references.len());
    let mut within = (usize::MAX, 0_usize);
    for reference in &read.references {
        within = if within.0 == reference.block {
            (reference.block, within.1.saturating_add(1))
        } else {
            (reference.block, 0)
        };
        let mut entry = occurrence(reference, within.1);
        entry.fragment_span =
            amiss_wire::extraction::fragment_span(source, reference.span, &reference.target);
        occurrences.push(entry);
    }
    Ok(Analysis {
        work: Work {
            nodes: u64::try_from(nodes).unwrap_or(u64::MAX),
            nesting: u64::try_from(read.nesting.saturating_add(1)).unwrap_or(u64::MAX),
        },
        embedded_code_bytes: 0,
        extraction: Some(Extraction {
            occurrences,
            opaque: Opaque {
                frontmatter_bytes: 0,
                mdx: Vec::new(),
                html: read.opaque,
            },
            governed: Vec::new(),
            headings: read
                .titles
                .into_iter()
                .map(|title| Heading {
                    text: title.text,
                    attribute: None,
                    source: HeadingSource::Rst,
                    span: title.span,
                })
                .collect(),
            html_anchors: Vec::new(),
            declared_anchors: read.anchors,
        }),
    })
}

fn occurrence(reference: &Reference, within: usize) -> Occurrence {
    let semantic = match reference.kind {
        ReferenceKind::DocRole => doc_destination(&reference.target),
        ReferenceKind::InlineHyperlink
        | ReferenceKind::NamedTarget
        | ReferenceKind::Image
        | ReferenceKind::Include
        | ReferenceKind::FileOption
        | ReferenceKind::RefRole => reference.target.clone(),
    };
    Occurrence {
        construct: construct(reference.kind),
        raw_destination: reference.target.clone(),
        semantic_destination: semantic,
        span: reference.span,
        node_path: vec![reference.block, within],
        block_kind: BlockKind::Paragraph,
        block_span: reference.block_span,
        fragment_span: None,
    }
}

/// A `:doc:` target is extensionless in Sphinx, so the source suffix is
/// appended here; a source-root-absolute target keeps its slash and stays a
/// declared site route, because the engine does not know the Sphinx root.
fn doc_destination(target: &str) -> String {
    let last = target.rsplit('/').next().unwrap_or(target);
    if target.starts_with('/') || last.contains('.') {
        target.to_owned()
    } else {
        format!("{target}.rst")
    }
}

const fn construct(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::InlineHyperlink => SourceConstruct::RstInlineHyperlink,
        ReferenceKind::NamedTarget => SourceConstruct::RstNamedTarget,
        ReferenceKind::Image => SourceConstruct::RstImageDirective,
        ReferenceKind::Include => SourceConstruct::RstIncludeDirective,
        ReferenceKind::FileOption => SourceConstruct::RstFileOption,
        ReferenceKind::DocRole => SourceConstruct::RstDocRole,
        ReferenceKind::RefRole => SourceConstruct::RstRefRole,
    }
}
