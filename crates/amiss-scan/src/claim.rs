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
    Broken {
        observed_digest: Digest,
        observed: Vec<u8>,
    },
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
/// Which invisible construct carries a claim, which is what a provable
/// rewrite must respell: the fix regenerates the carrier, not just the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimCarrier {
    Definition,
    RstComment,
    AdocComment,
}

impl ClaimCarrier {
    #[must_use]
    pub const fn of(adapter: amiss_wire::model::Adapter) -> Self {
        match adapter {
            amiss_wire::model::Adapter::Rst => Self::RstComment,
            amiss_wire::model::Adapter::AsciiDoc => Self::AdocComment,
            amiss_wire::model::Adapter::Markdown
            | amiss_wire::model::Adapter::Mdx
            | amiss_wire::model::Adapter::PlainAdvisory => Self::Definition,
        }
    }

    const fn prefix(self) -> &'static str {
        match self {
            Self::Definition => "",
            Self::RstComment => ".. ",
            Self::AdocComment => "// ",
        }
    }

    const fn prover(self) -> amiss_wire::model::Adapter {
        match self {
            Self::Definition => amiss_wire::model::Adapter::Markdown,
            Self::RstComment => amiss_wire::model::Adapter::Rst,
            Self::AdocComment => amiss_wire::model::Adapter::AsciiDoc,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub carrier: ClaimCarrier,
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

/// The canonical respelling that would attest a broken claim: the whole
/// definition rewritten with the observed line as its expected words. None
/// when the observed line cannot be spelled as a quoted title, or when the
/// respelled definition does not classify back to the identical claim under
/// the real extractor, which is the proof the fix resolves the finding.
#[must_use]
pub fn rewrite(
    name: &str,
    path: &RepoPath,
    line: u64,
    observed: &[u8],
    carrier: ClaimCarrier,
) -> Option<String> {
    let observed = std::str::from_utf8(observed).ok()?;
    if observed
        .chars()
        .any(|character| character == '"' || character == '\\' || character.is_control())
    {
        return None;
    }
    let path_text = path.as_str()?;
    let replacement = format!(
        "{}[amiss:{name}]: <amiss:value?path={path_text}&line=L{line}> \"{observed}\"",
        carrier.prefix(),
    );
    let mut resources = crate::ScanResources::new(crate::ScanLimits::CONTRACT);
    let scanned =
        crate::scan_document(&mut resources, carrier.prover(), replacement.as_bytes()).ok()?;
    let [source] = scanned.governed.as_slice() else {
        return None;
    };
    match &source.form {
        GovernedForm::Value(claim)
            if claim.name == name
                && claim.path == *path
                && claim.line == line
                && claim.expected == observed =>
        {
            Some(replacement)
        }
        GovernedForm::Value(_) | GovernedForm::Unknown => None,
    }
}
