use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::{
    Analysis, AnalyzeError, BlockKind, Extraction, Fault, Heading, HeadingSource, Occurrence,
    Opaque, Work,
};

use crate::macros::{Reference, ReferenceKind};
use crate::{Refusal, extract};

/// Reads one `AsciiDoc` document into the vocabulary every adapter speaks.
///
/// # Errors
///
/// `DocumentInvalid` when the bytes are not UTF-8; `AsciiDoc` is text.
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
        occurrences.push(occurrence(reference, within.1));
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
                    source: HeadingSource::AsciiDoc,
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
        ReferenceKind::InternalCrossReference => format!("#{}", reference.target),
        ReferenceKind::CrossReference
        | ReferenceKind::Link
        | ReferenceKind::BlockImage
        | ReferenceKind::InlineImage
        | ReferenceKind::Include => reference.target.clone(),
    };
    Occurrence {
        construct: construct(reference.kind),
        raw_destination: reference.target.clone(),
        semantic_destination: semantic,
        span: reference.span,
        node_path: vec![reference.block, within],
        block_kind: if reference.list_item {
            BlockKind::ListItem
        } else {
            BlockKind::Paragraph
        },
        block_span: reference.block_span,
        fragment_span: None,
    }
}

const fn construct(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::CrossReference => SourceConstruct::AsciidocCrossReference,
        ReferenceKind::InternalCrossReference => SourceConstruct::AsciidocInternalCrossReference,
        ReferenceKind::Link => SourceConstruct::AsciidocLinkMacro,
        ReferenceKind::BlockImage => SourceConstruct::AsciidocBlockImage,
        ReferenceKind::InlineImage => SourceConstruct::AsciidocInlineImage,
        ReferenceKind::Include => SourceConstruct::AsciidocInclude,
    }
}
