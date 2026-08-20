mod identity;

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
}

/// Every renderer rule the resolver knows. Adding one can only grow the set an
/// anchor may match, so the set is the union and a missing rule is the only
/// way to report a live anchor as absent.
pub const RULES: [AnchorRule; 12] = [
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
    },
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
    },
];
