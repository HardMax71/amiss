use amiss_wire::model::Digest;
use amiss_wire::model::RepoPath;

use crate::scanned::{GovernedForm, SpanDisplay};

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
        GovernedForm::Value(_) | GovernedForm::Projection { .. } | GovernedForm::Unknown => None,
    }
}
