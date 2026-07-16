use crate::lines::scan;

/// A recognized region may not exceed this exclusive frontmatter-relative
/// offset. Equality is accepted; one byte more is not a region at all.
pub const MAX_BYTES: usize = 65_536;

const BOM: [u8; 3] = [0xef, 0xbb, 0xbf];

/// A recognized `frontmatter` region. `bytes` counts the region itself
/// (both delimiters and every line ending inside it) and never the BOM, so the
/// raw document offset the parser resumes at is `bom_bytes + bytes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub bom_bytes: usize,
    pub bytes: usize,
    pub suffix_offset: usize,
    pub suffix_line: usize,
}

/// Recognizes a region only at byte offset zero, optionally after one UTF-8
/// BOM. The first complete line must be exactly `---` or `+++`; the closing
/// line must repeat it, where `---` also permits `...`. An opener with no
/// permitted closer, or a region past `MAX_BYTES`, is ordinary Markdown.
#[must_use]
pub fn recognize(source: &[u8]) -> Option<Region> {
    let bom_bytes = if source.starts_with(&BOM) {
        BOM.len()
    } else {
        0
    };
    let body = source.get(bom_bytes..)?;
    let mut lines = scan(body);
    let opener = lines.next()?;
    if !opener.terminated {
        return None;
    }
    let closers: &[&[u8]] = match opener.content(body) {
        b"---" => &[b"---", b"..."],
        b"+++" => &[b"+++"],
        _ => return None,
    };
    let mut consumed: usize = 1;
    for line in lines {
        consumed = consumed.saturating_add(1);
        if closers.contains(&line.content(body)) {
            if line.end > MAX_BYTES {
                return None;
            }
            return Some(Region {
                bom_bytes,
                bytes: line.end,
                suffix_offset: bom_bytes.saturating_add(line.end),
                suffix_line: consumed,
            });
        }
    }
    None
}
