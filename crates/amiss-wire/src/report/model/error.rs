use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::controls::{AnalysisPhase, ResourceName};

use super::EvaluationUnavailableReason;

use crate::model::RepoPath;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisError<P = RepoPath> {
    pub code: AnalysisErrorCode,
    pub configured_limit: Option<u64>,
    pub description: String,
    pub observed_lower_bound: Option<u64>,
    pub path: Option<P>,
    pub path_bytes: Option<Vec<u8>>,
    pub phase: AnalysisPhase,
    pub resource: Option<ResourceName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::report) struct AnalysisRoute {
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
            meaning: "the profile is not observe, enforce-introduced, or enforce; pass one of those three to --profile",
            metadata: Some(&INVALID_PROFILE),
        },
        RequestUnreadable => {
            meaning: "the machine evaluation request bytes could not be read; nothing was evaluated, so resend the whole sealed request frame on stdin with nothing after it",
            metadata: Some(&REQUEST_UNREADABLE),
        },
        ConfigurationInvalid => {
            meaning: "a policy or control input violates its schema; correct the file the row names, since one unknown field or malformed value makes the whole file invalid rather than partly honored",
            metadata: Some(&CONFIGURATION),
        },
        DuplicateJsonKey => {
            meaning: "a JSON input repeats an object key; drop the duplicate, since strict parsing refuses the file rather than choosing one of the values",
            metadata: Some(&CONFIGURATION),
        },
        InvalidUtf8 => {
            meaning: "a JSON input carries bytes that are not UTF-8",
            metadata: Some(&CONFIGURATION),
        },
        InvalidJson => {
            meaning: "an input that must be JSON does not parse as strict JSON; fix the syntax in the file the row names",
            metadata: Some(&CONFIGURATION),
        },
        UnknownSchema => {
            meaning: "a JSON input declares a schema identifier this engine does not recognize; correct the identifier, or upgrade to a release that defines it",
            metadata: Some(&CONFIGURATION),
        },
        UnknownField => {
            meaning: "a JSON input carries a field its closed schema does not define; unknown fields refuse rather than pass through unread",
            metadata: Some(&CONFIGURATION),
        },
        NoncanonicalArray => {
            meaning: "a JSON input array violates its required canonical ordering or uniqueness; sort it and drop the repeated members",
            metadata: Some(&CONFIGURATION),
        },
        DigestMismatch => {
            meaning: "a digest carried by an input does not match the bytes it names; the input is stale or altered",
            metadata: Some(&CONFIGURATION),
        },
        ControlBindingMismatch => {
            meaning: "an external control is bound to a different repository, ref, or run identity than this evaluation; nothing is applied, so reissue the control against the identity under review or leave it out",
            metadata: Some(&CONFIGURATION),
        },
        ExceptionOverlap => {
            meaning: "accepted exception items select the same finding more than once; narrow the debt and waiver items until each finding is selected once, since overlap is refused rather than double-suppressed",
            metadata: Some(&CONFIGURATION),
        },
        UnsupportedCapability => {
            meaning: "a candidate document declares a reserved amiss: capability this engine does not implement; correct the declaration in the named document, or upgrade to a release that implements it",
            metadata: Some(&POLICY),
        },
        GitRepositoryUnavailable => {
            meaning: "the --repo path does not open as a Git repository of the declared object format; pass the checkout root from git rev-parse --show-toplevel and match --object-format to git rev-parse --show-object-format",
            metadata: Some(&GIT),
        },
        GitObjectMissing => {
            meaning: "a commit, tree, or blob the run needs is absent from the object store; fetch full history or name commits the store holds, and for a whole-tree scan pass --base $(git rev-parse HEAD) --index rather than the all-zero or empty-tree id",
            metadata: Some(&GIT),
        },
        GitObjectWrongKind => {
            meaning: "a Git object is not the kind its use requires, as when a named commit resolves to another type",
            metadata: Some(&GIT),
        },
        GitObjectUnreadable => {
            meaning: "a Git object exists but its bytes cannot be decoded; the object store is damaged, so check it with git fsck and restore it from a fresh clone",
            metadata: Some(&GIT),
        },
        GitIndexInvalid => {
            meaning: "the staged index file does not parse under the index grammar; remove .git/index and run git reset to rebuild it, or compare two commits instead of the index",
            metadata: Some(&GIT),
        },
        GitIndexOutsideRepository => {
            meaning: "GIT_INDEX_FILE names an index outside this repository's own git directory, and the scan reads nothing there; unset it, or run from the repository whose Git process set it",
            metadata: Some(&GIT),
        },
        GitIndexFormatUnsupported => {
            meaning: "the index uses Git's split or sparse format, which this reader does not expand; compare two commits instead, or turn the format off with git update-index --no-split-index, or git config index.sparse false and git sparse-checkout reapply",
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
            meaning: "a document's bytes cannot be decoded as its format requires; save the file as UTF-8 or fix the syntax its grammar refused, and only that document is unsupported, never the run",
            metadata: Some(&PARSE),
        },
        ParserError => {
            meaning: "the pinned parser failed on a document; this is an Amiss defect rather than a fault in the file, so report it as a bug with the named document",
            metadata: Some(&PARSE),
        },
        ParserPanic => {
            meaning: "the pinned parser panicked on a document; the panic is caught and the run is incomplete, so report it as an Amiss bug with the named document",
            metadata: Some(&PARSE),
        },
        InvalidSourceSpan => {
            meaning: "the parser returned a node whose byte span does not address the document; the parse is not trusted, so report it as an Amiss bug with the named document",
            metadata: Some(&PARSE),
        },
        ResolutionError => {
            meaning: "reference resolution failed internally; no input fixes this, so report it as an Amiss bug with the command line",
            metadata: Some(&RESOLUTION),
        },
        ResourceLimitExceeded => {
            meaning: "a named resource crossed its ceiling, and no option raises one, so the input has to come under it; the row carries the resource, the configured limit, and the observed lower bound",
            metadata: None,
        },
        OutputLimitExceeded => {
            meaning: "the serialized report would cross the machine-json-bytes ceiling; no option raises it and findings are never dropped to fit, so the repository is past what one report holds",
            metadata: Some(&OUTPUT),
        },
        TooManyErrors => {
            meaning: "more distinct analysis errors accumulated than the retention ceiling; the lowest-keyed rows are kept and this sentinel stands for the rest, so fix those and rerun for what is left",
            metadata: Some(&INTERNAL),
        },
        ReportConstructionFailed => {
            meaning: "the report could not be constructed or emitted; the run has no trustworthy output, so check that the output stream can still be written, then report it as an Amiss bug",
            metadata: Some(&OUTPUT),
        },
        SandboxViolation => {
            meaning: "the run breached its sandbox descriptor; the result is not trustworthy, so discard it and report an Amiss bug with the command line",
            metadata: Some(&INTERNAL),
        },
        TrustedTimeInvalid => {
            meaning: "a control that needs trusted time has no statement that verifies, absent or failing its binding; the run will not act on an unverified clock, so supply a statement bound to this run or drop the control",
            metadata: Some(&CONFIGURATION),
        },
        InternalError => {
            meaning: "an engine invariant failed; this is a defect in Amiss, not in the input, so discard the run and report it as a bug with the command line",
            metadata: Some(&INTERNAL),
        },
    }
    metadata pub(in crate::report) const fn route(self) -> Option<&'static AnalysisRoute>;
}
