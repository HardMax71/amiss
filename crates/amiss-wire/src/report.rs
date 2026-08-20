mod error;
mod failure;
mod fatal;
mod finding;
mod sandbox;

use crate::json::Value;

pub use error::{AnalysisErrorCode, ErrorDetail, error_row_value};
pub use failure::{
    EngineProvenance, adapter_contract, engine_block, invocation_failure_envelope,
    invocation_failure_wire, unavailable_evaluation_envelope, unavailable_evaluation_wire,
};
pub use fatal::FatalSerializer;
pub use finding::{Disposition, FindingKind, FindingScope, FixKind, IntentKind};
pub use sandbox::sandbox_descriptor;

pub const ENGINE_CONTRACT: &str = "amiss/scanner";

/// The exact `machine-json-bytes` reservation: the report wire, canonical
/// envelope plus the trailing newline, never exceeds this.
pub const MACHINE_JSON_BYTES: u64 = 268_435_456;

/// The evaluator-managed memory ceiling asserted by the sandbox descriptor.
pub const EVALUATOR_MANAGED_MEMORY_BYTES: u64 = 1_073_741_824;

/// The private temporary-storage ceiling asserted by the sandbox descriptor.
pub const PRIVATE_TEMPORARY_STORAGE_BYTES: u64 = 67_108_864;

/// The watchdog ceiling asserted by the sandbox descriptor.
pub const WATCHDOG_MILLISECONDS: u64 = 120_000;

/// The fatal serializer's fixed scratch allowance: the staging buffer it
/// reserves up front plus every transient allocation one streaming emission
/// may make. The E0 maximal golden proves emission stays inside it.
pub const FATAL_SCRATCH_BYTES: usize = 65_536;

pub const ENGINE_DOMAIN: &str = "amiss/scanner-engine";
pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const PAYLOAD_SCHEMA: &str = "amiss/scanner-report-payload";
/// The wire's own version: a reshape mints the next major, as a major release.
pub const COMPATIBILITY: &str = "1";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "amiss/scanner-adapter-contract";
pub const BUILT_IN_POLICY: &str = "scanner-policy-defaults";
pub const SANDBOX_SCHEMA: &str = "amiss/scanner-sandbox-profile";

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn string(value: &str) -> Value {
    Value::String(value.into())
}

impl AnalysisErrorCode {
    /// One fixed engine-owned sentence per code: what blocked the run and how
    /// to unblock it. Printed by the human projection as a `note` line and
    /// rendered into the documentation from this same text.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::InvalidInvocation => {
                "the command line does not match the closed grammar; each documented option appears at most once and nothing else is accepted"
            }
            Self::InvalidEvent => {
                "the declared repository, ref, or default-branch identity is not in canonical form; pass a lowercase owner and name and full refs/heads/ references"
            }
            Self::InvalidProfile => "the profile is not observe, enforce-introduced, or enforce",
            Self::RequestUnreadable => {
                "the machine evaluation request bytes could not be read; nothing was evaluated"
            }
            Self::ConfigurationInvalid => {
                "a policy or control input violates its schema; one unknown field or malformed value makes the whole file invalid rather than partly honored"
            }
            Self::DuplicateJsonKey => {
                "a JSON input repeats an object key; strict parsing refuses the file instead of choosing one of the values"
            }
            Self::InvalidUtf8 => "a JSON input carries bytes that are not UTF-8",
            Self::InvalidJson => "an input that must be JSON does not parse as strict JSON",
            Self::UnknownSchema => {
                "a JSON input declares a schema identifier this engine does not recognize"
            }
            Self::UnknownField => {
                "a JSON input carries a field its closed schema does not define; unknown fields refuse rather than pass through unread"
            }
            Self::NoncanonicalArray => {
                "a JSON input array violates its required canonical ordering or uniqueness"
            }
            Self::DigestMismatch => {
                "a digest carried by an input does not match the bytes it names; the input is stale or altered"
            }
            Self::ControlBindingMismatch => {
                "an external control is bound to a different repository, ref, or run identity than this evaluation; nothing is applied and the run ends incomplete"
            }
            Self::ExceptionOverlap => {
                "accepted exception items select the same finding more than once; overlap ends evaluation incomplete instead of double-suppressing"
            }
            Self::UnsupportedCapability => {
                "a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim"
            }
            Self::GitRepositoryUnavailable => {
                "the --repo path does not open as a Git repository of the declared object format"
            }
            Self::GitObjectMissing => {
                "a commit, tree, or blob the run needs is absent from the object store; fetch full history or name commits the store holds"
            }
            Self::GitObjectWrongKind => {
                "a Git object is not the kind its use requires, as when a named commit resolves to another type"
            }
            Self::GitObjectUnreadable => "a Git object exists but its bytes cannot be decoded",
            Self::GitIndexInvalid => "the staged index file does not parse under the index grammar",
            Self::GitIndexUnmerged => {
                "the index holds unmerged conflict entries, so no single staged state exists; finish or abort the merge before checking the index"
            }
            Self::GitIntentToAdd => {
                "the index holds an intent-to-add entry whose content is not staged; stage the file or drop the intent entry before checking the index"
            }
            Self::GitSnapshotChanged => {
                "the staged index changed while the run was reading it; rerun when the repository is quiet"
            }
            Self::UnrepresentablePath => {
                "a tree or index name is outside the path grammar, a backslash, a NUL, or a dot segment; the exact bytes are disclosed as hex"
            }
            Self::DocumentInvalid => {
                "a discovered document's bytes cannot be decoded as its format requires; the run refuses instead of skipping the file and passing"
            }
            Self::ParserError => {
                "the pinned parser failed on a document; the document is named and the run is incomplete rather than the file silently dropped"
            }
            Self::ParserPanic => {
                "the pinned parser panicked on a document; the panic is caught and reported, and the run is incomplete"
            }
            Self::InvalidSourceSpan => {
                "the parser returned a node whose byte span does not address the document; the parse is not trusted"
            }
            Self::ResolutionError => {
                "reference resolution failed internally; the run ends incomplete rather than reporting around the gap"
            }
            Self::ResourceLimitExceeded => {
                "a named resource crossed its ceiling; the row carries the resource, the configured limit, and the observed lower bound"
            }
            Self::OutputLimitExceeded => {
                "the serialized report would cross the machine-json-bytes ceiling; the run ends incomplete instead of shortening the findings"
            }
            Self::TooManyErrors => {
                "more distinct analysis errors accumulated than the retention ceiling; the lowest-keyed rows are kept and this sentinel stands for the rest"
            }
            Self::ReportConstructionFailed => {
                "the report could not be constructed or emitted; the run has no trustworthy output"
            }
            Self::SandboxViolation => {
                "the run breached its sandbox descriptor; the result is not trustworthy"
            }
            Self::TrustedTimeInvalid => {
                "a control that needs trusted time has no statement that verifies, absent or failing its binding; the run will not act on an unverified clock"
            }
            Self::InternalError => {
                "an engine invariant failed; this is a defect in Amiss, not in the input, and the run has no trustworthy result"
            }
        }
    }

    /// Fixed phase for non-resource codes; `RESOURCE_LIMIT_EXCEEDED` takes its
    /// phase from the resource partition and has none here.
    #[must_use]
    pub const fn fixed_phase(self) -> Option<&'static str> {
        match self {
            Self::InvalidInvocation
            | Self::InvalidEvent
            | Self::InvalidProfile
            | Self::RequestUnreadable => Some("invocation"),
            Self::ConfigurationInvalid
            | Self::DuplicateJsonKey
            | Self::InvalidUtf8
            | Self::InvalidJson
            | Self::UnknownSchema
            | Self::UnknownField
            | Self::NoncanonicalArray
            | Self::DigestMismatch
            | Self::ControlBindingMismatch
            | Self::ExceptionOverlap
            | Self::TrustedTimeInvalid => Some("configuration"),
            Self::GitRepositoryUnavailable
            | Self::GitObjectMissing
            | Self::GitObjectWrongKind
            | Self::GitObjectUnreadable
            | Self::GitIndexInvalid
            | Self::GitIndexUnmerged
            | Self::GitIntentToAdd
            | Self::GitSnapshotChanged
            | Self::UnrepresentablePath => Some("git"),
            Self::DocumentInvalid
            | Self::ParserError
            | Self::ParserPanic
            | Self::InvalidSourceSpan => Some("parse"),
            Self::ResolutionError => Some("resolution"),
            Self::UnsupportedCapability => Some("policy"),
            Self::OutputLimitExceeded | Self::ReportConstructionFailed => Some("output"),
            Self::SandboxViolation | Self::TooManyErrors | Self::InternalError => Some("internal"),
            Self::ResourceLimitExceeded => None,
        }
    }

    pub(super) const fn evaluation_reason(self) -> Option<&'static str> {
        match self {
            Self::InvalidInvocation => Some("invalid-invocation"),
            Self::InvalidEvent => Some("invalid-event"),
            Self::InvalidProfile => Some("invalid-profile"),
            Self::RequestUnreadable => Some("request-unreadable"),
            Self::ConfigurationInvalid
            | Self::DuplicateJsonKey
            | Self::InvalidUtf8
            | Self::InvalidJson
            | Self::UnknownSchema
            | Self::UnknownField
            | Self::NoncanonicalArray
            | Self::DigestMismatch
            | Self::ControlBindingMismatch
            | Self::ExceptionOverlap
            | Self::UnsupportedCapability
            | Self::GitRepositoryUnavailable
            | Self::GitObjectMissing
            | Self::GitObjectWrongKind
            | Self::GitObjectUnreadable
            | Self::GitIndexInvalid
            | Self::GitIndexUnmerged
            | Self::GitIntentToAdd
            | Self::GitSnapshotChanged
            | Self::UnrepresentablePath
            | Self::DocumentInvalid
            | Self::ParserError
            | Self::ParserPanic
            | Self::InvalidSourceSpan
            | Self::ResolutionError
            | Self::ResourceLimitExceeded
            | Self::OutputLimitExceeded
            | Self::TooManyErrors
            | Self::ReportConstructionFailed
            | Self::SandboxViolation
            | Self::TrustedTimeInvalid
            | Self::InternalError => None,
        }
    }
}

impl FindingKind {
    /// The closed key-scope assignment.
    #[must_use]
    pub const fn scope(self) -> FindingScope {
        match self {
            Self::ExplicitTargetMissing | Self::ExplicitTargetTypeMismatch => {
                FindingScope::Reference
            }
            Self::InvalidReference
            | Self::TargetDeclaredUntracked
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::ExplicitReferenceRemoved
            | Self::ObservationCorrelationAmbiguous => FindingScope::Observation,
            Self::UnsupportedDocumentFormat
            | Self::DocumentRemoved
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::UnlinkedDocument => FindingScope::Document,
            Self::UnsupportedCapability
            | Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid
            | Self::ClaimBroken
            | Self::ClaimTargetMissing => FindingScope::Control,
        }
    }
}

impl FindingKind {
    #[must_use]
    pub const fn evidence_class(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference
            | Self::ClaimBroken
            | Self::ClaimTargetMissing => "deterministic-structural",
            Self::UnsupportedCapability
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope => "unsupported",
            Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged => "impact-observation",
            Self::TargetDeclaredUntracked
            | Self::ExplicitReferenceRemoved
            | Self::DocumentRemoved
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => "coverage-boundary",
            Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => "control-plane",
        }
    }
}

impl FindingKind {
    /// One fixed engine-owned sentence per kind: what the finding means and
    /// what to do about it. The human projection prints it as a `note` line
    /// and the documentation renders the same text, so the sentence a reader
    /// meets in a CI log is the sentence the book teaches.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing => {
                "a reference names a repository path, a line range inside one, or a heading anchor no known renderer publishes; restore the target or correct the link"
            }
            Self::ExplicitTargetTypeMismatch => {
                "the referenced path exists as a different kind than the reference promises, as when a trailing slash names a regular file; make the spelling match the target"
            }
            Self::InvalidReference => {
                "the destination cannot name a repository target: it escapes the repository or carries a backslash, an encoded separator, or control bytes; fix the destination"
            }
            Self::TargetDeclaredUntracked => {
                "a reference names a path a tracked ignore file names literally, so the repository declares it does not keep that target and no tree can answer for the link; the reference is recorded and counted, never cleared"
            }
            Self::UnsupportedReferenceSemantics => {
                "the reference uses semantics this run did not evaluate: a site route, a protocol-relative destination, a query string, a destination that needs a document attribute this run does not evaluate, or a fragment on a target it cannot answer for; the unchecked part is declared instead of guessed"
            }
            Self::UnsupportedDocumentFormat => {
                "a document this run discovered has no parser in this engine, whether a markup it does not read or a policy include; it is counted, and its content is never scanned"
            }
            Self::UnsupportedTargetKind => {
                "the reference resolves to a symlink or submodule, which Amiss does not follow; the boundary is declared instead of crossed"
            }
            Self::UnsupportedVersionScope => {
                "a forge URL names this repository at another version, a different branch, tag, or commit; only the candidate version is read, so the link is recognized and left unresolved"
            }
            Self::UnsupportedCapability => {
                "a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim"
            }
            Self::DependencyChangedSubjectUnchanged => {
                "the referenced content changed and the block citing it did not; a reason for a person to reread the prose, never a machine verdict that it is wrong"
            }
            Self::DependencyAndSubjectCochanged => {
                "the referenced content and the block citing it changed together, the shape of a maintained page; recorded with nothing to act on"
            }
            Self::SubjectChanged => {
                "the block holding the reference changed while its target did not; recorded so prose moving over an unchanged dependency stays visible"
            }
            Self::ExplicitReferenceRemoved => {
                "a reference that existed in the base is gone from the candidate; the removal is recorded as a fact, never treated as evidence that the edit was wrong"
            }
            Self::DocumentRemoved => {
                "a scanned document left the tree; recorded so the disappearance is a stated fact rather than a silent one"
            }
            Self::OpaqueMdxRegion => {
                "an MDX expression region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place"
            }
            Self::OpaqueHtmlRegion => {
                "a raw HTML region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place"
            }
            Self::ObservationCorrelationAmbiguous => {
                "an occurrence has more than one plausible counterpart across the comparison; Amiss never chooses by input order, so the match is recorded as undecided"
            }
            Self::UnlinkedDocument => {
                "a scanned document from which zero references were extracted; despite the name, it claims nothing about inbound links from other pages"
            }
            Self::PolicyWeakened => {
                "the candidate loosens its own repository policy, dropping an include, a protected path, or a raised disposition; loosening the rules is reported under the rules being loosened"
            }
            Self::CoverageReduced => {
                "a protected path is gone or not a scannable document while its protection stands; restore it or amend the protection in a reviewed change"
            }
            Self::ControlPlaneChanged => {
                "a floor-protected control path is not the identical present blob on both sides, in mode and content; the floor exists so control edits are always visible"
            }
            Self::DebtWorsened => {
                "the finding an accepted debt item names no longer matches the recorded fact; debt tolerates exactly the recorded state, so any drift fails"
            }
            Self::DebtExpired => {
                "trusted time reached a debt item's expiry while its finding persists; fix the finding or renew the debt in a reviewed change"
            }
            Self::WaiverInvalid => {
                "a waiver item cannot apply, expired against trusted time or issued outside the floor's authority; an invalid waiver suppresses nothing"
            }
            Self::ClaimBroken => {
                "a value claim's target line no longer says what the document claims it says; update the claim or the target so the two agree"
            }
            Self::ClaimTargetMissing => {
                "a value claim names a target line no regular file in the candidate can answer; point the claim at a tracked file and a line inside it"
            }
        }
    }

    #[must_use]
    pub const fn invariant_class(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference
            | Self::ClaimBroken
            | Self::ClaimTargetMissing => "ratcheted",
            Self::UnsupportedCapability => "analysis-integrity",
            Self::TargetDeclaredUntracked
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::ExplicitReferenceRemoved
            | Self::DocumentRemoved
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => "advisory",
            Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => "absolute",
        }
    }
}
