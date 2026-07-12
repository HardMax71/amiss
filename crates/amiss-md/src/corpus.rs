use amiss_wire::digest::hb;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::model::Adapter;
use amiss_wire::report::AnalysisErrorCode;

use crate::extract::{Extraction, Occurrence, analyze};

pub const SCHEMA: &str = "amiss/parser-profile-corpus/v1";

pub const COMMONMARK_FAMILY: &str = "commonmark-0.31.2";
pub const COMMONMARK_PIN: &str =
    "sha256:d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20";

pub const GFM_FAMILY: &str = "gfm-0.29";
pub const GFM_PIN: &str = "sha256:7d8e5814befec287ac116786d81ff14e0adc9b13295b4494649e995408fd871c";

pub const MDX_JSX_FAMILY: &str = "micromark-mdx-jsx-3.0.2";
pub const MDX_JSX_PIN: &str =
    "sha256:17df57441a015be02a333f78fb8aeddf0d93586019fc7c4ae665d00dab666c32";

pub const MDX_EXPRESSION_FAMILY: &str = "micromark-mdx-expression-3.0.1";
pub const MDX_EXPRESSION_PIN: &str =
    "sha256:2aaf8667378829192bf25674fed0edeccd759a7ce0b0c3eaf5625faeea364be6";

pub const MDX_ESM_FAMILY: &str = "micromark-mdxjs-esm-3.0.0";
pub const MDX_ESM_PIN: &str =
    "sha256:fdffc20bfaef4fcbdc6640a7fef9dfa6ec35715d455baeadd8a6c34e866a3151";

pub const FOOTNOTE_FAMILY: &str = "micromark-gfm-footnote-2.1.0";
pub const FOOTNOTE_PIN: &str =
    "sha256:41a437756e5c4615dfe9269acb23acbc74d8b01d9f7cabb4f121e8ca7e5d1a18";

pub const STRIKETHROUGH_FAMILY: &str = "micromark-gfm-strikethrough-2.1.0";
pub const STRIKETHROUGH_PIN: &str =
    "sha256:b7bdf617e8535348265bb8d91f0c7da65b7849e150460a44b063b22640e5178b";

/// The footnote suite also drives a directory of documents against the HTML
/// github.com itself renders for them. That directory is pinned whole, by one
/// digest over the canonical JSON of every file in it.
pub const GITHUB_FOOTNOTE_FAMILY: &str = "github-gfm-footnote-2.1.0";
pub const GITHUB_FOOTNOTE_PIN: &str =
    "sha256:24829d3c8c494684d63bd3d613578504371f0da8b8ef1a6bbae5a7093fa27e1a";

/// Every case is charged under every profile, so a grammar change anywhere
/// moves the manifest. The manifest names what it covers, so a reader never
/// mistakes a partial corpus for a complete one.
pub const PROFILES: [Adapter; 3] = [Adapter::Markdown, Adapter::Mdx, Adapter::PlainAdvisory];

/// What upstream says about a case: the HTML it publishes for the example, or
/// the message it rejects the example with, or nothing beyond acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    Html(String),
    Accepted,
    Rejected(String),
}

/// One executable example. `tag` carries the GFM extension marker, where
/// `disabled` means upstream does not execute the example.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub family: &'static str,
    pub number: usize,
    pub section: String,
    pub tag: Option<String>,
    pub source: String,
    pub expect: Expect,
    pub config: String,
}

impl Case {
    #[must_use]
    pub fn case_id(&self) -> String {
        format!("{}/{}", self.family, self.number)
    }

    /// Upstream executes an example unless it marked it `disabled`.
    #[must_use]
    pub fn executable(&self) -> bool {
        self.tag.as_deref() != Some("disabled")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defect {
    NotJson,
    NotAnExampleArray,
    MissingMember,
}

/// Reads the `CommonMark` specification's own machine-readable example array.
///
/// # Errors
///
/// `NotJson` when the bytes fail strict JSON, and `NotAnExampleArray` or
/// `MissingMember` when the array does not hold the documented example shape.
pub fn commonmark(spec_json: &[u8]) -> Result<Vec<Case>, Defect> {
    let Value::Array(rows) = parse(spec_json).map_err(|_invalid| Defect::NotJson)? else {
        return Err(Defect::NotAnExampleArray);
    };
    rows.iter()
        .map(|row| {
            let Value::Object(members) = row else {
                return Err(Defect::NotAnExampleArray);
            };
            let text = |key: &str| match members.iter().find(|(name, _)| name == key) {
                Some((_, Value::String(value))) => Ok(value.clone()),
                _ => Err(Defect::MissingMember),
            };
            let number = match members.iter().find(|(name, _)| name == "example") {
                Some((_, Value::Integer(value))) => {
                    usize::try_from(*value).map_err(|_range| Defect::MissingMember)?
                }
                _ => return Err(Defect::MissingMember),
            };
            Ok(Case {
                family: COMMONMARK_FAMILY,
                number,
                section: text("section")?,
                tag: None,
                source: text("markdown")?,
                expect: Expect::Html(text("html")?),
                config: String::new(),
            })
        })
        .collect()
}

/// Reads the GFM specification source. An example opens with exactly
/// thirty-two backticks and the word `example`, optionally followed by the
/// extension marker; source and expected HTML are split by a lone `.`; and a
/// tab is written as U+2192.
#[must_use]
pub fn gfm(spec_text: &str) -> Vec<Case> {
    const FENCE: &str = "````````````````````````````````";

    let mut cases = Vec::new();
    let mut section = String::new();
    let mut number = 0_usize;
    let mut source = String::new();
    let mut html = String::new();
    let mut tag = None;
    let mut open = false;
    let mut split = false;

    for line in spec_text.lines() {
        if !open {
            if let Some(title) = line.strip_prefix("## ") {
                section.clear();
                section.push_str(title.trim());
            }
            if let Some(marker) = line
                .strip_prefix(FENCE)
                .and_then(|rest| rest.strip_prefix(" example"))
            {
                open = true;
                split = false;
                source.clear();
                html.clear();
                number = number.saturating_add(1);
                tag = match marker.trim() {
                    "" => None,
                    found => Some(found.to_owned()),
                };
            }
            continue;
        }
        if line == FENCE {
            open = false;
            cases.push(Case {
                family: GFM_FAMILY,
                number,
                section: section.clone(),
                tag: tag.clone(),
                source: source.replace('\u{2192}', "\t"),
                expect: Expect::Html(html.replace('\u{2192}', "\t")),
                config: String::new(),
            });
            continue;
        }
        if line == "." && !split {
            split = true;
            continue;
        }
        let sink = if split { &mut html } else { &mut source };
        sink.push_str(line);
        sink.push('\n');
    }
    cases
}

/// A harvested fixture family, with the count of calls whose source is not a
/// literal (they pass a variable) so a dropped case is never silent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixtures {
    pub cases: Vec<Case>,
    pub skipped: usize,
}

/// Reads a micromark extension's own test suite. Each `micromark(...)` call is
/// one fixture: the first argument is the source, an enclosing `assert.throws`
/// means upstream rejects it, and the regular expression after the closure is
/// the reason it gives. A source assembled by concatenation is refused rather
/// than truncated to its first literal.
#[must_use]
pub fn micromark_fixtures(family: &'static str, text: &str) -> Fixtures {
    let bytes = text.as_bytes();
    let mut cases = Vec::new();
    let mut skipped = 0_usize;
    let mut number = 0_usize;
    let mut at = 0_usize;

    while let Some(call) = find(bytes, b"micromark(", at) {
        let opened = call.saturating_add("micromark(".len());
        at = opened;
        let Some((source, after)) = js_literal(bytes, skip_space(bytes, opened)) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let ends_the_argument = matches!(bytes.get(skip_space(bytes, after)), Some(&b',' | &b')'));
        if !ends_the_argument {
            skipped = skipped.saturating_add(1);
            continue;
        }
        number = number.saturating_add(1);
        let block = rfind(bytes, b"t.test(", call).unwrap_or(0);
        let name = js_literal(
            bytes,
            skip_space(bytes, block.saturating_add("t.test(".len())),
        )
        .map_or_else(String::new, |(text, _end)| text);
        let expect = if rejects(bytes, block, call) {
            Expect::Rejected(reason(bytes, call).unwrap_or_default())
        } else {
            expected_html(bytes, opened).map_or(Expect::Accepted, Expect::Html)
        };
        cases.push(Case {
            family,
            number,
            section: name,
            tag: None,
            source,
            expect,
            config: config(bytes, opened, after),
        });
    }
    Fixtures { cases, skipped }
}

/// Everything the call passes after the source. A suite that configures the
/// extension away from what this profile pins is testing another profile, and
/// the reader of the fixture has to be able to see that.
fn config(bytes: &[u8], opened: usize, after_source: usize) -> String {
    let closed = call_end(bytes, opened).unwrap_or(after_source);
    let body = bytes
        .get(after_source..closed.saturating_sub(1))
        .unwrap_or_default();
    String::from_utf8_lossy(body).into_owned()
}

/// An accepted fixture is compared against the HTML the suite writes as the
/// second argument of its equality. Where that argument is not a literal, the
/// fixture still pins acceptance.
fn expected_html(bytes: &[u8], opened: usize) -> Option<String> {
    let closed = call_end(bytes, opened)?;
    let comma = skip_space(bytes, closed);
    if bytes.get(comma) != Some(&b',') {
        return None;
    }
    let value = skip_space(bytes, comma.saturating_add(1));
    js_literal(bytes, value).map(|(html, _end)| html)
}

/// Walks to the parenthesis that closes the call, stepping over any literal so
/// a bracket inside a string is never counted.
fn call_end(bytes: &[u8], opened: usize) -> Option<usize> {
    let mut depth = 1_usize;
    let mut at = opened;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\'' | b'"' | b'`' => {
                at = skip_literal(bytes, at)?;
                continue;
            }
            b'(' => depth = depth.saturating_add(1),
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at.saturating_add(1));
                }
            }
            _ => {}
        }
        at = at.saturating_add(1);
    }
    None
}

/// Steps over one literal, including a template holding substitutions.
fn skip_literal(bytes: &[u8], at: usize) -> Option<usize> {
    let quote = *bytes.get(at)?;
    let mut cursor = at.saturating_add(1);
    let mut inside = 0_usize;
    while let Some(&byte) = bytes.get(cursor) {
        if byte == b'\\' {
            cursor = cursor.saturating_add(2);
            continue;
        }
        if quote == b'`' && byte == b'$' && bytes.get(cursor.saturating_add(1)) == Some(&b'{') {
            inside = inside.saturating_add(1);
            cursor = cursor.saturating_add(2);
            continue;
        }
        if quote == b'`' && byte == b'}' && inside > 0 {
            inside = inside.saturating_sub(1);
        } else if byte == quote && inside == 0 {
            return Some(cursor.saturating_add(1));
        }
        cursor = cursor.saturating_add(1);
    }
    None
}

/// The call is a rejection when the nearest assertion opened before it is
/// `assert.throws` rather than an equality.
fn rejects(bytes: &[u8], block: usize, call: usize) -> bool {
    let raised = rfind_within(bytes, b"assert.throws", block, call);
    let equal = rfind_within(bytes, b"assert.equal", block, call);
    let deep = rfind_within(bytes, b"assert.deepEqual", block, call);
    match raised {
        None => false,
        Some(at) => equal.is_none_or(|other| at > other) && deep.is_none_or(|other| at > other),
    }
}

/// The rejection reason is the regular expression literal that closes the
/// `assert.throws` call.
fn reason(bytes: &[u8], call: usize) -> Option<String> {
    let mut at = find(bytes, b"}, /", call)?.saturating_add(4);
    let mut out = Vec::new();
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'/' => return String::from_utf8(out).ok(),
            b'\\' => {
                if let Some(&escaped) = bytes.get(at.saturating_add(1)) {
                    out.push(escaped);
                }
                at = at.saturating_add(1);
            }
            _ => out.push(byte),
        }
        at = at.saturating_add(1);
    }
    None
}

/// Decodes one JavaScript string or template literal. A template holding a
/// substitution is not a fixture source and is refused.
fn js_literal(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(at)?;
    if !matches!(quote, b'\'' | b'"' | b'`') {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut cursor = at.saturating_add(1);
    while let Some(&byte) = bytes.get(cursor) {
        if byte == quote {
            let text = String::from_utf8(out).ok()?;
            return Some((text, cursor.saturating_add(1)));
        }
        if byte == b'$' && quote == b'`' && bytes.get(cursor.saturating_add(1)) == Some(&b'{') {
            return None;
        }
        if byte == b'\\' {
            let escaped = *bytes.get(cursor.saturating_add(1))?;
            cursor = cursor.saturating_add(2);
            match escaped {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'0' => out.push(0),
                b'\\' | b'\'' | b'"' | b'`' => out.push(escaped),
                b'u' => {
                    let (point, next) = js_code_point(bytes, cursor)?;
                    let mut buffer = [0_u8; 4];
                    out.extend_from_slice(point.encode_utf8(&mut buffer).as_bytes());
                    cursor = next;
                }
                _ => return None,
            }
            continue;
        }
        out.push(byte);
        cursor = cursor.saturating_add(1);
    }
    None
}

/// Reads the body of a `\u` escape, in either the four-digit or the braced form.
fn js_code_point(bytes: &[u8], at: usize) -> Option<(char, usize)> {
    let braced = bytes.get(at) == Some(&b'{');
    let start = if braced { at.saturating_add(1) } else { at };
    let mut value = 0_u32;
    let mut cursor = start;
    while let Some(&byte) = bytes.get(cursor) {
        let Some(digit) = char::from(byte).to_digit(16) else {
            break;
        };
        value = value.checked_mul(16)?.checked_add(digit)?;
        cursor = cursor.saturating_add(1);
        if !braced && cursor.saturating_sub(start) == 4 {
            break;
        }
    }
    if braced {
        if bytes.get(cursor) != Some(&b'}') {
            return None;
        }
        cursor = cursor.saturating_add(1);
    }
    Some((char::from_u32(value)?, cursor))
}

fn skip_space(bytes: &[u8], at: usize) -> usize {
    let mut cursor = at;
    while matches!(bytes.get(cursor), Some(&byte) if byte.is_ascii_whitespace()) {
        cursor = cursor.saturating_add(1);
    }
    cursor
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let tail = hay.get(from..)?;
    tail.windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at.saturating_add(from))
}

fn rfind(hay: &[u8], needle: &[u8], before: usize) -> Option<usize> {
    hay.get(..before)?
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn rfind_within(hay: &[u8], needle: &[u8], from: usize, before: usize) -> Option<usize> {
    let at = rfind(hay, needle, before)?;
    (at >= from).then_some(at)
}

fn span_value(span: (usize, usize)) -> Value {
    Value::Array(vec![
        Value::Integer(clamp(span.0)),
        Value::Integer(clamp(span.1)),
    ])
}

fn occurrence_value(entry: &Occurrence) -> Value {
    Value::Object(vec![
        (
            "block_kind".to_owned(),
            Value::String(entry.block_kind.as_str().to_owned()),
        ),
        ("block_span".to_owned(), span_value(entry.block_span)),
        (
            "node_path".to_owned(),
            Value::Array(
                entry
                    .node_path
                    .iter()
                    .map(|index| Value::Integer(clamp(*index)))
                    .collect(),
            ),
        ),
        (
            "raw_destination".to_owned(),
            Value::String(entry.raw_destination.clone()),
        ),
        (
            "semantic_destination".to_owned(),
            Value::String(entry.semantic_destination.clone()),
        ),
        (
            "source_construct".to_owned(),
            Value::String(entry.construct.as_str().to_owned()),
        ),
        ("span".to_owned(), span_value(entry.span)),
    ])
}

fn extraction_members(extraction: &Extraction) -> Vec<(String, Value)> {
    vec![
        (
            "occurrences".to_owned(),
            Value::Array(
                extraction
                    .occurrences
                    .iter()
                    .map(occurrence_value)
                    .collect(),
            ),
        ),
        (
            "opaque".to_owned(),
            Value::Object(vec![
                (
                    "frontmatter_bytes".to_owned(),
                    Value::Integer(clamp(extraction.opaque.frontmatter_bytes)),
                ),
                (
                    "html".to_owned(),
                    Value::Array(
                        extraction
                            .opaque
                            .html
                            .iter()
                            .map(|span| span_value(*span))
                            .collect(),
                    ),
                ),
                (
                    "mdx".to_owned(),
                    Value::Array(
                        extraction
                            .opaque
                            .mdx
                            .iter()
                            .map(|span| span_value(*span))
                            .collect(),
                    ),
                ),
            ]),
        ),
    ]
}

fn profile_value(adapter: Adapter, source: &[u8]) -> Value {
    match analyze(adapter, source) {
        Ok(analysis) => {
            let mut members = vec![
                (
                    "nesting".to_owned(),
                    Value::Integer(i64::try_from(analysis.work.nesting).unwrap_or(i64::MAX)),
                ),
                (
                    "nodes".to_owned(),
                    Value::Integer(i64::try_from(analysis.work.nodes).unwrap_or(i64::MAX)),
                ),
            ];
            if let Some(extraction) = &analysis.extraction {
                members.extend(extraction_members(extraction));
            }
            Value::Object(members)
        }
        Err(fault) => Value::Object(vec![(
            "fault".to_owned(),
            Value::String(AnalysisErrorCode::from(fault).as_str().to_owned()),
        )]),
    }
}

fn clamp(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

fn case_value(case: &Case) -> Value {
    let charged: Vec<(String, Value)> = PROFILES
        .iter()
        .map(|adapter| {
            (
                adapter.grammar_profile().to_owned(),
                profile_value(*adapter, case.source.as_bytes()),
            )
        })
        .collect();
    let mut members = vec![
        ("case_id".to_owned(), Value::String(case.case_id())),
        ("section".to_owned(), Value::String(case.section.clone())),
        ("source".to_owned(), Value::String(case.source.clone())),
    ];
    match &case.expect {
        Expect::Html(_) | Expect::Accepted => {
            members.push(("upstream".to_owned(), Value::String("accepted".to_owned())));
        }
        Expect::Rejected(reason) => {
            members.push(("upstream".to_owned(), Value::String("rejected".to_owned())));
            members.push(("upstream_reason".to_owned(), Value::String(reason.clone())));
        }
    }
    if let Some(tag) = &case.tag {
        members.push(("tag".to_owned(), Value::String(tag.clone())));
    }
    members.push(("work".to_owned(), Value::Object(charged)));
    Value::Object(members)
}

/// Builds the manifest: every case's raw source, what upstream says about it,
/// and its exact node count and depth under every published profile.
#[must_use]
pub fn manifest(cases: &[Case], skipped: &[(&'static str, usize)]) -> Value {
    let families = [
        (COMMONMARK_FAMILY, COMMONMARK_PIN),
        (GFM_FAMILY, GFM_PIN),
        (MDX_JSX_FAMILY, MDX_JSX_PIN),
        (MDX_EXPRESSION_FAMILY, MDX_EXPRESSION_PIN),
        (MDX_ESM_FAMILY, MDX_ESM_PIN),
        (FOOTNOTE_FAMILY, FOOTNOTE_PIN),
        (STRIKETHROUGH_FAMILY, STRIKETHROUGH_PIN),
        (GITHUB_FOOTNOTE_FAMILY, GITHUB_FOOTNOTE_PIN),
    ];
    let family_rows: Vec<Value> = families
        .iter()
        .map(|(family, pin)| {
            let count = cases.iter().filter(|case| case.family == *family).count();
            let dropped = skipped
                .iter()
                .find(|(name, _)| name == family)
                .map_or(0, |(_, count)| *count);
            Value::Object(vec![
                ("cases".to_owned(), Value::Integer(clamp(count))),
                ("family".to_owned(), Value::String((*family).to_owned())),
                ("input_digest".to_owned(), Value::String((*pin).to_owned())),
                ("not_a_literal".to_owned(), Value::Integer(clamp(dropped))),
            ])
        })
        .collect();
    let profiles: Vec<Value> = PROFILES
        .iter()
        .map(|adapter| Value::String(adapter.grammar_profile().to_owned()))
        .collect();
    Value::Object(vec![
        ("schema".to_owned(), Value::String(SCHEMA.to_owned())),
        ("families".to_owned(), Value::Array(family_rows)),
        ("profiles".to_owned(), Value::Array(profiles)),
        (
            "cases".to_owned(),
            Value::Array(cases.iter().map(case_value).collect()),
        ),
    ])
}

/// The documents the footnote suite renders against github.com's own HTML.
/// Cases are numbered by sorted name so the manifest never moves with the
/// directory listing.
#[must_use]
pub fn github_fixtures(pairs: &[(String, String, String)]) -> Vec<Case> {
    pairs
        .iter()
        .enumerate()
        .map(|(index, (name, source, html))| Case {
            family: GITHUB_FOOTNOTE_FAMILY,
            number: index.saturating_add(1),
            section: name.clone(),
            tag: None,
            source: source.clone(),
            expect: Expect::Html(html.clone()),
            config: String::new(),
        })
        .collect()
}

/// One digest over a whole directory, so a fixture cannot be edited, added, or
/// dropped without the pin moving.
#[must_use]
pub fn directory_digest(files: &[(String, String)]) -> String {
    let members: Vec<(String, Value)> = files
        .iter()
        .map(|(name, body)| (name.clone(), Value::String(body.clone())))
        .collect();
    hb(GITHUB_FOOTNOTE_FAMILY, &canonical(&Value::Object(members))).to_string()
}
