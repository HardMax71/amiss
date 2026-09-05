use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, IntoEnumIterator, IntoStaticStr};

use super::model::{EvidenceClass, InvariantClass};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumIter, Serialize, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum IntentKind {
    RepositoryPath,
    SameRepositoryGithub,
    SameRepositoryGitlab,
    SameRepositoryGitea,
    SameRepositoryBitbucketCloud,
    SameRepositoryBitbucketDataCenter,
    ExternalUrl,
    SiteRoute,
    Label,
    Unsupported,
}

/// The four finding scopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingScope {
    Reference,
    Observation,
    Document,
    Control,
}

/// The closed disposition values a policy step can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Record,
    Warn,
    Fail,
}

/// The immutable policy and report classification assigned to one finding
/// kind by the built-in taxonomy.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FindingMetadata {
    pub scope: FindingScope,
    pub evidence_class: EvidenceClass,
    pub invariant_class: InvariantClass,
    pub observe_disposition: Disposition,
    pub enforce_disposition: Disposition,
}

static RATCHETED_REFERENCE: FindingMetadata = FindingMetadata {
    scope: FindingScope::Reference,
    evidence_class: EvidenceClass::DeterministicStructural,
    invariant_class: InvariantClass::Ratcheted,
    observe_disposition: Disposition::Warn,
    enforce_disposition: Disposition::Fail,
};
static RATCHETED_OBSERVATION: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::DeterministicStructural,
    invariant_class: InvariantClass::Ratcheted,
    observe_disposition: Disposition::Warn,
    enforce_disposition: Disposition::Fail,
};
static ABSOLUTE_OBSERVATION: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::DeterministicStructural,
    invariant_class: InvariantClass::Absolute,
    observe_disposition: Disposition::Warn,
    enforce_disposition: Disposition::Fail,
};
static RATCHETED_CONTROL: FindingMetadata = FindingMetadata {
    scope: FindingScope::Control,
    evidence_class: EvidenceClass::DeterministicStructural,
    invariant_class: InvariantClass::Ratcheted,
    observe_disposition: Disposition::Warn,
    enforce_disposition: Disposition::Fail,
};
static COVERAGE_OBSERVATION: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::CoverageBoundary,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Record,
    enforce_disposition: Disposition::Record,
};
static COVERAGE_DOCUMENT: FindingMetadata = FindingMetadata {
    scope: FindingScope::Document,
    evidence_class: EvidenceClass::CoverageBoundary,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Record,
    enforce_disposition: Disposition::Record,
};
static UNSUPPORTED_OBSERVATION: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::Unsupported,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Record,
    enforce_disposition: Disposition::Record,
};
static UNSUPPORTED_DOCUMENT: FindingMetadata = FindingMetadata {
    scope: FindingScope::Document,
    evidence_class: EvidenceClass::Unsupported,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Record,
    enforce_disposition: Disposition::Record,
};
static UNSUPPORTED_CONTROL: FindingMetadata = FindingMetadata {
    scope: FindingScope::Control,
    evidence_class: EvidenceClass::Unsupported,
    invariant_class: InvariantClass::AnalysisIntegrity,
    observe_disposition: Disposition::Fail,
    enforce_disposition: Disposition::Fail,
};
static IMPACT_WARNING: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::ImpactObservation,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Warn,
    enforce_disposition: Disposition::Warn,
};
static IMPACT_RECORD: FindingMetadata = FindingMetadata {
    scope: FindingScope::Observation,
    evidence_class: EvidenceClass::ImpactObservation,
    invariant_class: InvariantClass::Advisory,
    observe_disposition: Disposition::Record,
    enforce_disposition: Disposition::Record,
};
static CONTROL_PLANE: FindingMetadata = FindingMetadata {
    scope: FindingScope::Control,
    evidence_class: EvidenceClass::ControlPlane,
    invariant_class: InvariantClass::Absolute,
    observe_disposition: Disposition::Fail,
    enforce_disposition: Disposition::Fail,
};

declare_taxonomy! {
    /// The complete closed finding taxonomy, in schema declaration order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumIter, IntoStaticStr, Serialize, Deserialize)]
    #[strum(serialize_all = "kebab-case")]
    #[serde(rename_all = "kebab-case")]
    pub enum FindingKind {
        ExplicitTargetMissing => {
            meaning: "a reference names a repository path, a line range inside one, or a heading anchor no known renderer publishes; restore the target or correct the link",
            metadata: &RATCHETED_REFERENCE,
        },
        ExplicitTargetTypeMismatch => {
            meaning: "the referenced path exists as a different kind than the reference promises, as when a trailing slash names a regular file; make the spelling match the target",
            metadata: &RATCHETED_REFERENCE,
        },
        InvalidReference => {
            meaning: "the destination cannot name a repository target: it escapes the repository or carries a backslash, an encoded separator, or control bytes; fix the destination",
            metadata: &RATCHETED_OBSERVATION,
        },
        TargetDeclaredUntracked => {
            meaning: "a reference names a path a tracked ignore file names literally, so the repository declares it does not keep that target and no tree can answer for the link; the reference is recorded and counted, never cleared",
            metadata: &COVERAGE_OBSERVATION,
        },
        UnsupportedReferenceSemantics => {
            meaning: "the reference uses semantics this run did not evaluate: a site route, a protocol-relative destination, a query string the selected grammar does not recognize, a destination that needs a document attribute this run does not evaluate, or a fragment on a target it cannot answer for; the unchecked part is declared instead of guessed",
            metadata: &UNSUPPORTED_OBSERVATION,
        },
        UnsupportedDocumentFormat => {
            meaning: "a document this run discovered has no parser in this engine, whether a markup it does not read or a policy include; it is counted, and its content is never scanned",
            metadata: &UNSUPPORTED_DOCUMENT,
        },
        UnsupportedTargetKind => {
            meaning: "the reference resolves to a symlink or submodule, which Amiss does not follow; the boundary is declared instead of crossed",
            metadata: &UNSUPPORTED_OBSERVATION,
        },
        UnsupportedVersionScope => {
            meaning: "a forge URL names this repository at another named version or an exact commit whose required objects are unavailable; use the candidate ref, or make the exact commit available",
            metadata: &UNSUPPORTED_OBSERVATION,
        },
        UnsupportedCapability => {
            meaning: "a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim",
            metadata: &UNSUPPORTED_CONTROL,
        },
        DependencyChangedSubjectUnchanged => {
            meaning: "the referenced content changed and the block citing it did not; a reason for a person to reread the prose, never a machine verdict that it is wrong",
            metadata: &IMPACT_WARNING,
        },
        DependencyAndSubjectCochanged => {
            meaning: "the referenced content and the block citing it changed together, the shape of a maintained page; recorded with nothing to act on",
            metadata: &IMPACT_RECORD,
        },
        SubjectChanged => {
            meaning: "the block holding the reference changed while its target did not; recorded so prose moving over an unchanged dependency stays visible",
            metadata: &IMPACT_RECORD,
        },
        ExplicitReferenceRemoved => {
            meaning: "a reference that existed in the base is gone from the candidate; the removal is recorded as a fact, never treated as evidence that the edit was wrong",
            metadata: &COVERAGE_OBSERVATION,
        },
        DocumentRemoved => {
            meaning: "a scanned document left the tree; recorded so the disappearance is a stated fact rather than a silent one",
            metadata: &COVERAGE_DOCUMENT,
        },
        OpaqueMdxRegion => {
            meaning: "an MDX expression region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place",
            metadata: &COVERAGE_DOCUMENT,
        },
        OpaqueHtmlRegion => {
            meaning: "a raw HTML region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place",
            metadata: &COVERAGE_DOCUMENT,
        },
        ObservationCorrelationAmbiguous => {
            meaning: "an occurrence has more than one plausible counterpart across the comparison; Amiss never chooses by input order, so the match is recorded as undecided",
            metadata: &COVERAGE_OBSERVATION,
        },
        UnlinkedDocument => {
            meaning: "a scanned structured document inside a complete site build's source root is unreachable from every rendered navigation entrypoint; link the page from rendered navigation or keep non-page material outside that root",
            metadata: &COVERAGE_DOCUMENT,
        },
        SiteBuildDefect => {
            meaning: "a complete site build reports a route with conflicting owners or a redirect whose declared terminal route or anchor is not uniquely published; repair the route table or its available routing source",
            metadata: &ABSOLUTE_OBSERVATION,
        },
        PolicyWeakened => {
            meaning: "the candidate loosens its own repository policy, dropping an include, a protected path, a projection assertion, or a raised disposition; loosening the rules is reported under the rules being loosened",
            metadata: &CONTROL_PLANE,
        },
        CoverageReduced => {
            meaning: "a protected path is gone or not a scannable document while its protection stands; restore it or amend the protection in a reviewed change",
            metadata: &CONTROL_PLANE,
        },
        ControlPlaneChanged => {
            meaning: "a floor-protected control path is not the identical present blob on both sides, in mode and content; the floor exists so control edits are always visible",
            metadata: &CONTROL_PLANE,
        },
        DebtWorsened => {
            meaning: "the finding an accepted debt item names no longer matches the recorded fact; debt tolerates exactly the recorded state, so any drift fails",
            metadata: &CONTROL_PLANE,
        },
        DebtExpired => {
            meaning: "trusted time reached a debt item's expiry while its finding persists; fix the finding or renew the debt in a reviewed change",
            metadata: &CONTROL_PLANE,
        },
        WaiverInvalid => {
            meaning: "a waiver item cannot apply, expired against trusted time or issued outside the floor's authority; an invalid waiver suppresses nothing",
            metadata: &CONTROL_PLANE,
        },
        ClaimBroken => {
            meaning: "a value claim's target line no longer says what the document claims it says; update the claim or the target so the two agree",
            metadata: &RATCHETED_CONTROL,
        },
        ClaimTargetMissing => {
            meaning: "a value claim names a target line no regular file in the candidate can answer; point the claim at a tracked file and a line inside it",
            metadata: &RATCHETED_CONTROL,
        },
        ProjectionDrift => {
            meaning: "a policy-owned projection cannot prove that its visible code block equals its selected repository source; restore its unique sink and source or make their projected bytes agree",
            metadata: &RATCHETED_CONTROL,
        },
    }
    metadata pub const fn metadata(self) -> &'static FindingMetadata;
}

impl FindingKind {
    /// The first policy-step result for a candidate fact under
    /// `scanner-policy-defaults`, per profile.
    #[must_use]
    pub const fn built_in_disposition(self, profile: crate::controls::Profile) -> Disposition {
        let metadata = self.metadata();
        if profile.enforces() {
            metadata.enforce_disposition
        } else {
            metadata.observe_disposition
        }
    }
}

declare_meaningful_enum! {
    /// The closed set of machine-applicable rewrites the engine can prove. A
    /// finding carries at most one, and every producer names its kind here.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum FixKind {
        ClaimValueRewrite => "replace the definition so the claim expects the target's current line",
        AnchorRespelling => "replace the fragment with the one published anchor it matches apart from case and separator style",
        PathRespelling => "replace the path with the one tracked spelling it matches apart from case",
    }
}
