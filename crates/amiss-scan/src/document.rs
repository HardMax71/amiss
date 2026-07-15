use amiss_wire::model::Adapter;

/// The intrinsic built-in classification rows, applied first match wins. The
/// fifth row, `policy-included`, needs a repository policy and is not a
/// property of the path alone, so it lives with the policy layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classification {
    StructuredMarkdown,
    StructuredMdx,
    ExtensionlessMarkdown,
    PlainAdvisory,
    PolicyIncluded,
}

const EXTENSIONLESS: [&str; 6] = [
    "README",
    "CONTRIBUTING",
    "CHANGELOG",
    "SECURITY",
    "SUPPORT",
    "CODE_OF_CONDUCT",
];

const EXCLUDED_TREES: [&str; 7] = [
    "node_modules",
    "vendor",
    "third_party",
    "dist",
    "build",
    ".next",
    "target",
];

impl Classification {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StructuredMarkdown => "structured-markdown",
            Self::StructuredMdx => "structured-mdx",
            Self::ExtensionlessMarkdown => "extensionless-markdown",
            Self::PlainAdvisory => "plain-advisory",
            Self::PolicyIncluded => "policy-included",
        }
    }

    /// The native adapter, where one exists: a policy include installs no
    /// parser.
    #[must_use]
    pub const fn adapter(self) -> Option<Adapter> {
        match self {
            Self::StructuredMarkdown | Self::ExtensionlessMarkdown => Some(Adapter::Markdown),
            Self::StructuredMdx => Some(Adapter::Mdx),
            Self::PlainAdvisory => Some(Adapter::PlainAdvisory),
            Self::PolicyIncluded => None,
        }
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
