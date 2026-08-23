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
/// which is what keeps a nested block from ending its parent early.
#[must_use]
pub fn blocks(text: &str) -> Vec<Block> {
    let mut found: Vec<Block> = Vec::new();
    let mut open: Vec<(String, Delimiter, usize)> = Vec::new();
    let mut paragraph: Option<(usize, bool, usize)> = None;
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

        if let Some((fence, delimiter)) = fence_of(line) {
            flush(&mut found, &mut paragraph, start);
            open.push((fence, delimiter, offset));
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut found, &mut paragraph, start);
            continue;
        }
        if paragraph.is_none() {
            paragraph = Some((start, is_list_item(line), open.len()));
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

fn flush(found: &mut Vec<Block>, paragraph: &mut Option<(usize, bool, usize)>, end: usize) {
    if let Some((start, list_item, depth)) = paragraph.take()
        && end > start
    {
        found.push(Block {
            span: (start, end),
            delimiter: None,
            depth,
            list_item,
        });
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
