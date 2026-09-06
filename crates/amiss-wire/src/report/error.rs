use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use super::model::{AnalysisError, AnalysisPhase, EvaluationUnavailableReason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AnalysisRoute {
    pub phase: AnalysisPhase,
    pub evaluation_reason: Option<EvaluationUnavailableReason>,
}

const INVALID_INVOCATION: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Invocation,
    evaluation_reason: Some(EvaluationUnavailableReason::InvalidInvocation),
};
const INVALID_EVENT: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Invocation,
    evaluation_reason: Some(EvaluationUnavailableReason::InvalidEvent),
};
const INVALID_PROFILE: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Invocation,
    evaluation_reason: Some(EvaluationUnavailableReason::InvalidProfile),
};
const REQUEST_UNREADABLE: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Invocation,
    evaluation_reason: Some(EvaluationUnavailableReason::RequestUnreadable),
};
const CONFIGURATION: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Configuration,
    evaluation_reason: None,
};
const GIT: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Git,
    evaluation_reason: None,
};
const PARSE: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Parse,
    evaluation_reason: None,
};
const RESOLUTION: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Resolution,
    evaluation_reason: None,
};
const POLICY: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Policy,
    evaluation_reason: None,
};
const OUTPUT: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Output,
    evaluation_reason: None,
};
const INTERNAL: AnalysisRoute = AnalysisRoute {
    phase: AnalysisPhase::Internal,
    evaluation_reason: None,
};

declare_taxonomy! {
    /// The closed analysis-error codes in schema declaration order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, Display, EnumIter, EnumString, IntoStaticStr, SerializeDisplay, DeserializeFromStr)]
    #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
    pub enum AnalysisErrorCode {
        InvalidInvocation => {
            meaning: "the command line does not match the closed grammar; each documented option appears at most once and nothing else is accepted",
            metadata: Some(&INVALID_INVOCATION),
        },
        InvalidEvent => {
            meaning: "the declared repository, ref, or default-branch identity is not in canonical form; pass a lowercase owner and name and full refs/heads/ references",
            metadata: Some(&INVALID_EVENT),
        },
        InvalidProfile => {
            meaning: "the profile is not observe, enforce-introduced, or enforce",
            metadata: Some(&INVALID_PROFILE),
        },
        RequestUnreadable => {
            meaning: "the machine evaluation request bytes could not be read; nothing was evaluated",
            metadata: Some(&REQUEST_UNREADABLE),
        },
        ConfigurationInvalid => {
            meaning: "a policy or control input violates its schema; one unknown field or malformed value makes the whole file invalid rather than partly honored",
            metadata: Some(&CONFIGURATION),
        },
        DuplicateJsonKey => {
            meaning: "a JSON input repeats an object key; strict parsing refuses the file instead of choosing one of the values",
            metadata: Some(&CONFIGURATION),
        },
        InvalidUtf8 => {
            meaning: "a JSON input carries bytes that are not UTF-8",
            metadata: Some(&CONFIGURATION),
        },
        InvalidJson => {
            meaning: "an input that must be JSON does not parse as strict JSON",
            metadata: Some(&CONFIGURATION),
        },
        UnknownSchema => {
            meaning: "a JSON input declares a schema identifier this engine does not recognize",
            metadata: Some(&CONFIGURATION),
        },
        UnknownField => {
            meaning: "a JSON input carries a field its closed schema does not define; unknown fields refuse rather than pass through unread",
            metadata: Some(&CONFIGURATION),
        },
        NoncanonicalArray => {
            meaning: "a JSON input array violates its required canonical ordering or uniqueness",
            metadata: Some(&CONFIGURATION),
        },
        DigestMismatch => {
            meaning: "a digest carried by an input does not match the bytes it names; the input is stale or altered",
            metadata: Some(&CONFIGURATION),
        },
        ControlBindingMismatch => {
            meaning: "an external control is bound to a different repository, ref, or run identity than this evaluation; nothing is applied and the run ends incomplete",
            metadata: Some(&CONFIGURATION),
        },
        ExceptionOverlap => {
            meaning: "accepted exception items select the same finding more than once; overlap ends evaluation incomplete instead of double-suppressing",
            metadata: Some(&CONFIGURATION),
        },
        UnsupportedCapability => {
            meaning: "a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim",
            metadata: Some(&POLICY),
        },
        GitRepositoryUnavailable => {
            meaning: "the --repo path does not open as a Git repository of the declared object format",
            metadata: Some(&GIT),
        },
        GitObjectMissing => {
            meaning: "a commit, tree, or blob the run needs is absent from the object store; fetch full history or name commits the store holds",
            metadata: Some(&GIT),
        },
        GitObjectWrongKind => {
            meaning: "a Git object is not the kind its use requires, as when a named commit resolves to another type",
            metadata: Some(&GIT),
        },
        GitObjectUnreadable => {
            meaning: "a Git object exists but its bytes cannot be decoded",
            metadata: Some(&GIT),
        },
        GitIndexInvalid => {
            meaning: "the staged index file does not parse under the index grammar",
            metadata: Some(&GIT),
        },
        GitIndexUnmerged => {
            meaning: "the index holds unmerged conflict entries, so no single staged state exists; finish or abort the merge before checking the index",
            metadata: Some(&GIT),
        },
        GitIntentToAdd => {
            meaning: "the index holds an intent-to-add entry whose content is not staged; stage the file or drop the intent entry before checking the index",
            metadata: Some(&GIT),
        },
        GitSnapshotChanged => {
            meaning: "the staged index changed while the run was reading it; rerun when the repository is quiet",
            metadata: Some(&GIT),
        },
        UnrepresentablePath => {
            meaning: "a tree or index name is outside the path grammar, a backslash, a NUL, or a dot segment; the exact bytes are disclosed as hex",
            metadata: Some(&GIT),
        },
        DocumentInvalid => {
            meaning: "a discovered document's bytes cannot be decoded as its format requires; the run refuses instead of skipping the file and passing",
            metadata: Some(&PARSE),
        },
        ParserError => {
            meaning: "the pinned parser failed on a document; the document is named and the run is incomplete rather than the file silently dropped",
            metadata: Some(&PARSE),
        },
        ParserPanic => {
            meaning: "the pinned parser panicked on a document; the panic is caught and reported, and the run is incomplete",
            metadata: Some(&PARSE),
        },
        InvalidSourceSpan => {
            meaning: "the parser returned a node whose byte span does not address the document; the parse is not trusted",
            metadata: Some(&PARSE),
        },
        ResolutionError => {
            meaning: "reference resolution failed internally; the run ends incomplete rather than reporting around the gap",
            metadata: Some(&RESOLUTION),
        },
        ResourceLimitExceeded => {
            meaning: "a named resource crossed its ceiling; the row carries the resource, the configured limit, and the observed lower bound",
            metadata: None,
        },
        OutputLimitExceeded => {
            meaning: "the serialized report would cross the machine-json-bytes ceiling; the run ends incomplete instead of shortening the findings",
            metadata: Some(&OUTPUT),
        },
        TooManyErrors => {
            meaning: "more distinct analysis errors accumulated than the retention ceiling; the lowest-keyed rows are kept and this sentinel stands for the rest",
            metadata: Some(&INTERNAL),
        },
        ReportConstructionFailed => {
            meaning: "the report could not be constructed or emitted; the run has no trustworthy output",
            metadata: Some(&OUTPUT),
        },
        SandboxViolation => {
            meaning: "the run breached its sandbox descriptor; the result is not trustworthy",
            metadata: Some(&INTERNAL),
        },
        TrustedTimeInvalid => {
            meaning: "a control that needs trusted time has no statement that verifies, absent or failing its binding; the run will not act on an unverified clock",
            metadata: Some(&CONFIGURATION),
        },
        InternalError => {
            meaning: "an engine invariant failed; this is a defect in Amiss, not in the input, and the run has no trustworthy result",
            metadata: Some(&INTERNAL),
        },
    }
    metadata pub(super) const fn route(self) -> Option<&'static AnalysisRoute>;
}

/// One typed analysis error's reportable detail: the code, the exact path
/// where the partition names one, the raw bytes of a name the report cannot
/// hold as text, and the crossing triple for a resource error. Field order
/// is the canonical error key, so the derived ordering is the wire's.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorDetail {
    pub code: AnalysisErrorCode,
    pub path: Option<crate::model::RepoPath>,
    pub path_bytes: Option<Vec<u8>>,
    pub resource: Option<(crate::controls::ResourceName, u64, u64)>,
}

/// One wire error row with its partition phase.
#[must_use]
pub fn error_row(detail: &ErrorDetail) -> AnalysisError<crate::model::RepoPath> {
    let phase = detail.resource.map_or_else(
        || {
            detail
                .code
                .route()
                .map_or(AnalysisPhase::Internal, |route| route.phase)
        },
        |(name, _limit, _observed)| name.phase(),
    );
    AnalysisError {
        phase,
        code: detail.code,
        description: detail.code.meaning().to_owned(),
        path: detail.path.clone(),
        path_bytes_hex: detail.path_bytes.as_ref().map(hex::encode),
        resource: detail.resource.map(|(name, _, _)| name),
        configured_limit: detail
            .resource
            .map(|(_, limit, _)| limit.min(i64::MAX.unsigned_abs())),
        observed_lower_bound: detail
            .resource
            .map(|(_, _, observed)| observed.min(i64::MAX.unsigned_abs())),
    }
}
