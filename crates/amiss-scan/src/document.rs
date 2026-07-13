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
/// Other case or suffixes are not silently treated as equivalent.
#[must_use]
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the discovery contract is byte-exact: other case is not equivalent"
)]
pub fn classify(path: &str) -> Option<Classification> {
    if path.ends_with(".md") || path.ends_with(".markdown") {
        return Some(Classification::StructuredMarkdown);
    }
    if path.ends_with(".mdx") {
        return Some(Classification::StructuredMdx);
    }
    let basename = path.rsplit('/').next().unwrap_or(path);
    if EXTENSIONLESS.contains(&basename) {
        return Some(Classification::ExtensionlessMarkdown);
    }
    if basename == ".cursorrules" || basename == "llms.txt" {
        return Some(Classification::PlainAdvisory);
    }
    None
}

/// A document under a directory component in the closed excluded set is
/// discovered but excluded by built-in scope. The basename itself is not a
/// tree component, and matching is byte-exact.
#[must_use]
pub fn excluded_by_built_in(path: &str) -> bool {
    let Some((directories, _basename)) = path.rsplit_once('/') else {
        return false;
    };
    directories
        .split('/')
        .any(|component| EXCLUDED_TREES.contains(&component))
}
