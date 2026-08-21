use std::collections::{BTreeSet, HashMap, HashSet};

use amiss_md::{Heading, HeadingSource};
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

use super::{
    AnchorRule, Attribute, Case, Duplicates, Edges, Empty, Fold, Head, Keep, Normalize, RULES,
    RawHtml, Runs, Separators, Trim, Typography,
};

/// Every identity the known renderers would publish for one document, plus the
/// anchors the document declares itself, in raw HTML or in an attribute block.
#[must_use]
pub fn anchor_set(
    headings: &[Heading],
    html_anchors: &[String],
    declared_anchors: &[String],
) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = html_anchors.iter().cloned().collect();
    set.extend(declared_anchors.iter().cloned());
    for rule in &RULES {
        set.extend(identities(rule, headings));
    }
    set
}

/// The identities one rule publishes, in document order, with the headings it
/// publishes nothing for left out.
#[must_use]
pub fn identities(rule: &AnchorRule, headings: &[Heading]) -> Vec<String> {
    let mut occupied = OccupiedIdentities {
        taken: HashSet::with_capacity(headings.len()),
        ..OccupiedIdentities::default()
    };
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
        let Some(unique) = fill(rule, base, &mut occupied) else {
            continue;
        };
        out.push(unique);
    }
    out
}

#[derive(Default)]
struct OccupiedIdentities {
    taken: HashSet<String>,
    numbered: HashMap<String, u32>,
    bumped: HashMap<String, String>,
}

fn fill(rule: &AnchorRule, base: String, occupied: &mut OccupiedIdentities) -> Option<String> {
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
        Duplicates::Dash => numbered_identity(base, '-', 1, occupied),
        Duplicates::UnderscoreFromTwo => numbered_identity(base, '_', 2, occupied),
        Duplicates::Underscore => bumped_identity(base, occupied),
    };
    occupied.taken.insert(unique.clone());
    Some(unique)
}

fn numbered_identity(
    base: String,
    separator: char,
    first: u32,
    occupied: &mut OccupiedIdentities,
) -> String {
    if !occupied.taken.contains(&base) {
        return base;
    }
    let mut count = occupied.numbered.get(&base).copied().unwrap_or(first);
    loop {
        let candidate = format!("{base}{separator}{count}");
        count = count.saturating_add(1);
        if !occupied.taken.contains(&candidate) {
            occupied.numbered.insert(base, count);
            return candidate;
        }
    }
}

fn bumped_identity(base: String, occupied: &mut OccupiedIdentities) -> String {
    if !base.is_empty() && !occupied.taken.contains(&base) {
        return base;
    }
    let mut candidate = occupied.bumped.remove(&base).unwrap_or_else(|| bump(&base));
    while occupied.taken.contains(&candidate) || candidate.is_empty() {
        candidate = bump(&candidate);
    }
    occupied.bumped.insert(base, bump(&candidate));
    candidate
}

/// python-markdown rewrites `x_1` to `x_2` rather than appending again.
fn bump(candidate: &str) -> String {
    let head = candidate.trim_end_matches(|ch: char| ch.is_ascii_digit());
    let digits = candidate.get(head.len()..).unwrap_or_default();
    match (head.strip_suffix('_'), digits.parse::<u64>()) {
        (Some(stem), Ok(count)) => {
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
        if ch == rule.separator || is_separator(rule.separators, ch) {
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
        if ch == rule.separator || is_separator(rule.separators, ch) {
            if rule.runs == Runs::Collapse && out.ends_with(rule.separator) {
                continue;
            }
            out.push(rule.separator);
        } else {
            out.push(ch);
        }
    }
    let out = match rule.edges {
        Edges::Trim => out.trim_matches(rule.separator).to_owned(),
        Edges::TrimEnd => out.trim_end_matches(rule.separator).to_owned(),
        Edges::AsWritten => out,
    };
    let digit_prefixed = match rule.leading_digit_prefix {
        Some(prefix) if out.starts_with(|ch: char| ch.is_ascii_digit()) => format!("{prefix}{out}"),
        Some(_) | None => out,
    };
    match rule.prefix {
        Some(prefix) if !digit_prefixed.is_empty() => format!("{prefix}{digit_prefixed}"),
        Some(_) | None => digit_prefixed,
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
        Separators::NonAlphanumeric => !ch.is_ascii_alphanumeric(),
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
        Keep::AsciidoctorId => ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.',
        Keep::AnythingButC0 => !matches!(u32::from(ch), 0x0000..=0x001F),
    }
}
