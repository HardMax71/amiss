mod identity;

use amiss_wire::model::Adapter;

pub use identity::{anchor_set, identities};

/// The Unicode normalization a renderer applies before it reads the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normalize {
    None,
    Nfc,
    Nfkd,
}

/// What a renderer does to the decomposed text: nothing, drop everything
/// outside ASCII, or drop the Latin combining block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    None,
    AsciiIgnore,
    LatinMarks,
}

/// Whether case folding runs over the whole string before the filter, with the
/// full Unicode mapping, or per surviving character with the simple one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    FullBeforeFilter,
    SimpleAfterFilter,
}

/// The characters a renderer carries into the identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    LetterMarkNumberConnector,
    LetterNumberUnderscore,
    AlphabeticNumericUnderscore,
    AsciiAlphanumeric,
    AsciidoctorId,
    AnythingButC0,
}

/// The characters a renderer turns into a separator. A hyphen is one under
/// every rule, either because it survives or because it is replaced by one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Separators {
    Space,
    Whitespace,
    WhitespaceUnderscore,
    MditVuePunctuation,
    NonAlphanumeric,
}

/// When whitespace is trimmed: never, before the filter runs, or after
/// removal but before separators are mapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trim {
    None,
    Before,
    AfterRemoval,
}

/// What a renderer publishes for a heading whose identity came out empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Empty {
    Drop,
    Keep,
    Fill(&'static str),
}

/// Whether the renderer rewrites dashes and ellipses before it reads the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typography {
    Plain,
    SmartPunctuation,
}

/// Whether the leading run of non-letters is dropped before anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    AsWritten,
    StripNonLetter,
}

/// Whether a run of separators becomes one separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runs {
    AsWritten,
    Collapse,
}

/// Whether separators at either end survive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edges {
    AsWritten,
    Trim,
    TrimEnd,
}

/// Whether a heading's own `{#id}` becomes the identity or stays text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribute {
    Literal,
    Honored,
}

/// Whether the renderer builds an identity from a heading written as raw HTML.
/// The ones that do run over the rendered document rather than over the
/// Markdown tree, so they see both kinds in one sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawHtml {
    Anchored,
    Ignored,
}

/// Whether the renderer builds an identity from a definition-list term. Only
/// Hugo does, under `autoDefinitionTermID`, and it numbers a repeat on the
/// same counter the headings use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terms {
    Anchored,
    Ignored,
}

/// How a repeated identity is made unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duplicates {
    Dash,
    Underscore,
    UnderscoreFromTwo,
    Collide,
}

/// One renderer's heading-identity rule, as a table rather than as code, so a
/// reader can compare two renderers by reading two rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorRule {
    pub name: &'static str,
    pub typography: Typography,
    pub normalize: Normalize,
    pub fold: Fold,
    pub head: Head,
    pub trim: Trim,
    pub case: Case,
    pub keep: Keep,
    pub separators: Separators,
    pub runs: Runs,
    pub edges: Edges,
    pub leading_digit_prefix: Option<&'static str>,
    pub separator: char,
    pub prefix: Option<&'static str>,
    pub empty: Empty,
    pub duplicates: Duplicates,
    pub attribute: Attribute,
    pub raw_html: RawHtml,
    pub terms: Terms,
}

/// The rule github.com publishes heading identities under, which is also the
/// one Hugo's default `autoHeadingIDType` spells and the one the term rule
/// below reads its construct with.
const fn github() -> AnchorRule {
    AnchorRule {
        name: "github",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::None,
        case: Case::FullBeforeFilter,
        keep: Keep::LetterMarkNumberConnector,
        separators: Separators::Space,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Dash,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Anchored,
        terms: Terms::Ignored,
    }
}

/// The identity Hugo publishes for a definition-list term under
/// `autoDefinitionTermID`, which is the slug its heading rule builds, on the
/// counter its headings occupy. The rule sits beside the table rather than
/// inside it because it reads a construct no renderer in the table reads, so
/// what it adds to the union is the terms and nothing else.
pub const DEFINITION_TERMS: AnchorRule = AnchorRule {
    name: "definition-term",
    terms: Terms::Anchored,
    ..github()
};

/// Every renderer rule the resolver knows. Adding one can only grow the set an
/// anchor may match, so the set is the union and a missing rule is the only
/// way to report a live anchor as absent.
pub const RULES: [AnchorRule; 12] = [
    github(),
    AnchorRule {
        name: "gitea",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::SimpleAfterFilter,
        keep: Keep::LetterNumberUnderscore,
        separators: Separators::Whitespace,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Drop,
        duplicates: Duplicates::Collide,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "forgejo",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::SimpleAfterFilter,
        keep: Keep::LetterNumberUnderscore,
        separators: Separators::Whitespace,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Fill("heading"),
        duplicates: Duplicates::Dash,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "mdbook",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::FullBeforeFilter,
        keep: Keep::AlphabeticNumericUnderscore,
        separators: Separators::Whitespace,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Dash,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "mdbook-smart",
        typography: Typography::SmartPunctuation,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::FullBeforeFilter,
        keep: Keep::AlphabeticNumericUnderscore,
        separators: Separators::Whitespace,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Dash,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "goldmark",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::AsciiIgnore,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::SimpleAfterFilter,
        keep: Keep::AsciiAlphanumeric,
        separators: Separators::WhitespaceUnderscore,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Fill("heading"),
        duplicates: Duplicates::Dash,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "python-markdown",
        typography: Typography::Plain,
        normalize: Normalize::Nfkd,
        fold: Fold::AsciiIgnore,
        head: Head::AsWritten,
        trim: Trim::AfterRemoval,
        case: Case::FullBeforeFilter,
        keep: Keep::LetterNumberUnderscore,
        separators: Separators::Whitespace,
        runs: Runs::Collapse,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Underscore,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "pymdownx",
        typography: Typography::Plain,
        normalize: Normalize::Nfc,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::FullBeforeFilter,
        keep: Keep::LetterNumberUnderscore,
        separators: Separators::Space,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Underscore,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "mdit-vue",
        typography: Typography::Plain,
        normalize: Normalize::Nfkd,
        fold: Fold::LatinMarks,
        head: Head::AsWritten,
        trim: Trim::None,
        case: Case::FullBeforeFilter,
        keep: Keep::AnythingButC0,
        separators: Separators::MditVuePunctuation,
        runs: Runs::Collapse,
        edges: Edges::Trim,
        leading_digit_prefix: Some("_"),
        separator: '-',
        prefix: None,
        empty: Empty::Keep,
        duplicates: Duplicates::Dash,
        attribute: Attribute::Honored,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "kramdown",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::StripNonLetter,
        trim: Trim::None,
        case: Case::FullBeforeFilter,
        keep: Keep::AsciiAlphanumeric,
        separators: Separators::Space,
        runs: Runs::AsWritten,
        edges: Edges::AsWritten,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Fill("section"),
        duplicates: Duplicates::Dash,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "asciidoctor",
        typography: Typography::Plain,
        normalize: Normalize::None,
        fold: Fold::None,
        head: Head::AsWritten,
        trim: Trim::Before,
        case: Case::SimpleAfterFilter,
        keep: Keep::AsciidoctorId,
        separators: Separators::Space,
        runs: Runs::Collapse,
        edges: Edges::TrimEnd,
        leading_digit_prefix: None,
        separator: '_',
        prefix: Some("_"),
        empty: Empty::Drop,
        duplicates: Duplicates::UnderscoreFromTwo,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
    AnchorRule {
        name: "docutils",
        typography: Typography::Plain,
        normalize: Normalize::Nfkd,
        fold: Fold::AsciiIgnore,
        head: Head::StripNonLetter,
        trim: Trim::Before,
        case: Case::SimpleAfterFilter,
        keep: Keep::AsciiAlphanumeric,
        separators: Separators::NonAlphanumeric,
        runs: Runs::Collapse,
        edges: Edges::Trim,
        leading_digit_prefix: None,
        separator: '-',
        prefix: None,
        empty: Empty::Drop,
        duplicates: Duplicates::Dash,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Ignored,
        terms: Terms::Ignored,
    },
];

/// One spelling a document or its generator declares rather than a renderer
/// deriving it from heading text: the spelling an author uses, the profiles
/// that read it, and the file whose presence on the document's ancestor chain
/// turns it on. A rule declared by nothing is read in every tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclarationRule {
    pub name: &'static str,
    pub spelling: &'static str,
    pub adapters: &'static [Adapter],
    pub declared_by: &'static [&'static str],
}

/// The snippet syntax belongs to a mkdocs extension, so it is read only under
/// the file that declares mkdocs; without one the line is ordinary text.
pub(crate) const MKDOCS_SNIPPET: DeclarationRule = DeclarationRule {
    name: "mkdocs-snippet",
    spelling: "a `--8<--` line naming a quoted path, alone on the line",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::MKDOCS.declared_by,
};

/// The instruction a documentation generator answers, which stands for output
/// this engine cannot reproduce; the identities it publishes are outside the
/// tree. It is read under the same declaration the snippet line is.
const MKDOCS_DIRECTIVE: DeclarationRule = DeclarationRule {
    name: "mkdocs-directive",
    spelling: "a `:::` line naming what a generator renders, alone on the line",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::MKDOCS.declared_by,
};

/// The shortcode a hook the site declares expands before the page is
/// rendered. What arrives in the comment's place is a program's output, so it
/// is read under the same declaration the snippet line is.
const MKDOCS_SHORTCODE: DeclarationRule = DeclarationRule {
    name: "mkdocs-shortcode",
    spelling: "an HTML comment naming a hook's shortcode, `<!-- md:name -->`",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::MKDOCS.declared_by,
};

/// The tab whose identity the tabbed extension slugs under the settings the
/// site writes down, which is why it is read under the same declaration.
const MKDOCS_CONTENT_TAB: DeclarationRule = DeclarationRule {
    name: "mkdocs-content-tab",
    spelling: "a content tab opening a quoted title, `=== \"Title\"`",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::MKDOCS.declared_by,
};

/// The call a Hugo template answers before the page is rendered. The template
/// lives outside the tree, so what arrives in the call's place cannot be
/// enumerated here, and the spelling is read only under the file that declares
/// Hugo.
pub(crate) const HUGO_SHORTCODE: DeclarationRule = DeclarationRule {
    name: "hugo-shortcode",
    spelling: "a shortcode call alone on its line or anywhere in a heading, `{{% name %}}` or \
               `{{< name >}}`",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::HUGO.declared_by,
};

/// The Liquid an Eleventy page is rendered through before Markdown reads it,
/// read the way a shortcode in a heading is and only under the file that
/// declares Eleventy.
pub(crate) const ELEVENTY_TEMPLATE: DeclarationRule = DeclarationRule {
    name: "eleventy-template",
    spelling: "a Liquid tag or output anywhere in a heading, `{% name %}` or `{{ name }}`",
    adapters: &[Adapter::Markdown],
    declared_by: crate::route::ELEVENTY.declared_by,
};

const SPHINX_DECLARED_BY: &[&str] = crate::route::SPHINX.declared_by;

/// The label a plain link names. Sphinx keeps every name a page declares as a
/// global label, so a `MyST` link writes one where a path goes or as a bare
/// fragment, and neither spelling is a path in any other tree.
pub(crate) const MYST_LINK: DeclarationRule = DeclarationRule {
    name: "myst-link",
    spelling: "a plain link naming a label, `[text](name)` or `[text](#name)`",
    adapters: &[Adapter::Markdown],
    declared_by: SPHINX_DECLARED_BY,
};

/// Every way a document names its own identities rather than leaving them to a
/// renderer's slug, plus the spellings a declared generator owns, grouped
/// by the profile that reads each one. An identity rule joins the union beside
/// the renderer rules, so it can only grow the set an anchor may match.
pub const DECLARATIONS: [DeclarationRule; 24] = [
    DeclarationRule {
        name: "html-id",
        spelling: "an `id` or `name` attribute on a raw HTML element, or on one written \
                   inside an `mdx-code-block` fence",
        adapters: &[Adapter::Markdown, Adapter::Mdx],
        declared_by: &[],
    },
    DeclarationRule {
        name: "attr-list",
        spelling: "an attribute block alone on a block's first or last line, `{#id}`",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    DeclarationRule {
        name: "attr-list-inline",
        spelling: "an attribute block directly after an inline construct or a bracketed span, \
                   `**text**{#id}` or `[text]{#id}`",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    DeclarationRule {
        name: "definition-term",
        spelling: "a term line above a `: ` definition line",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    MKDOCS_SNIPPET,
    MKDOCS_DIRECTIVE,
    MKDOCS_SHORTCODE,
    MKDOCS_CONTENT_TAB,
    HUGO_SHORTCODE,
    ELEVENTY_TEMPLATE,
    DeclarationRule {
        name: "myst-target",
        spelling: "a target alone on its line, `(name)=`",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    DeclarationRule {
        name: "myst-directive-name",
        spelling: "a directive's `:name:` option, or a `figure-md` opener's argument",
        adapters: &[Adapter::Markdown, Adapter::Rst],
        declared_by: &[],
    },
    DeclarationRule {
        name: "myst-glossary",
        spelling: "a term of a definition list opening with `{.glossary}`",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    DeclarationRule {
        name: "myst-domain-object",
        spelling: "a `domain:type` directive's object name, `{py:class} widgets.Widget`",
        adapters: &[Adapter::Markdown],
        declared_by: &[],
    },
    DeclarationRule {
        name: "myst-role",
        spelling: "a cross-reference role, `` {doc}`name` ``",
        adapters: &[Adapter::Markdown],
        declared_by: SPHINX_DECLARED_BY,
    },
    MYST_LINK,
    DeclarationRule {
        name: "mdx-comment",
        spelling: "an MDX comment ending a heading, `{/* #id */}`",
        adapters: &[Adapter::Mdx],
        declared_by: &[],
    },
    DeclarationRule {
        name: "mdx-heading-id",
        spelling: "an MDX expression ending a heading, `{#id}`",
        adapters: &[Adapter::Mdx],
        declared_by: &[],
    },
    DeclarationRule {
        name: "jsx-id",
        spelling: "an `id` attribute on a lowercase JSX element",
        adapters: &[Adapter::Mdx],
        declared_by: &[],
    },
    DeclarationRule {
        name: "mdx-partial",
        spelling: "a default import of a relative document, rendered as an element",
        adapters: &[Adapter::Mdx],
        declared_by: &[],
    },
    DeclarationRule {
        name: "asciidoc-anchor",
        spelling: "a block anchor or attribute list alone on its line, `[[id]]`, `[#id]`, `[source#id]` or `[id=name]`",
        adapters: &[Adapter::AsciiDoc],
        declared_by: &[],
    },
    DeclarationRule {
        name: "asciidoc-inline-anchor",
        spelling: "an anchor in the flow of a line or a section title, `[[id]]`, `[[[bib]]]` or `anchor:id[]`",
        adapters: &[Adapter::AsciiDoc],
        declared_by: &[],
    },
    DeclarationRule {
        name: "asciidoc-reference-text",
        spelling: "a section title, or the reference text an anchor or `reftext` gives, that a natural cross reference names",
        adapters: &[Adapter::AsciiDoc],
        declared_by: &[],
    },
    DeclarationRule {
        name: "rst-target",
        spelling: "an internal hyperlink target, `.. _name:`",
        adapters: &[Adapter::Rst],
        declared_by: &[],
    },
];
