use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

/// The five closed source adapters. Every wire string an adapter contributes
/// (identity, grammar profile, frontmatter contract, projection, address
/// scheme) is frozen here so no call site can spell one by hand.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Adapter {
    #[strum(serialize = "asciidoc")]
    AsciiDoc,
    Markdown,
    Mdx,
    PlainAdvisory,
    Rst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterMetadata {
    pub parser_name: &'static str,
    pub grammar_profile: &'static str,
    pub frontmatter_contract: &'static str,
    pub source_projection: &'static str,
    pub structural_address: &'static str,
}

const MARKDOWN_METADATA: AdapterMetadata = AdapterMetadata {
    parser_name: "amiss-markdown-adapter",
    grammar_profile: "commonmark-gfm",
    frontmatter_contract: "frontmatter",
    source_projection: "source-projection",
    structural_address: "markdown-ast-node-path",
};
const MDX_METADATA: AdapterMetadata = AdapterMetadata {
    parser_name: "amiss-mdx-adapter",
    grammar_profile: "mdx-source",
    frontmatter_contract: "frontmatter",
    source_projection: "source-projection",
    structural_address: "mdx-ast-node-path",
};
const ASCIIDOC_METADATA: AdapterMetadata = AdapterMetadata {
    parser_name: "amiss-asciidoc-adapter",
    grammar_profile: "asciidoctor-2",
    frontmatter_contract: "none",
    source_projection: "source-projection",
    structural_address: "asciidoc-block-path",
};
const RST_METADATA: AdapterMetadata = AdapterMetadata {
    parser_name: "amiss-rst-adapter",
    grammar_profile: "docutils-rst-sphinx-refs",
    frontmatter_contract: "none",
    source_projection: "source-projection",
    structural_address: "rst-block-path",
};
const PLAIN_ADVISORY_METADATA: AdapterMetadata = AdapterMetadata {
    parser_name: "amiss-plain-advisory",
    grammar_profile: "plain-zero-lexer",
    frontmatter_contract: "none",
    source_projection: "none",
    structural_address: "none",
};

impl Adapter {
    #[must_use]
    pub const fn metadata(self) -> &'static AdapterMetadata {
        match self {
            Self::Markdown => &MARKDOWN_METADATA,
            Self::Mdx => &MDX_METADATA,
            Self::AsciiDoc => &ASCIIDOC_METADATA,
            Self::Rst => &RST_METADATA,
            Self::PlainAdvisory => &PLAIN_ADVISORY_METADATA,
        }
    }
}
