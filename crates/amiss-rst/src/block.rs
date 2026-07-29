/// What a block holds. `Literal` is an indented literal block opened by `::`,
/// whose content is code. `Comment` is an explicit markup block this parser
/// declines to read into. `Directive` holds a directive's argument and options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Literal,
    Comment,
    Directive,
}

/// One block of a document: its byte span, what it holds, and the indent that
/// opened it, which is the only nesting reStructuredText has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub span: (usize, usize),
    pub kind: Kind,
    pub indent: usize,
}

/// Splits a document into blocks. Indentation is the whole structure here: an
/// explicit markup line opens a block that runs until the indent returns, and a
/// paragraph ending in `::` opens a literal block the same way.
#[must_use]
pub fn blocks(text: &str) -> Vec<Block> {
    let mut found: Vec<Block> = Vec::new();
    let mut open: Option<(usize, Kind, usize)> = None;
    let mut offset = 0_usize;
    let mut pending_literal = false;
    let mut paragraph: Option<(usize, usize)> = None;

    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset = offset.saturating_add(raw.len());
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let indent = line.len().saturating_sub(line.trim_start().len());
        let blank = line.trim().is_empty();

        if blank {
            flush(&mut found, &mut paragraph, start);
        }
        if let Some((body_start, kind, opened_at)) = open {
            if blank {
                continue;
            }
            if indent > opened_at {
                continue;
            }
            found.push(Block {
                span: (body_start, start),
                kind,
                indent: opened_at,
            });
            open = None;
        }

        if blank {
            continue;
        }
        if pending_literal && indent > 0 {
            open = Some((start, Kind::Literal, 0));
            pending_literal = false;
            continue;
        }
        pending_literal = false;

        let trimmed = line.trim_start();
        if trimmed.starts_with("..") {
            flush(&mut found, &mut paragraph, start);
        }
        if let Some(rest) = trimmed.strip_prefix(".. ") {
            let kind = if rest.starts_with('_') || rest.contains(":: ") || rest.ends_with("::") {
                Kind::Directive
            } else {
                Kind::Comment
            };
            open = Some((start, kind, indent));
            continue;
        }
        if trimmed == ".." {
            open = Some((start, Kind::Comment, indent));
            continue;
        }
        if trimmed.ends_with("::") {
            pending_literal = true;
        }
        if paragraph.is_none() {
            paragraph = Some((start, indent));
        }
    }
    flush(&mut found, &mut paragraph, offset);
    if let Some((body_start, kind, opened_at)) = open {
        found.push(Block {
            span: (body_start, offset),
            kind,
            indent: opened_at,
        });
    }
    found.sort_by_key(|block| block.span);
    found
}

fn flush(found: &mut Vec<Block>, paragraph: &mut Option<(usize, usize)>, end: usize) {
    if let Some((start, indent)) = paragraph.take()
        && end > start
    {
        found.push(Block {
            span: (start, end),
            kind: Kind::Text,
            indent,
        });
    }
}
