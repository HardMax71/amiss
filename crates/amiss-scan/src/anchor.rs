use std::collections::BTreeSet;

use amiss_md::{Heading, HeadingSource};
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

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
    pub empty: Empty,
    pub duplicates: Duplicates,
    pub attribute: Attribute,
    pub raw_html: RawHtml,
}

/// Every renderer rule the resolver knows. Adding one can only grow the set an
/// anchor may match, so the set is the union and a missing rule is the only
/// way to report a live anchor as absent.
pub const RULES: [AnchorRule; 10] = [
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
        empty: Empty::Fill("section"),
        duplicates: Duplicates::Dash,
        attribute: Attribute::Literal,
        raw_html: RawHtml::Ignored,
    },
];

/// Every identity the known renderers would publish for one document, plus the
/// anchors its raw HTML declares.
#[must_use]
pub fn anchor_set(headings: &[Heading], html_anchors: &[String]) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = html_anchors.iter().cloned().collect();
    for rule in &RULES {
        set.extend(identities(rule, headings));
    }
    set
}

/// The identities one rule publishes, in document order, with the headings it
/// publishes nothing for left out.
#[must_use]
pub fn identities(rule: &AnchorRule, headings: &[Heading]) -> Vec<String> {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::with_capacity(headings.len());
    for heading in headings {
        if heading.source == HeadingSource::RawHtml && rule.raw_html == RawHtml::Ignored {
            continue;
        }
        let base = match (&heading.attribute, rule.attribute) {
            (Some(attribute), Attribute::Honored) => attribute.id.clone(),
            (Some(attribute), Attribute::Literal) => {
                slug(rule, &format!("{}{}", heading.text, attribute.suffix))
            }
            (None, _) => slug(rule, &heading.text),
        };
        let Some(unique) = fill(rule, base, &mut taken) else {
            continue;
        };
        out.push(unique);
    }
    out
}

fn fill(rule: &AnchorRule, base: String, taken: &mut BTreeSet<String>) -> Option<String> {
    let base = if base.is_empty() {
        match rule.empty {
            Empty::Drop => return None,
            Empty::Keep => base,
            Empty::Fill(text) => text.to_owned(),
        }
    } else {
        base
    };
    let unique = match rule.duplicates {
        Duplicates::Collide => base,
        Duplicates::Dash => {
            let mut candidate = base.clone();
            let mut count = 0_u32;
            while taken.contains(&candidate) {
                count = count.saturating_add(1);
                candidate = format!("{base}-{count}");
            }
            candidate
        }
        Duplicates::Underscore => {
            let mut candidate = base;
            while taken.contains(&candidate) || candidate.is_empty() {
                candidate = bump(&candidate);
            }
            candidate
        }
    };
    taken.insert(unique.clone());
    Some(unique)
}

/// python-markdown rewrites `x_1` to `x_2` rather than appending again.
fn bump(candidate: &str) -> String {
    let digits: String = candidate
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect();
    let head = candidate.get(..candidate.len().saturating_sub(digits.len()));
    match (
        digits.chars().rev().collect::<String>().parse::<u64>(),
        head,
    ) {
        (Ok(count), Some(head)) if head.ends_with('_') && !digits.is_empty() => {
            let stem = head.get(..head.len().saturating_sub(1)).unwrap_or(head);
            format!("{stem}_{}", count.saturating_add(1))
        }
        _ => format!("{candidate}_1"),
    }
}

fn slug(rule: &AnchorRule, text: &str) -> String {
    let typography = match rule.typography {
        Typography::SmartPunctuation => smart(text),
        Typography::Plain => text.to_owned(),
    };
    let normalized = match rule.normalize {
        Normalize::None => typography,
        Normalize::Nfc => typography.nfc().collect(),
        Normalize::Nfkd => typography.nfkd().collect(),
    };
    let stripped = match rule.head {
        Head::StripNonLetter => normalized
            .trim_start_matches(|ch: char| !ch.is_ascii_alphabetic())
            .to_owned(),
        Head::AsWritten => normalized,
    };
    let trimmed = match rule.trim {
        Trim::Before => stripped.trim().to_owned(),
        Trim::None | Trim::AfterRemoval => stripped,
    };
    let folded: String = match rule.fold {
        Fold::None => trimmed,
        Fold::AsciiIgnore => trimmed.chars().filter(char::is_ascii).collect(),
        Fold::LatinMarks => trimmed
            .chars()
            .filter(|ch| !matches!(u32::from(*ch), 0x0300..=0x036F))
            .collect(),
    };
    let cased = match rule.case {
        Case::FullBeforeFilter => folded.to_lowercase(),
        Case::SimpleAfterFilter => folded,
    };

    let mut retained = String::with_capacity(cased.len());
    for ch in cased.chars() {
        if ch == '-' || is_separator(rule.separators, ch) {
            retained.push(ch);
        } else if is_kept(rule.keep, ch) {
            match rule.case {
                Case::FullBeforeFilter => retained.push(ch),
                Case::SimpleAfterFilter => retained.push(simple_lower(ch)),
            }
        }
    }
    let retained = match rule.trim {
        Trim::AfterRemoval => retained.trim().to_owned(),
        Trim::None | Trim::Before => retained,
    };

    let mut out = String::with_capacity(retained.len());
    for ch in retained.chars() {
        if ch == '-' || is_separator(rule.separators, ch) {
            if rule.runs == Runs::Collapse && out.ends_with('-') {
                continue;
            }
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    let out = match rule.edges {
        Edges::Trim => out.trim_matches('-').to_owned(),
        Edges::AsWritten => out,
    };
    match rule.leading_digit_prefix {
        Some(prefix) if out.starts_with(|ch: char| ch.is_ascii_digit()) => format!("{prefix}{out}"),
        Some(_) | None => out,
    }
}

/// The dash and ellipsis rewrites pulldown-cmark performs before mdBook reads
/// a heading. Its quote rewrites are unobservable here: every rule either drops
/// both spellings or treats both as separators.
fn smart(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['-', '.']) {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        let (replacement, width) = if tail.starts_with("---") {
            ("\u{2014}", 3)
        } else if tail.starts_with("--") {
            ("\u{2013}", 2)
        } else if tail.starts_with("...") {
            ("\u{2026}", 3)
        } else {
            ("", 0)
        };
        if width == 0 {
            let Some((first, remainder)) = split_first(tail) else {
                break;
            };
            out.push(first);
            rest = remainder;
            continue;
        }
        out.push_str(replacement);
        rest = tail.get(width..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

fn split_first(text: &str) -> Option<(char, &str)> {
    let first = text.chars().next()?;
    Some((first, text.get(first.len_utf8()..)?))
}

fn simple_lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn is_separator(separators: Separators, ch: char) -> bool {
    match separators {
        Separators::Space => ch == ' ',
        Separators::Whitespace => ch.is_whitespace(),
        Separators::WhitespaceUnderscore => ch.is_whitespace() || ch == '_',
        Separators::MditVuePunctuation => {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '~' | '`'
                        | '!'
                        | '@'
                        | '#'
                        | '$'
                        | '%'
                        | '^'
                        | '&'
                        | '*'
                        | '('
                        | ')'
                        | '_'
                        | '+'
                        | '='
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '|'
                        | '\\'
                        | ';'
                        | ':'
                        | '"'
                        | '\''
                        | '\u{201C}'
                        | '\u{201D}'
                        | '\u{2018}'
                        | '\u{2019}'
                        | '<'
                        | '>'
                        | ','
                        | '.'
                        | '?'
                        | '/'
                )
        }
    }
}

const LETTERS: [GeneralCategory; 5] = [
    GeneralCategory::LowercaseLetter,
    GeneralCategory::ModifierLetter,
    GeneralCategory::OtherLetter,
    GeneralCategory::TitlecaseLetter,
    GeneralCategory::UppercaseLetter,
];

const MARKS: [GeneralCategory; 3] = [
    GeneralCategory::EnclosingMark,
    GeneralCategory::NonspacingMark,
    GeneralCategory::SpacingMark,
];

const NUMBERS: [GeneralCategory; 3] = [
    GeneralCategory::DecimalNumber,
    GeneralCategory::LetterNumber,
    GeneralCategory::OtherNumber,
];

fn is_kept(keep: Keep, ch: char) -> bool {
    let category = get_general_category(ch);
    let word = LETTERS.contains(&category) || NUMBERS.contains(&category);
    match keep {
        Keep::LetterMarkNumberConnector => {
            word || MARKS.contains(&category) || category == GeneralCategory::ConnectorPunctuation
        }
        Keep::LetterNumberUnderscore => word || ch == '_',
        Keep::AlphabeticNumericUnderscore => ch.is_alphanumeric() || ch == '_',
        Keep::AsciiAlphanumeric => ch.is_ascii_alphanumeric(),
        Keep::AnythingButC0 => !matches!(u32::from(ch), 0x0000..=0x001F),
    }
}
