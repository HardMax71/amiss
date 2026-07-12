use amiss_wire::json::{Value, parse};
use amiss_wire::model::Adapter;
use amiss_wire::report::AnalysisErrorCode;

use crate::accounting::{Fault, Work, charge};

pub const SCHEMA: &str = "amiss/parser-profile-corpus/v1";

pub const COMMONMARK_FAMILY: &str = "commonmark-0.31.2";
pub const COMMONMARK_PIN: &str =
    "sha256:d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20";

pub const GFM_FAMILY: &str = "gfm-0.29";
pub const GFM_PIN: &str = "sha256:7d8e5814befec287ac116786d81ff14e0adc9b13295b4494649e995408fd871c";

/// The profiles this corpus publishes goldens for. `mdx-source-v1` is absent
/// until its own families land, and the manifest names what it covers so a
/// reader never mistakes a partial corpus for a complete one.
pub const PROFILES: [Adapter; 2] = [Adapter::Markdown, Adapter::PlainAdvisory];

/// One executable example: the raw source plus, where upstream publishes one,
/// its expected HTML. `tag` carries the GFM extension marker, where `disabled`
/// means upstream does not execute the example.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub family: &'static str,
    pub number: usize,
    pub section: String,
    pub tag: Option<String>,
    pub source: String,
    pub html: String,
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
                html: text("html")?,
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
                html: html.replace('\u{2192}', "\t"),
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

fn work_value(work: Result<Work, Fault>) -> Value {
    match work {
        Ok(charged) => Value::Object(vec![
            ("nesting".to_owned(), Value::Integer(clamp(charged.nesting))),
            ("nodes".to_owned(), Value::Integer(clamp(charged.nodes))),
        ]),
        Err(fault) => Value::Object(vec![(
            "fault".to_owned(),
            Value::String(AnalysisErrorCode::from(fault).as_str().to_owned()),
        )]),
    }
}

fn clamp(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

fn case_value(case: &Case) -> Value {
    let charged: Vec<(String, Value)> = PROFILES
        .iter()
        .map(|adapter| {
            (
                adapter.grammar_profile().to_owned(),
                work_value(charge(*adapter, case.source.as_bytes())),
            )
        })
        .collect();
    let mut members = vec![
        ("case_id".to_owned(), Value::String(case.case_id())),
        ("section".to_owned(), Value::String(case.section.clone())),
        ("source".to_owned(), Value::String(case.source.clone())),
        ("work".to_owned(), Value::Object(charged)),
    ];
    if let Some(tag) = &case.tag {
        members.push(("tag".to_owned(), Value::String(tag.clone())));
    }
    Value::Object(members)
}

/// Builds the manifest: every case's raw source and its exact node count and
/// depth under every published profile.
#[must_use]
pub fn manifest(cases: &[Case]) -> Value {
    let families = [(COMMONMARK_FAMILY, COMMONMARK_PIN), (GFM_FAMILY, GFM_PIN)];
    let family_rows: Vec<Value> = families
        .iter()
        .map(|(family, pin)| {
            let count = cases.iter().filter(|case| case.family == *family).count();
            Value::Object(vec![
                (
                    "cases".to_owned(),
                    Value::Integer(clamp(u64::try_from(count).unwrap_or(u64::MAX))),
                ),
                ("family".to_owned(), Value::String((*family).to_owned())),
                ("input_digest".to_owned(), Value::String((*pin).to_owned())),
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
