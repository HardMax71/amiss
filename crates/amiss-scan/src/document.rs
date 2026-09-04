use amiss_wire::model::Adapter;
pub use amiss_wire::report::model::DocumentClassification as Classification;

const EXTENSIONLESS: [&str; 6] = [
    "README",
    "CONTRIBUTING",
    "CHANGELOG",
    "SECURITY",
    "SUPPORT",
    "CODE_OF_CONDUCT",
];

const EXCLUDED_TREES: [&str; 9] = [
    "node_modules",
    "vendor",
    "third_party",
    "dist",
    "build",
    ".next",
    "target",
    "test",
    "tests",
];

/// A policy include needs an explicit binding; unparsed markup has no native adapter.
#[must_use]
pub const fn native_adapter(classification: Classification) -> Option<Adapter> {
    match classification {
        Classification::StructuredMarkdown | Classification::ExtensionlessMarkdown => {
            Some(Adapter::Markdown)
        }
        Classification::StructuredMdx => Some(Adapter::Mdx),
        Classification::StructuredAsciiDoc => Some(Adapter::AsciiDoc),
        Classification::StructuredRst => Some(Adapter::Rst),
        Classification::PlainAdvisory => Some(Adapter::PlainAdvisory),
        Classification::UnparsedMarkup | Classification::PolicyIncluded => None,
    }
}

/// Classifies one repository path by the closed built-in rows: exact lowercase
/// suffix, then exact extensionless basename, then exact advisory basename.
/// Other case or suffixes are not silently treated as equivalent, and the
/// rows read raw bytes, so a path text cannot hold still classifies.
#[must_use]
pub fn classify(path: &[u8]) -> Option<Classification> {
    if path.ends_with(b".md") || path.ends_with(b".markdown") {
        return Some(Classification::StructuredMarkdown);
    }
    if path.ends_with(b".mdx") {
        return Some(Classification::StructuredMdx);
    }
    if path.ends_with(b".adoc") || path.ends_with(b".asciidoc") {
        return Some(Classification::StructuredAsciiDoc);
    }
    if path.ends_with(b".rst") {
        return Some(Classification::StructuredRst);
    }
    if path.ends_with(b".ipynb") || path.ends_with(b".org") {
        return Some(Classification::UnparsedMarkup);
    }
    let basename = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    if EXTENSIONLESS.iter().any(|name| name.as_bytes() == basename) {
        return Some(Classification::ExtensionlessMarkdown);
    }
    if basename == b".cursorrules" || basename == b"llms.txt" {
        return Some(Classification::PlainAdvisory);
    }
    None
}

/// A document under a directory component in the closed excluded set is
/// discovered but excluded by built-in scope. The basename itself is not a
/// tree component, and matching is byte-exact.
#[must_use]
pub fn excluded_by_built_in(path: &[u8]) -> bool {
    let Some(split) = path.iter().rposition(|byte| *byte == b'/') else {
        return false;
    };
    path.get(..split).is_some_and(|directories| {
        directories.split(|byte| *byte == b'/').any(|component| {
            EXCLUDED_TREES
                .iter()
                .any(|tree| tree.as_bytes() == component)
        })
    })
}
