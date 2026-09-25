use sha2::Digest as _;

use amiss_md::extract::RESERVED_LABEL_PREFIX;
use amiss_md::{Occurrence, Opaque, Work};
use amiss_wire::controls::{KeyFormat, KeyValueSelection, ProjectionKind};
use amiss_wire::extraction::GovernedDefinition;
use amiss_wire::model::{Adapter, Digest, RepoPath};
use amiss_wire::report::model::{
    ProjectionDifference, ProjectionObserved, RowsProjectionDifference,
};

/// One-based Unicode-scalar display positions for a machine byte span, after
/// the same CRLF and bare-CR to LF conversion the projection applies. A tab is
/// one scalar and no display-width expansion occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanDisplay {
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
}

/// One extracted occurrence enriched with what the report needs beyond the
/// corpus goldens: display positions, the containing block's projection
/// digest, and the raw destination digest, where an empty destination hashes
/// zero bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedOccurrence {
    pub occurrence: Occurrence,
    pub display: SpanDisplay,
    pub projection_digest: Digest,
    pub raw_destination_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticCodeSink {
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub digest: Digest,
    pub value: String,
}

/// One reserved governed definition with its raw span, display positions,
/// the digest of its exact contributing source bytes, and the claim form
/// its words spell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSource {
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub digest: Digest,
    pub form: GovernedForm,
    pub previous_code: Option<SemanticCodeSink>,
}

/// The raw anchor inputs a scanned document retains so the resolve lane never
/// parses an in-set target twice; slugging stays lazy, paid only for targets
/// a fragment actually asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorSource {
    pub headings: Vec<amiss_wire::extraction::Heading>,
    pub html_anchors: Vec<String>,
    pub transclusions: Vec<amiss_wire::extraction::Transclusion>,
}

/// What a document's frontmatter says about where and how it is published.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Publication {
    pub slug: Option<String>,
    pub id: Option<String>,
    pub redirects: Vec<String>,
    pub layout: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub adapter: Adapter,
    pub work: Work,
    pub embedded_code_bytes: u64,
    pub occurrences: Vec<ScannedOccurrence>,
    pub opaque: Opaque,
    pub governed: Vec<GovernedSource>,
    pub declared_anchors: Vec<String>,
    pub publication: Publication,
    /// Where an `AsciiDoc` document's image macros resolve from.
    pub images_dir: amiss_adoc::ImagesDir,
    pub anchor_source: Option<AnchorSource>,
    /// Read only once the HTML comments the MDX grammar refused were read as
    /// comments, which a Docusaurus site does and MDX alone does not.
    pub commented: bool,
}

/// What a reserved governed definition spells: the one claim kind this
/// engine evaluates, or a capability it does not implement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernedForm {
    Value(ValueClaim),
    Projection { name: String },
    Unknown,
}

/// One value claim: the document asserts that line `line` of the repository
/// file at `path`, without its terminator, is exactly `expected`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueClaim {
    pub name: String,
    pub path: RepoPath,
    pub line: u64,
    pub expected: String,
}

/// Classifies one reserved definition against the closed claim grammar.
/// Everything outside it stays an unsupported capability, so an unknown
/// kind is refused rather than guessed at.
#[must_use]
pub fn classify(definition: &GovernedDefinition) -> GovernedForm {
    if let Some(claim) = value_claim(definition) {
        GovernedForm::Value(claim)
    } else if definition.angled
        && definition.title.is_none()
        && definition.url == "amiss:projection"
        && let Some(name) = definition.label.strip_prefix(RESERVED_LABEL_PREFIX)
        && amiss_wire::extraction::governed_name_valid(name)
    {
        GovernedForm::Projection {
            name: name.to_owned(),
        }
    } else {
        GovernedForm::Unknown
    }
}

fn value_claim(definition: &GovernedDefinition) -> Option<ValueClaim> {
    if !definition.angled {
        return None;
    }
    let name = definition.label.strip_prefix(RESERVED_LABEL_PREFIX)?;
    if !amiss_wire::extraction::governed_name_valid(name) {
        return None;
    }
    let rest = definition.url.strip_prefix("amiss:value?path=")?;
    let (path_text, line_part) = rest.split_once('&')?;
    let line_text = line_part.strip_prefix("line=L")?;
    if line_text.contains('&') {
        return None;
    }
    let line = safe_line_number(line_text)?;
    if path_text.is_empty() || path_text.contains(['?', '#']) {
        return None;
    }
    let path = RepoPath::new(path_text.to_owned())?;
    let expected = definition.title.clone()?;
    Some(ValueClaim {
        name: name.to_owned(),
        path,
        line,
        expected,
    })
}

const MAX_SAFE: u64 = 9_007_199_254_740_991;

/// One safe line number: nonzero first digit, at most sixteen digits, and
/// within the range every consumer of the report can hold exactly.
pub(crate) fn safe_line_number(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 16 || bytes.first() == Some(&b'0') {
        return None;
    }
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    text.parse::<u64>().ok().filter(|value| *value <= MAX_SAFE)
}

pub(crate) const CODE_TEXT_SOURCE_DOMAIN: &str = "amiss/scanner-code-text-source";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Attested,
    Drift {
        reason: ProjectionObserved,
        expected_digest: Option<Digest>,
        observed_digest: Option<Digest>,
        expected_bytes: Option<u64>,
        observed_bytes: Option<u64>,
        difference: Option<ProjectionDifference<Box<RowsProjectionDifference>>>,
    },
}

pub(crate) fn unavailable(reason: ProjectionObserved, sink: &SemanticCodeSink) -> Verdict {
    Verdict::Drift {
        reason,
        expected_digest: None,
        observed_digest: Some(sink.digest),
        expected_bytes: None,
        observed_bytes: Some(u64::try_from(sink.value.len()).unwrap_or(u64::MAX)),
        difference: None,
    }
}

/// The verdict one code-text value earns against the visible block: equal to
/// it under `code-text-v1`, somewhere inside it under `contains-v1`.
pub(crate) fn code_verdict(
    expected: &[u8],
    projection: ProjectionKind,
    sink: &SemanticCodeSink,
) -> Verdict {
    let observed = sink.value.as_bytes();
    let (held, reason) = match projection {
        ProjectionKind::ContainsV1 => (
            memchr::memmem::find(observed, expected).is_some(),
            ProjectionObserved::ContentAbsent,
        ),
        ProjectionKind::CodeTextV1
        | ProjectionKind::SortedRowsV1
        | ProjectionKind::DecimalCountV1 => {
            (expected == observed, ProjectionObserved::ContentDiffers)
        }
    };
    if held {
        return Verdict::Attested;
    }
    Verdict::Drift {
        reason,
        expected_digest: Some(Digest::from(
            sha2::Sha256::new_with_prefix(CODE_TEXT_SOURCE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(expected)
                .finalize()
                .0,
        )),
        observed_digest: Some(sink.digest),
        expected_bytes: Some(u64::try_from(expected.len()).unwrap_or(u64::MAX)),
        observed_bytes: Some(u64::try_from(observed.len()).unwrap_or(u64::MAX)),
        difference: None,
    }
}

/// The scalar one key path names in a TOML or JSON file, spelled the way the
/// file's own grammar prints it: a string as its text, a number, boolean or
/// date as written. A table, array or null names no one value.
pub(crate) fn key_value(
    body: &[u8],
    selection: &KeyValueSelection,
) -> Result<String, ProjectionObserved> {
    let text = std::str::from_utf8(body).map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
    match selection.format {
        KeyFormat::Toml => {
            let mut table: toml::Table =
                toml::from_str(text).map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
            let (last, parents) = selection
                .key
                .split_last()
                .ok_or(ProjectionObserved::SourceKeyAbsent)?;
            for segment in parents {
                match table.remove(segment) {
                    Some(toml::Value::Table(inner)) => table = inner,
                    Some(_) | None => return Err(ProjectionObserved::SourceKeyAbsent),
                }
            }
            match table
                .remove(last)
                .ok_or(ProjectionObserved::SourceKeyAbsent)?
            {
                toml::Value::String(value) => Ok(value),
                toml::Value::Integer(value) => Ok(value.to_string()),
                toml::Value::Float(value) => Ok(value.to_string()),
                toml::Value::Boolean(value) => Ok(value.to_string()),
                toml::Value::Datetime(value) => Ok(value.to_string()),
                toml::Value::Array(_) | toml::Value::Table(_) => {
                    Err(ProjectionObserved::SourceKeyNotScalar)
                }
            }
        }
        KeyFormat::Json => {
            let mut value: serde_json::Value = serde_json::from_str(text)
                .map_err(|_defect| ProjectionObserved::SourceUnparsable)?;
            for segment in &selection.key {
                value = match value {
                    serde_json::Value::Object(mut object) => object
                        .remove(segment)
                        .ok_or(ProjectionObserved::SourceKeyAbsent)?,
                    serde_json::Value::Null
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::String(_)
                    | serde_json::Value::Array(_) => {
                        return Err(ProjectionObserved::SourceKeyAbsent);
                    }
                };
            }
            match value {
                serde_json::Value::String(value) => Ok(value),
                serde_json::Value::Number(value) => Ok(value.to_string()),
                serde_json::Value::Bool(value) => Ok(value.to_string()),
                serde_json::Value::Null
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => Err(ProjectionObserved::SourceKeyNotScalar),
            }
        }
    }
}
