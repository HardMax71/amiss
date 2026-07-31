use crate::{Reference, ReferenceKind};

const PATH_DIRECTIVES: [(&str, ReferenceKind); 4] = [
    ("image::", ReferenceKind::Image),
    ("figure::", ReferenceKind::Image),
    ("include::", ReferenceKind::Include),
    ("literalinclude::", ReferenceKind::Include),
];

/// Reads one line's references. `at` is the line's byte offset in the document.
#[must_use]
pub fn references(line: &str, at: usize) -> Vec<Reference> {
    let mut found = Vec::new();
    let trimmed = line.trim_start();
    let lead = line.len().saturating_sub(trimmed.len());

    if let Some(rest) = trimmed.strip_prefix(".. ") {
        let after = lead.saturating_add(3);
        if let Some((name, kind)) = PATH_DIRECTIVES
            .iter()
            .find(|(name, _)| rest.starts_with(name))
            .copied()
        {
            let argument = rest.get(name.len()..).unwrap_or_default().trim();
            if !argument.is_empty() && !argument.contains(char::is_whitespace) {
                found.push(build(kind, argument, at, after, line.len()));
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
    roles(line, at, &mut found);
    inline(line, at, &mut found);
    found.sort_by_key(|reference| reference.span);
    found
}

const SPHINX_ROLES: [(&str, ReferenceKind); 2] = [
    (":doc:`", ReferenceKind::DocRole),
    (":ref:`", ReferenceKind::RefRole),
];

/// The two Sphinx roles, by name. A `title <target>` body carries its target
/// in the brackets; a bare body is the target itself.
fn roles(line: &str, at: usize, found: &mut Vec<Reference>) {
    for (opener, kind) in SPHINX_ROLES {
        let mut index = 0_usize;
        while let Some(hit) = line.get(index..).and_then(|tail| tail.find(opener)) {
            let start = index.saturating_add(hit);
            let body_at = start.saturating_add(opener.len());
            let Some(close) = line.get(body_at..).and_then(|tail| tail.find('`')) else {
                break;
            };
            let end = body_at.saturating_add(close);
            let body = line.get(body_at..end).unwrap_or_default();
            let target = body
                .rsplit_once('<')
                .and_then(|(_, tail)| tail.strip_suffix('>'))
                .unwrap_or(body)
                .trim();
            let phrase_allowed = matches!(kind, ReferenceKind::RefRole);
            let acceptable = !target.is_empty()
                && (phrase_allowed || !target.contains(char::is_whitespace))
                && !target.contains('`');
            if acceptable {
                found.push(build(kind, target, at, start, end.saturating_add(1)));
            }
            index = end.saturating_add(1);
        }
    }
}

/// `.. _name: target` names an external hyperlink target. A line with nothing
/// after the colon declares an internal label instead, which is an anchor.
fn named_target(rest: &str) -> Option<&str> {
    let body = rest.strip_prefix('_')?;
    let (_, target) = body.rsplit_once(": ")?;
    let target = target.trim();
    (!target.is_empty() && !target.contains(char::is_whitespace)).then_some(target)
}

/// The `:file:` option that `csv-table` and `raw` take.
fn file_option(trimmed: &str) -> Option<&str> {
    let value = trimmed.strip_prefix(":file:")?.trim();
    (!value.is_empty() && !value.contains(char::is_whitespace)).then_some(value)
}

/// `` `text <target>`_ `` carries its target inline. The trailing underscore is
/// what separates a hyperlink from an ordinary interpreted-text span.
fn inline(line: &str, at: usize, found: &mut Vec<Reference>) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while let Some(open) = line.get(index..).and_then(|tail| tail.find('`')) {
        let start = index.saturating_add(open);
        let body_at = start.saturating_add(1);
        let Some(close) = line.get(body_at..).and_then(|tail| tail.find('`')) else {
            return;
        };
        let end = body_at.saturating_add(close);
        if bytes.get(end.saturating_add(1)) != Some(&b'_') {
            index = end.saturating_add(1);
            continue;
        }
        if let Some(target) = line
            .get(body_at..end)
            .and_then(|body| body.rsplit_once('<'))
            .and_then(|(_, tail)| tail.strip_suffix('>'))
            .map(str::trim)
            .filter(|target| !target.is_empty() && !target.contains(char::is_whitespace))
        {
            found.push(build(
                ReferenceKind::InlineHyperlink,
                target,
                at,
                start,
                end.saturating_add(2),
            ));
        }
        index = end.saturating_add(2);
    }
}

fn build(kind: ReferenceKind, target: &str, at: usize, start: usize, end: usize) -> Reference {
    Reference {
        kind,
        target: target.to_owned(),
        span: (at.saturating_add(start), at.saturating_add(end)),
        block: 0,
        block_span: (0, 0),
    }
}

/// An internal label a document declares, which publishes an anchor identity.
/// A phrase label arrives backtick-quoted, `.. _`name`:`, and the quotes are
/// spelling, not name.
#[must_use]
pub fn target_definition(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(".. _")?;
    let label = rest.strip_suffix(':')?.trim();
    let label = label
        .strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .map_or(label, str::trim);
    (!label.is_empty()).then(|| label.to_owned())
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
