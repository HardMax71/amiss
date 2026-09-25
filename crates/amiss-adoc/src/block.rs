use crate::{Block, Delimiter};

const FENCES: [(char, Delimiter); 8] = [
    ('-', Delimiter::Verbatim),
    ('.', Delimiter::Verbatim),
    ('+', Delimiter::Passthrough),
    ('/', Delimiter::Comment),
    ('=', Delimiter::Compound),
    ('*', Delimiter::Compound),
    ('_', Delimiter::Compound),
    ('|', Delimiter::Compound),
];

/// Splits a document into blocks. A delimiter line is four or more repeats of
/// one fence character and nothing else, and it closes on the identical line,
/// which is what keeps a nested block from ending its parent early. A
/// paragraph whose first line is indented and carries no list marker is a
/// literal paragraph, which reads verbatim like a delimited literal block, and
/// one under an attribute line naming a verbatim, passthrough or comment style
/// reads the way that style's delimited block would, blank lines between them
/// or not.
#[must_use]
pub fn blocks(text: &str) -> Vec<Block> {
    let mut found: Vec<Block> = Vec::new();
    let mut open: Vec<(String, Delimiter, usize)> = Vec::new();
    let mut paragraph: Option<Block> = None;
    let mut styled: Option<Delimiter> = None;
    let mut offset = 0_usize;

    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset = offset.saturating_add(raw.len());
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);

        if let Some((fence, delimiter, body_start)) = open.last().cloned() {
            if line == fence {
                let popped = open.pop().is_some();
                debug_assert!(popped);
                found.push(Block {
                    span: (body_start, start),
                    delimiter: Some(delimiter),
                    depth: open.len().saturating_add(1),
                    list_item: false,
                });
                continue;
            }
            if delimiter != Delimiter::Compound {
                continue;
            }
        }

        if paragraph.as_ref().is_some_and(|block| {
            setext_level(text.get(block.span.0..start).unwrap_or_default(), line).is_some()
        }) {
            continue;
        }
        if let Some((fence, delimiter)) = fence_of(line) {
            styled = None;
            flush(&mut found, &mut paragraph, start);
            open.push((fence, delimiter, offset));
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut found, &mut paragraph, start);
            continue;
        }
        if paragraph.is_none()
            && let Some(delimiter) = paragraph_style(line)
        {
            found.push(Block {
                span: (start, offset),
                delimiter: None,
                depth: open.len(),
                list_item: false,
            });
            styled = Some(delimiter);
            continue;
        }
        if paragraph.is_none() {
            let list_item = is_list_item(line);
            paragraph = Some(Block {
                span: (start, start),
                delimiter: styled.take().or_else(|| {
                    (!list_item && line.starts_with([' ', '\t'])).then_some(Delimiter::Verbatim)
                }),
                depth: open.len(),
                list_item,
            });
        }
    }
    flush(&mut found, &mut paragraph, offset);
    for (_, delimiter, body_start) in open {
        found.push(Block {
            span: (body_start, offset),
            delimiter: Some(delimiter),
            depth: 1,
            list_item: false,
        });
    }
    found.sort_by_key(|block| block.span);
    found
}

/// The level a two-line section title declares, Asciidoctor's own rule: one
/// line that opens with neither a space nor a dot and carries no list marker,
/// over a run of `=`, `-`, `~`, `^` or `+` within one character of its length.
#[must_use]
pub fn setext_level(title: &str, underline: &str) -> Option<usize> {
    let title = title.strip_suffix('\n').unwrap_or(title);
    let title = title.strip_suffix('\r').unwrap_or(title);
    let underline = underline.trim_end();
    let marker = underline.chars().next()?;
    let level = ['=', '-', '~', '^', '+']
        .iter()
        .position(|mark| *mark == marker)?
        .saturating_add(1);
    let length = underline.chars().count();
    (length >= 2
        && underline.chars().all(|character| character == marker)
        && !title.contains('\n')
        && !title.trim().is_empty()
        && !title.starts_with([' ', '\t', '.'])
        && !is_list_item(title)
        && title.chars().count().abs_diff(length) < 2)
        .then_some(level)
}

/// What an attribute line standing above a paragraph makes of it, read from
/// the style its first positional attribute names ahead of any `#id`, `.role`
/// or `%option` shorthand: source, listing and literal read verbatim, pass
/// and the math styles pass through, and comment drops the paragraph.
fn paragraph_style(line: &str) -> Option<Delimiter> {
    let inside = line.trim_end().strip_prefix('[')?.strip_suffix(']')?;
    let style = inside
        .split([',', '#', '.', '%'])
        .next()
        .unwrap_or_default()
        .trim();
    match style {
        "source" | "listing" | "literal" => Some(Delimiter::Verbatim),
        "pass" | "stem" | "latexmath" | "asciimath" => Some(Delimiter::Passthrough),
        "comment" => Some(Delimiter::Comment),
        _ => None,
    }
}

fn flush(found: &mut Vec<Block>, paragraph: &mut Option<Block>, end: usize) {
    if let Some(mut block) = paragraph.take()
        && end > block.span.0
    {
        block.span.1 = end;
        found.push(block);
    }
}

fn fence_of(line: &str) -> Option<(String, Delimiter)> {
    let trimmed = line.trim_end();
    let mut characters = trimmed.chars();
    let first = characters.next()?;
    let (_, delimiter) = FENCES.iter().find(|(fence, _)| *fence == first)?;
    if first == '|' {
        return (trimmed == "|===").then(|| (trimmed.to_owned(), *delimiter));
    }
    if trimmed.len() < 4 || !trimmed.chars().all(|byte| byte == first) {
        return None;
    }
    Some((trimmed.to_owned(), *delimiter))
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut characters = trimmed.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !matches!(first, '*' | '-' | '.') {
        return false;
    }
    let rest = trimmed.trim_start_matches(first);
    rest.starts_with(' ') && rest.len() < trimmed.len()
}
