use amiss_md::extract::RESERVED_LABEL_PREFIX;
use amiss_wire::digest::Digest;
use amiss_wire::extraction::GovernedDefinition;
use amiss_wire::model::RepoPath;

use crate::scan::SpanDisplay;

/// What a reserved governed definition spells: the one claim kind this
/// engine evaluates, or a capability it does not implement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernedForm {
    Value(ValueClaim),
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
    match value_claim(definition) {
        Some(claim) => GovernedForm::Value(claim),
        None => GovernedForm::Unknown,
    }
}

fn value_claim(definition: &GovernedDefinition) -> Option<ValueClaim> {
    if !definition.angled {
        return None;
    }
    let name = definition.label.strip_prefix(RESERVED_LABEL_PREFIX)?;
    if !claim_name(name) {
        return None;
    }
    let rest = definition.url.strip_prefix("amiss:value?path=")?;
    let (path_text, line_part) = rest.split_once('&')?;
    let line_text = line_part.strip_prefix("line=L")?;
    if line_text.contains('&') {
        return None;
    }
    let line = crate::resolve::safe_line_number(line_text)?;
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

/// A claim name is rule-id safe: it heads a rule id, so it holds no slash.
fn claim_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.len() <= 120
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The one answer a value claim can get from the tree it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimVerdict {
    Attested,
    Broken { observed_digest: Digest },
    TargetMissing(ClaimMissingReason),
}

/// Why a claim's target could not answer at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimMissingReason {
    Absent,
    NotABlob,
    LfsPointer,
    LineOutOfRange,
}

/// One evaluated claim, carried from the candidate walk to the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub document: RepoPath,
    pub name: String,
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub source_digest: Digest,
    pub path: RepoPath,
    pub line: u64,
    pub expected_digest: Digest,
    pub verdict: ClaimVerdict,
}
