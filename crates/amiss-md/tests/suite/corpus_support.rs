use amiss_wire::model::Adapter;
use amiss_wire::report::AnalysisErrorCode;
use serde_json::{Value, json};
use sha2::Digest as _;

use amiss_md::analyze;

pub(crate) const SCHEMA: &str = "amiss/parser-profile-corpus";

pub(crate) const COMMONMARK_FAMILY: &str = "commonmark-0.31.2";
pub(crate) const COMMONMARK_PIN: &str =
    "sha256:d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20";

pub(crate) const GFM_FAMILY: &str = "gfm-0.29";
pub(crate) const GFM_PIN: &str =
    "sha256:7d8e5814befec287ac116786d81ff14e0adc9b13295b4494649e995408fd871c";

pub(crate) const MDX_JSX_FAMILY: &str = "micromark-mdx-jsx-3.0.2";
pub(crate) const MDX_JSX_PIN: &str =
    "sha256:17df57441a015be02a333f78fb8aeddf0d93586019fc7c4ae665d00dab666c32";

pub(crate) const MDX_EXPRESSION_FAMILY: &str = "micromark-mdx-expression-3.0.1";
pub(crate) const MDX_EXPRESSION_PIN: &str =
    "sha256:2aaf8667378829192bf25674fed0edeccd759a7ce0b0c3eaf5625faeea364be6";

pub(crate) const MDX_ESM_FAMILY: &str = "micromark-mdxjs-esm-3.0.0";
pub(crate) const MDX_ESM_PIN: &str =
    "sha256:fdffc20bfaef4fcbdc6640a7fef9dfa6ec35715d455baeadd8a6c34e866a3151";

pub(crate) const FOOTNOTE_FAMILY: &str = "micromark-gfm-footnote-2.1.0";
pub(crate) const FOOTNOTE_PIN: &str =
    "sha256:41a437756e5c4615dfe9269acb23acbc74d8b01d9f7cabb4f121e8ca7e5d1a18";

pub(crate) const STRIKETHROUGH_FAMILY: &str = "micromark-gfm-strikethrough-2.1.0";
pub(crate) const STRIKETHROUGH_PIN: &str =
    "sha256:b7bdf617e8535348265bb8d91f0c7da65b7849e150460a44b063b22640e5178b";

/// The footnote suite also drives a directory of documents against the HTML
/// github.com itself renders for them. That directory is pinned whole, by one
/// digest over the canonical JSON of every file in it.
pub(crate) const GITHUB_FOOTNOTE_FAMILY: &str = "github-gfm-footnote-2.1.0";
pub(crate) const GITHUB_FOOTNOTE_PIN: &str =
    "sha256:24829d3c8c494684d63bd3d613578504371f0da8b8ef1a6bbae5a7093fa27e1a";

/// Every case is charged under every profile, so a grammar change anywhere
/// moves the manifest. The manifest names what it covers, so a reader never
/// mistakes a partial corpus for a complete one.
pub(crate) const PROFILES: [Adapter; 3] = [Adapter::Markdown, Adapter::Mdx, Adapter::PlainAdvisory];

/// What upstream says about a case: the HTML it publishes for the example, or
/// the message it rejects the example with, or nothing beyond acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expect {
    Html(String),
    Accepted,
    Rejected(String),
}

/// One executable example. `tag` carries the GFM extension marker, where
/// `disabled` means upstream does not execute the example.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Case {
    pub(crate) family: &'static str,
    pub(crate) number: usize,
    pub(crate) section: String,
    pub(crate) tag: Option<String>,
    pub(crate) source: String,
    pub(crate) expect: Expect,
    pub(crate) config: String,
}

impl Case {
    #[must_use]
    pub(crate) fn case_id(&self) -> String {
        format!("{}/{}", self.family, self.number)
    }

    /// Upstream executes an example unless it marked it `disabled`.
    #[must_use]
    pub(crate) fn executable(&self) -> bool {
        self.tag.as_deref() != Some("disabled")
    }
}

#[derive(serde::Deserialize)]
struct CommonmarkExample {
    example: usize,
    section: String,
    markdown: String,
    html: String,
}

/// Reads the `CommonMark` specification's machine-readable example array.
///
/// # Errors
/// Rejects invalid JSON or examples without their source, HTML, section and ordinal.
pub(crate) fn commonmark(spec_json: &[u8]) -> serde_json::Result<Vec<Case>> {
    let examples: Vec<CommonmarkExample> = serde_json::from_slice(spec_json)?;
    Ok(examples
        .into_iter()
        .map(|row| Case {
            family: COMMONMARK_FAMILY,
            number: row.example,
            section: row.section,
            tag: None,
            source: row.markdown,
            expect: Expect::Html(row.html),
            config: String::new(),
        })
        .collect())
}

/// Reads the GFM specification source. An example opens with exactly
/// thirty-two backticks and the word `example`, optionally followed by the
/// extension marker; source and expected HTML are split by a lone `.`; and a
/// tab is written as U+2192.
#[must_use]
pub(crate) fn gfm(spec_text: &str) -> Vec<Case> {
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
pub(crate) struct Fixtures {
    pub(crate) cases: Vec<Case>,
    pub(crate) skipped: usize,
}

/// Reads a micromark extension's own test suite. Each `micromark(...)` call is
/// one fixture: the first argument is the source, an enclosing `assert.throws`
/// means upstream rejects it, and the regular expression after the closure is
/// the reason it gives. A source assembled by concatenation is refused rather
/// than truncated to its first literal.
#[must_use]
pub(crate) fn micromark_fixtures(family: &'static str, text: &str) -> Fixtures {
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

/// Decodes one representable code point from one or two adjacent `\u` escapes.
fn js_code_point(bytes: &[u8], at: usize) -> Option<(char, usize)> {
    let read = |start: usize| -> Option<(u32, usize)> {
        let braced = bytes.get(start) == Some(&b'{');
        let first = if braced { start.checked_add(1)? } else { start };
        let last = if braced {
            bytes
                .get(first..)?
                .iter()
                .position(|byte| *byte == b'}')?
                .checked_add(first)?
        } else {
            first.checked_add(4)?
        };
        if first == last {
            return None;
        }
        let value = bytes
            .get(first..last)?
            .iter()
            .try_fold(0_u32, |value, byte| {
                value
                    .checked_mul(16)?
                    .checked_add(char::from(*byte).to_digit(16)?)
            })?;
        let next = if braced { last.checked_add(1)? } else { last };
        Some((value, next))
    };

    let (leading, next) = read(at)?;
    if let Some(point) = char::from_u32(leading) {
        return Some((point, next));
    }
    if !(0xD800..=0xDBFF).contains(&leading) {
        return None;
    }
    let marker = next.checked_add(1)?;
    if bytes.get(next) != Some(&b'\\') || bytes.get(marker) != Some(&b'u') {
        return None;
    }
    let (trailing, end) = read(marker.checked_add(1)?)?;
    if !(0xDC00..=0xDFFF).contains(&trailing) {
        return None;
    }
    let mut decoded =
        char::decode_utf16([u16::try_from(leading).ok()?, u16::try_from(trailing).ok()?]);
    Some((decoded.next()?.ok()?, end))
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

fn profile_work(adapter: Adapter, source: &[u8]) -> Value {
    match analyze(adapter, source, u64::MAX) {
        Ok(analysis) => {
            let Some(extraction) = &analysis.extraction else {
                return json!({ "nesting": analysis.work.nesting, "nodes": analysis.work.nodes });
            };
            let occurrences: Vec<_> = extraction
                .occurrences
                .iter()
                .map(|entry| {
                    json!({
                        "block_kind": entry.block_kind,
                        "block_span": entry.block_span,
                        "node_path": entry.node_path,
                        "raw_destination": entry.raw_destination,
                        "semantic_destination": entry.semantic_destination,
                        "source_construct": entry.construct,
                        "span": entry.span,
                    })
                })
                .collect();
            let headings: Vec<_> = extraction
                .headings
                .iter()
                .map(|heading| {
                    json!({
                        "attribute": heading.attribute.as_ref().map(|attribute| json!({
                            "id": attribute.id,
                            "suffix": attribute.suffix,
                        })),
                        "source": heading.source.as_ref(),
                        "span": heading.span,
                        "text": heading.text,
                    })
                })
                .collect();
            json!({
                "declared_anchors": extraction.declared_anchors,
                "headings": headings,
                "html_anchors": extraction.html_anchors,
                "nesting": analysis.work.nesting,
                "nodes": analysis.work.nodes,
                "occurrences": occurrences,
                "opaque": {
                    "frontmatter_bytes": extraction.opaque.frontmatter_bytes,
                    "html": extraction.opaque.html,
                    "mdx": extraction.opaque.mdx,
                },
            })
        }
        Err(error) => {
            let code = match error {
                amiss_md::AnalyzeError::Fault(fault) => AnalysisErrorCode::from(fault),
                amiss_md::AnalyzeError::EmbeddedCodeAllowance { .. } => {
                    AnalysisErrorCode::ResourceLimitExceeded
                }
            };
            json!({ "fault": code })
        }
    }
}

#[derive(serde::Serialize)]
struct ManifestCase<'a> {
    case_id: String,
    section: &'a str,
    source: &'a str,
    upstream: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<&'a str>,
    work: std::collections::BTreeMap<&'static str, Value>,
}

/// Builds the manifest: every case's raw source, what upstream says about it,
/// and its exact node count and depth under every published profile.
#[must_use]
pub(crate) fn manifest(cases: &[Case], skipped: &[(&'static str, usize)]) -> Value {
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
    let family_rows: Vec<_> = families.iter().map(|(family, pin)| {
        let count = cases.iter().filter(|case| case.family == *family).count();
        let dropped = skipped.iter().find(|(name, _)| name == family).map_or(0, |(_, count)| *count);
        json!({ "cases": count, "family": family, "input_digest": pin, "not_a_literal": dropped })
    }).collect();
    let profiles: Vec<_> = PROFILES
        .iter()
        .map(|adapter| adapter.metadata().grammar_profile)
        .collect();
    let cases: Vec<_> = cases
        .iter()
        .map(|case| {
            let (upstream, upstream_reason) = match &case.expect {
                Expect::Html(_) | Expect::Accepted => ("accepted", None),
                Expect::Rejected(reason) => ("rejected", Some(reason.as_str())),
            };
            ManifestCase {
                case_id: case.case_id(),
                section: &case.section,
                source: &case.source,
                upstream,
                upstream_reason,
                tag: case.tag.as_deref(),
                work: PROFILES
                    .iter()
                    .map(|adapter| {
                        (
                            adapter.metadata().grammar_profile,
                            profile_work(*adapter, case.source.as_bytes()),
                        )
                    })
                    .collect(),
            }
        })
        .collect();
    json!({ "schema": SCHEMA, "families": family_rows, "profiles": profiles, "cases": cases })
}

/// The documents the footnote suite renders against github.com's own HTML.
/// Cases are numbered by sorted name so the manifest never moves with the
/// directory listing.
#[must_use]
pub(crate) fn github_fixtures(pairs: &[(String, String, String)]) -> Vec<Case> {
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
pub(crate) fn directory_digest(
    files: &[(String, String)],
) -> Result<String, Box<dyn std::error::Error>> {
    let members: std::collections::BTreeMap<_, _> =
        files.iter().map(|(name, body)| (name, body)).collect();
    Ok(amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(GITHUB_FOOTNOTE_FAMILY)
            .chain_update([0_u8])
            .chain_update(&serde_json_canonicalizer::to_vec(&members)?)
            .finalize()
            .0,
    )
    .to_string())
}
