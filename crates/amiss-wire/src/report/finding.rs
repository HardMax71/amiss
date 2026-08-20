use strum::{AsRefStr, EnumIter, IntoEnumIterator, IntoStaticStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum IntentKind {
    RepositoryPath,
    SameRepositoryGithub,
    SameRepositoryGitlab,
    SameRepositoryGitea,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum Disposition {
    Record,
    Warn,
    Fail,
}

/// The complete closed finding taxonomy, in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum FindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
    InvalidReference,
    TargetDeclaredUntracked,
    UnsupportedReferenceSemantics,
    UnsupportedDocumentFormat,
    UnsupportedTargetKind,
    UnsupportedVersionScope,
    UnsupportedCapability,
    DependencyChangedSubjectUnchanged,
    DependencyAndSubjectCochanged,
    SubjectChanged,
    ExplicitReferenceRemoved,
    DocumentRemoved,
    OpaqueMdxRegion,
    OpaqueHtmlRegion,
    ObservationCorrelationAmbiguous,
    UnlinkedDocument,
    PolicyWeakened,
    CoverageReduced,
    ControlPlaneChanged,
    DebtWorsened,
    DebtExpired,
    WaiverInvalid,
    ClaimBroken,
    ClaimTargetMissing,
}

impl FindingKind {
    /// Every finding kind in schema declaration order.
    #[must_use]
    pub fn all() -> impl ExactSizeIterator<Item = Self> {
        Self::iter()
    }

    /// The first policy-step result for a candidate fact under
    /// `scanner-policy-defaults`, per profile.
    #[must_use]
    pub const fn built_in_disposition(self, profile: crate::controls::Profile) -> Disposition {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference
            | Self::ClaimBroken
            | Self::ClaimTargetMissing => {
                if profile.enforces() {
                    Disposition::Fail
                } else {
                    Disposition::Warn
                }
            }
            Self::UnsupportedCapability
            | Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => Disposition::Fail,
            Self::DependencyChangedSubjectUnchanged => Disposition::Warn,
            Self::TargetDeclaredUntracked
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::ExplicitReferenceRemoved
            | Self::DocumentRemoved
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => Disposition::Record,
        }
    }
}

/// The closed set of machine-applicable rewrites the engine can prove. A
/// finding carries at most one, and every producer names its kind here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixKind {
    ClaimValueRewrite,
    AnchorRespelling,
    PathRespelling,
}

impl FixKind {
    /// One fixed engine-owned sentence per rewrite: what applying it does.
    /// The wire carries this text as the fix's `description`.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::ClaimValueRewrite => {
                "replace the definition so the claim expects the target's current line"
            }
            Self::AnchorRespelling => {
                "replace the fragment with the one published anchor it matches apart from case and separator style"
            }
            Self::PathRespelling => {
                "replace the path with the one tracked spelling it matches apart from case"
            }
        }
    }
}
