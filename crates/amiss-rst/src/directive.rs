use amiss_wire::extraction::TransclusionKind;

use crate::{Reference, ReferenceKind};

const PATH_DIRECTIVES: [(&str, ReferenceKind, Option<TransclusionKind>); 4] = [
    ("image::", ReferenceKind::Image, None),
    ("figure::", ReferenceKind::Image, None),
    (
        "include::",
        ReferenceKind::Include,
        Some(TransclusionKind::Parsed),
    ),
    (
        "literalinclude::",
        ReferenceKind::Include,
        Some(TransclusionKind::Literal),
    ),
];

/// Reads one line's references. `at` is the line's byte offset in the document.
#[must_use]
pub fn references(line: &str, at: usize) -> Vec<Reference> {
    let mut found = Vec::new();
    let trimmed = line.trim_start();
    let lead = line.len().saturating_sub(trimmed.len());

    if let Some(rest) = trimmed.strip_prefix(".. ") {
        let after = lead.saturating_add(3);
        if let Some((name, kind, transclusion)) = PATH_DIRECTIVES
            .iter()
            .find(|(name, _, _)| rest.starts_with(name))
            .copied()
        {
            let argument = rest.get(name.len()..).unwrap_or_default().trim();
            if !argument.is_empty()
                && !argument.contains(char::is_whitespace)
                && !argument.ends_with(".*")
            {
                let mut reference = build(kind, argument, at, after, line.len());
                reference.transclusion = transclusion.map(Ok);
                found.push(reference);
            }
            return found;
        }
        if let Some(target) = named_target(rest) {
            found.push(build(
                ReferenceKind::NamedTarget,
                target,
                at,
                after,
                line.len(),
            ));
            return found;
        }
    }
    if let Some(path) = file_option(trimmed) {
        found.push(build(ReferenceKind::FileOption, path, at, lead, line.len()));
        return found;
    }
    interpreted_text(line, at, &mut found);
    found.sort_by_key(|reference| reference.span);
    found
}

const SPHINX_ROLES: [(&str, ReferenceKind); 2] = [
    (":doc:`", ReferenceKind::DocRole),
    (":ref:`", ReferenceKind::RefRole),
];

struct RoleState {
    opener: &'static str,
    kind: ReferenceKind,
    open: Option<usize>,
}

/// `.. _name: target` names an external hyperlink target. A line with nothing
/// after the colon declares an internal label instead, which is an anchor.
fn named_target(rest: &str) -> Option<&str> {
    let body = rest.strip_prefix('_')?;
    let (_, target) = body.rsplit_once(": ")?;
    let target = target.trim();
    (!target.is_empty() && !target.contains(char::is_whitespace) && !indirect(target))
        .then_some(target)
}

/// A destination ending `_` names another target, docutils' indirect form,
/// which is an alias rather than anything a tree can answer.
fn indirect(target: &str) -> bool {
    target.ends_with('_')
}

/// The `:file:` option that `csv-table` and `raw` take.
fn file_option(trimmed: &str) -> Option<&str> {
    let value = trimmed.strip_prefix(":file:")?.trim();
    (!value.is_empty() && !value.contains(char::is_whitespace) && !value.starts_with('`'))
        .then_some(value)
}

/// Reads the two Sphinx roles and inline hyperlinks from one stream of
/// backticks. Each form keeps independent delimiter state because malformed
/// forms may overlap without changing how another form is recognized.
fn interpreted_text(line: &str, at: usize, found: &mut Vec<Reference>) {
    let bytes = line.as_bytes();
    let mut roles = SPHINX_ROLES.map(|(opener, kind)| RoleState {
        opener,
        kind,
        open: None,
    });
    let mut inline_open: Option<usize> = None;

    for (tick, _) in line.match_indices('`') {
        for role in &mut roles {
            if let Some(start) = role.open.take() {
                let body_at = start.saturating_add(role.opener.len());
                let body = line.get(body_at..tick).unwrap_or_default();
                let target = body
                    .rsplit_once('<')
                    .and_then(|(_, tail)| tail.strip_suffix('>'))
                    .unwrap_or(body)
                    .trim();
                let phrase_allowed = matches!(role.kind, ReferenceKind::RefRole);
                let acceptable = !target.is_empty()
                    && (phrase_allowed || !target.contains(char::is_whitespace))
                    && !target.contains('`');
                if acceptable {
                    found.push(build(role.kind, target, at, start, tick.saturating_add(1)));
                }
                continue;
            }

            let Some(start) = tick.checked_sub(role.opener.len().saturating_sub(1)) else {
                continue;
            };
            let prefixed = start
                .checked_sub(1)
                .and_then(|before| bytes.get(before))
                .is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'.' | b'-' | b':')
                });
            if !prefixed && line.get(start..tick.saturating_add(1)) == Some(role.opener) {
                role.open = Some(start);
            }
        }

        if let Some(start) = inline_open.take() {
            if bytes.get(tick.saturating_add(1)) == Some(&b'_')
                && let Some(target) = line
                    .get(start.saturating_add(1)..tick)
                    .and_then(|body| body.rsplit_once('<'))
                    .and_then(|(_, tail)| tail.strip_suffix('>'))
                    .map(str::trim)
                    .filter(|target| {
                        !target.is_empty()
                            && !target.contains(char::is_whitespace)
                            && !indirect(target)
                    })
            {
                found.push(build(
                    ReferenceKind::InlineHyperlink,
                    target,
                    at,
                    start,
                    tick.saturating_add(2),
                ));
            }
        } else {
            inline_open = Some(tick);
        }
    }
}

fn build(kind: ReferenceKind, target: &str, at: usize, start: usize, end: usize) -> Reference {
    Reference {
        kind,
        target: target.to_owned(),
        span: (at.saturating_add(start), at.saturating_add(end)),
        block: 0,
        block_span: (0, 0),
        transclusion: None,
    }
}

/// An internal label a document declares, which publishes an anchor identity.
/// A phrase label arrives backtick-quoted, `.. _`name`:`, and the quotes are
/// spelling, not name. Docutils allows a target wherever block content goes,
/// so a declaration also counts as the body of a list item or alone in a
/// grid-table cell.
#[must_use]
pub fn target_definition(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let unbulleted = ["* ", "- ", "+ "]
        .iter()
        .find_map(|marker| trimmed.strip_prefix(marker))
        .map_or(trimmed, str::trim_start);
    if let Some(label) = bare_target(unbulleted) {
        return Some(label);
    }
    if trimmed.starts_with('|') {
        return trimmed.split('|').find_map(|cell| bare_target(cell.trim()));
    }
    None
}

fn bare_target(text: &str) -> Option<String> {
    let rest = text.strip_prefix(".. _")?;
    let rest = rest.strip_suffix('\r').unwrap_or(rest);
    let label = rest.strip_suffix(':')?.trim();
    let label = label
        .strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .map_or(label, str::trim);
    (!label.is_empty() && !label.contains('|')).then(|| label.to_owned())
}

/// A section underline: a run of one punctuation character at least as long as
/// the title it follows.
#[must_use]
pub fn title_underline(line: &str, title: &str) -> Option<char> {
    let trimmed = line.trim_end();
    let first = trimmed.chars().next()?;
    if first.is_alphanumeric() || first.is_whitespace() || trimmed.chars().count() < 2 {
        return None;
    }
    let uniform = trimmed.chars().all(|character| character == first);
    let long_enough = trimmed.chars().count() >= title.trim_end().chars().count();
    (uniform && long_enough).then_some(first)
}
