use std::collections::HashSet;

use super::{Piece, escaped, folded, snapped};

/// The full and collapsed references in one text run whose label no
/// definition declares, `[text][label]` and `[label][]` or their image forms,
/// each with its label. Markdown leaves one as the text it spells, so it is a
/// link that broke rather than prose. A pair glued to what surrounds it,
/// `a[i][j]` or `[A-Z][a-z]*`, is an index or a pattern, and an escaped
/// opener is text.
pub(super) fn undefined_references(
    suffix: &str,
    span: (usize, usize),
    pieces: &[Piece],
    defined: &HashSet<String>,
) -> Vec<(usize, usize, String, bool)> {
    let Some(run) = suffix.get(span.0..span.1) else {
        return Vec::new();
    };
    let bracket = |from: usize| {
        run.get(from..)
            .and_then(|rest| rest.find(['[', ']']))
            .map(|offset| from.saturating_add(offset))
            .filter(|at| run.as_bytes().get(*at) == Some(&b']'))
    };
    let mut found = Vec::new();
    let mut at = 0_usize;
    while let Some(open) = run
        .get(at..)
        .and_then(|rest| rest.find('['))
        .map(|offset| at.saturating_add(offset))
    {
        at = open.saturating_add(1);
        let Some(text_end) = bracket(open.saturating_add(1)) else {
            continue;
        };
        if run.as_bytes().get(text_end.saturating_add(1)) != Some(&b'[') {
            continue;
        }
        let Some(label_end) = bracket(text_end.saturating_add(2)) else {
            continue;
        };
        let text = run
            .get(open.saturating_add(1)..text_end)
            .unwrap_or_default();
        let written = run
            .get(text_end.saturating_add(2)..label_end)
            .unwrap_or_default();
        let label = if written.is_empty() { text } else { written };
        let start = span.0.saturating_add(open);
        let end = span.0.saturating_add(label_end).saturating_add(1);
        let before = start
            .checked_sub(1)
            .and_then(|at| suffix.as_bytes().get(at));
        let after = suffix.as_bytes().get(end);
        let image = before == Some(&b'!');
        let glued = before.is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b']' | b')' | b'`')
        }) || after.is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'+' | b'?' | b'{' | b'(' | b'[')
        });
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let first = if image {
            start.saturating_sub(1)
        } else {
            start
        };
        if glued
            || text.trim().is_empty()
            || label.is_empty()
            || label.len() > 999
            || label.starts_with('^')
            || escaped(suffix, pieces, first)
            || snapped(pieces, first, end) != Some((first, end))
            || defined.contains(&folded(&label))
        {
            continue;
        }
        found.push((first, end, label, image));
        at = label_end.saturating_add(1);
    }
    found
}
