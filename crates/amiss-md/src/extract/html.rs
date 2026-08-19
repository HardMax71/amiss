use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::{Heading, HeadingSource};

pub(super) struct HtmlDestination {
    pub(super) within: usize,
    pub(super) construct: SourceConstruct,
    pub(super) raw_destination: String,
    pub(super) semantic_destination: String,
    pub(super) span: (usize, usize),
}

/// `href` and `src` are read out of a raw-HTML node the way any destination
/// is, references decoded; comments and raw-text bodies stay the blind spot.
pub(super) fn destinations(start: usize, region: &[u8], out: &mut Vec<HtmlDestination>) {
    let mut within = 0_usize;
    walk_region(region, |at| {
        if let Some(end) = opaque_text_end(region, at) {
            return Some(end);
        }
        let Some((construct, attribute)) = destination_open_at(region, at) else {
            return foreign_tag_end(region, at);
        };
        let end = tag_close(region, at)?;
        let value = unquoted(region, at, |inner, _byte| {
            if inner >= end {
                return Some(None);
            }
            attribute_name_at(region, inner, attribute)
                .then(|| attribute_value(region, inner.saturating_add(attribute.len())))
                .flatten()
                .map(|(value, _next)| Some(value))
        });
        if let Some(Some(raw_destination)) = value {
            let ordinal = within;
            within = within.saturating_add(1);
            if let Some(semantic_destination) = decoded(&raw_destination) {
                out.push(HtmlDestination {
                    within: ordinal,
                    construct,
                    raw_destination,
                    semantic_destination,
                    span: (start.saturating_add(at), start.saturating_add(end)),
                });
            }
        }
        Some(end)
    });
}

/// Every `id` and `name` attribute value inside the raw-HTML regions, in
/// document order. Accepting more than a browser would can only leave an
/// anchor unreported, never report a live one as missing.
pub(super) fn anchors(_start: usize, region: &[u8], out: &mut Vec<String>) {
    walk_region(region, |at| {
        let name = ["id", "name"]
            .into_iter()
            .find(|name| attribute_name_at(region, at, name.as_bytes()))?;
        let after = at.saturating_add(name.len());
        let Some((value, next)) = attribute_value(region, after) else {
            return Some(after);
        };
        out.push(value);
        Some(next)
    });
}

/// Every `h1` through `h6` element written inside the raw-HTML regions, with
/// the text content its renderer would read. An element whose closing tag is
/// missing from its own region is left out.
pub(super) fn headings(start: usize, region: &[u8], out: &mut Vec<Heading>) {
    out.extend(Headings {
        start,
        region,
        cursor: 0,
        unclosed: [false; 6],
    });
}

struct Headings<'a> {
    start: usize,
    region: &'a [u8],
    cursor: usize,
    unclosed: [bool; 6],
}

impl Iterator for Headings<'_> {
    type Item = Heading;

    fn next(&mut self) -> Option<Self::Item> {
        // One failed search proves the level has no closer left, so a region of
        // openers costs six scans rather than one per opener.
        while let Some(at) = scan(self.region, self.cursor, |at| {
            heading_open_at(self.region, at).is_some()
        }) {
            let Some(level) = heading_open_at(self.region, at) else {
                self.cursor = at.saturating_add(1);
                continue;
            };
            let depth = usize::from(level.saturating_sub(b'1'));
            let Some(open_end) = tag_end(self.region, at) else {
                self.cursor = self.region.len();
                return None;
            };
            if self.unclosed.get(depth) == Some(&true) {
                self.cursor = open_end;
                continue;
            }
            let Some(close) = closing_tag(self.region, open_end, level) else {
                if let Some(flag) = self.unclosed.get_mut(depth) {
                    *flag = true;
                }
                self.cursor = open_end;
                continue;
            };
            self.cursor = close;
            let inner = self
                .region
                .get(open_end..close)
                .and_then(|raw| core::str::from_utf8(raw).ok());
            if let Some(inner) = inner {
                return Some(Heading {
                    text: strip_markup(inner),
                    attribute: None,
                    source: HeadingSource::RawHtml,
                    span: (
                        self.start.saturating_add(at),
                        self.start.saturating_add(
                            tag_end(self.region, close).unwrap_or(self.region.len()),
                        ),
                    ),
                });
            }
        }
        None
    }
}

pub(super) fn collect_regions<T>(
    suffix: &str,
    regions: &[(usize, usize)],
    mut scan: impl FnMut(usize, &[u8], &mut Vec<T>),
) -> Vec<T> {
    let mut out = Vec::new();
    for (start, region) in slices(suffix, regions) {
        scan(start, region, &mut out);
    }
    out
}

/// Every position in one region, advancing by whatever the step recognized or
/// by one byte when it recognized nothing.
fn walk_region(region: &[u8], mut step: impl FnMut(usize) -> Option<usize>) {
    let mut at = 0_usize;
    while at < region.len() {
        at = step(at).unwrap_or_else(|| at.saturating_add(1));
    }
}

fn destination_open_at(region: &[u8], at: usize) -> Option<(SourceConstruct, &'static [u8])> {
    if region.get(at) != Some(&b'<') {
        return None;
    }
    for (name, construct, attribute) in [
        (
            b"a".as_slice(),
            SourceConstruct::HtmlAnchor,
            b"href".as_slice(),
        ),
        (
            b"img".as_slice(),
            SourceConstruct::HtmlImage,
            b"src".as_slice(),
        ),
    ] {
        let after = region.get(at.saturating_add(1).saturating_add(name.len()));
        let opens = region
            .get(at.saturating_add(1)..at.saturating_add(1).saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && after
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/');
        if opens {
            return Some((construct, attribute));
        }
    }
    None
}

const RAW_TEXT_ELEMENTS: [&[u8]; 4] = [b"script", b"style", b"textarea", b"title"];

/// A comment or raw-text element body: no renderer follows a tag spelled
/// inside one, so the miner steps over the whole span.
fn opaque_text_end(region: &[u8], at: usize) -> Option<usize> {
    if region.get(at) != Some(&b'<') {
        return None;
    }
    if region.get(at..at.saturating_add(4)) == Some(b"<!--") {
        return Some(comment_end(region, at));
    }
    let name = RAW_TEXT_ELEMENTS.into_iter().find(|name| {
        let start = at.saturating_add(1);
        region
            .get(start..start.saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && region
                .get(start.saturating_add(name.len()))
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
    })?;
    Some(raw_text_end(region, at, name))
}

/// Any other tag is consumed whole, so a raw-text opener spelled inside its
/// quoted attribute values is never mistaken for markup.
fn foreign_tag_end(region: &[u8], at: usize) -> Option<usize> {
    let named = matches!(region.get(at), Some(&b'<'))
        && region
            .get(at.saturating_add(1))
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'/');
    if named { tag_close(region, at) } else { None }
}

fn comment_end(region: &[u8], from: usize) -> usize {
    let start = from.saturating_add(4);
    region
        .windows(3)
        .skip(start)
        .position(|window| window == b"-->")
        .map_or(region.len(), |offset| {
            start.saturating_add(offset).saturating_add(3)
        })
}

fn raw_text_end(region: &[u8], from: usize, name: &[u8]) -> usize {
    let mut at = from.saturating_add(1);
    while at < region.len() {
        let after = at.saturating_add(2).saturating_add(name.len());
        let closes = region.get(at..at.saturating_add(2)) == Some(b"</")
            && region
                .get(at.saturating_add(2)..after)
                .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && region
                .get(after)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>');
        if closes {
            return tag_end(region, at).unwrap_or(region.len());
        }
        at = at.saturating_add(1);
    }
    region.len()
}

/// Every position outside quoted attribute values, visited in order until the
/// visitor answers; quoted spans are stepped over whole.
fn unquoted<T>(
    region: &[u8],
    from: usize,
    mut visit: impl FnMut(usize, u8) -> Option<T>,
) -> Option<T> {
    let mut quote: Option<u8> = None;
    let mut at = from;
    while let Some(byte) = region.get(at).copied() {
        if let Some(mark) = quote {
            if byte == mark {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if let Some(result) = visit(at, byte) {
            return Some(result);
        }
        at = at.saturating_add(1);
    }
    None
}

/// The `>` that ends the tag; an unclosed one yields nothing rather than a
/// truncated span.
fn tag_close(region: &[u8], from: usize) -> Option<usize> {
    unquoted(region, from, |at, byte| {
        (byte == b'>').then(|| at.saturating_add(1))
    })
}

fn heading_open_at(region: &[u8], at: usize) -> Option<u8> {
    if region.get(at) != Some(&b'<')
        || !matches!(region.get(at.saturating_add(1)), Some(b'h' | b'H'))
    {
        return None;
    }
    let level = *region.get(at.saturating_add(2))?;
    let after = *region.get(at.saturating_add(3))?;
    ((b'1'..=b'6').contains(&level)
        && (after.is_ascii_whitespace() || after == b'>' || after == b'/'))
        .then_some(level)
}

fn slices<'a>(
    suffix: &'a str,
    regions: &'a [(usize, usize)],
) -> impl Iterator<Item = (usize, &'a [u8])> {
    regions
        .iter()
        .filter_map(|(start, end)| Some((*start, suffix.as_bytes().get(*start..*end)?)))
}

fn scan(region: &[u8], from: usize, hit: impl Fn(usize) -> bool) -> Option<usize> {
    (from..region.len()).find(|at| hit(*at))
}

fn tag_end(region: &[u8], from: usize) -> Option<usize> {
    scan(region, from, |at| region.get(at) == Some(&b'>')).map(|at| at.saturating_add(1))
}

fn closing_tag(region: &[u8], from: usize, level: u8) -> Option<usize> {
    scan(region, from, |at| {
        region.get(at) == Some(&b'<')
            && region.get(at.saturating_add(1)) == Some(&b'/')
            && matches!(region.get(at.saturating_add(2)), Some(b'h' | b'H'))
            && region.get(at.saturating_add(3)) == Some(&level)
    })
}

/// The text content a browser reads from one element's markup: nested tags and
/// comments contribute nothing, character references decode, and every other
/// byte survives exactly, including the whitespace a wrapped element carries.
fn strip_markup(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut rest = inner;
    while let Some(at) = rest.find(['<', '&']) {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        rest = if let Some(comment) = tail.strip_prefix("<!--") {
            comment
                .find("-->")
                .and_then(|end| comment.get(end.saturating_add(3)..))
                .unwrap_or_default()
        } else if tail.starts_with('<') {
            tail.find('>')
                .and_then(|end| tail.get(end.saturating_add(1)..))
                .unwrap_or_default()
        } else if let Some((decoded, next)) = reference(tail) {
            out.push(decoded);
            next
        } else {
            out.push('&');
            tail.get(1..).unwrap_or_default()
        };
    }
    out.push_str(rest);
    out
}

/// A destination's character references decoded, the format's own semantic
/// reading. A bare ampersand that forms no reference stays itself; a
/// reference-shaped run the table cannot decode yields nothing, so the
/// destination stays a blind spot rather than a half-decoded miss.
fn decoded(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        if let Some((symbol, next)) = reference(tail) {
            out.push(symbol);
            rest = next;
        } else if reference_shaped(tail) {
            return None;
        } else {
            out.push('&');
            rest = tail.get(1..).unwrap_or_default();
        }
    }
    out.push_str(rest);
    Some(out)
}

fn reference_shaped(tail: &str) -> bool {
    const LONGEST: usize = 32;
    tail.find(';')
        .filter(|end| *end <= LONGEST)
        .and_then(|end| tail.get(1..end))
        .is_some_and(|body| {
            !body.is_empty()
                && body
                    .chars()
                    .all(|symbol| symbol.is_ascii_alphanumeric() || symbol == '#')
        })
}

/// The named references HTML predefines, plus numeric ones. A run longer than
/// any of those spellings is text, not a reference.
fn reference(tail: &str) -> Option<(char, &str)> {
    const LONGEST: usize = 32;
    let end = tail.find(';').filter(|end| *end <= LONGEST)?;
    let body = tail.get(1..end)?;
    let next = tail.get(end.saturating_add(1)..)?;
    let decoded = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let digits = body.strip_prefix('#')?;
            let point = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(point)?
        }
    };
    Some((decoded, next))
}

fn attribute_name_at(region: &[u8], at: usize, name: &[u8]) -> bool {
    let before = at
        .checked_sub(1)
        .and_then(|index| region.get(index))
        .is_some_and(u8::is_ascii_whitespace);
    let after = region
        .get(at.saturating_add(name.len()))
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'=');
    before
        && after
        && region
            .get(at..at.saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
}

fn attribute_value(region: &[u8], from: usize) -> Option<(String, usize)> {
    let mut at = from;
    while region.get(at).is_some_and(u8::is_ascii_whitespace) {
        at = at.saturating_add(1);
    }
    if region.get(at) != Some(&b'=') {
        return None;
    }
    at = at.saturating_add(1);
    while region.get(at).is_some_and(u8::is_ascii_whitespace) {
        at = at.saturating_add(1);
    }
    let quote = match region.get(at).copied() {
        Some(mark @ (b'"' | b'\'')) => Some(mark),
        Some(_) | None => None,
    };
    let start = if quote.is_some() {
        at.saturating_add(1)
    } else {
        at
    };
    let mut end = start;
    while let Some(byte) = region.get(end) {
        let closes = quote.map_or_else(
            || byte.is_ascii_whitespace() || *byte == b'>',
            |mark| *byte == mark,
        );
        if closes {
            break;
        }
        end = end.saturating_add(1);
    }
    let value = region
        .get(start..end)
        .and_then(|raw| core::str::from_utf8(raw).ok())?;
    let next = if quote.is_some() {
        end.saturating_add(1)
    } else {
        end
    };
    (!value.is_empty()).then(|| (value.to_owned(), next))
}
