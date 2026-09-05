use amiss_wire::report::FindingKind;
use amiss_wire::resolution::Resolution;

mod claims;
mod control;
mod debt;
mod documents;
mod finding;
mod model;
mod projections;
mod references;
mod run;
mod tests;
mod waiver;

pub(crate) use claims::source_multiplicities;
pub use claims::{ClaimGroup, claim_groups};
pub use control::GovernedSeed;
pub(crate) use finding::fact;
use model::FindingKeyScope;
pub use model::{
    Attribution, DebtApplied, DocumentInput, DocumentSide, Finding, FindingFact, FindingFix,
    Location, LocationSide, PolicyStep, WaiverApplied,
};
pub use references::structural_facts;
use run::candidate_digest_of;
pub(crate) use run::{GovernedInputs, evaluate_with_site};
pub use run::{evaluate, evaluate_with_policy};

pub const FINDING_KEY_SCHEMA: &str = "amiss/scanner-finding-key-input";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_SCHEMA: &str = "amiss/scanner-fact";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

pub(super) struct ResolutionKinds {
    pub(super) structural: Option<FindingKind>,
    pub(super) boundary: Option<FindingKind>,
}

pub(super) const fn resolution_kinds(resolution: &crate::resolve::Resolution) -> ResolutionKinds {
    match resolution {
        Resolution::Missing(_) => ResolutionKinds {
            structural: Some(FindingKind::ExplicitTargetMissing),
            boundary: None,
        },
        Resolution::TypeMismatch { .. } => ResolutionKinds {
            structural: Some(FindingKind::ExplicitTargetTypeMismatch),
            boundary: None,
        },
        Resolution::Invalid { .. } => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::InvalidReference),
        },
        Resolution::UnsupportedSemantics(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedReferenceSemantics),
        },
        Resolution::UnsupportedVersion { .. } => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedVersionScope),
        },
        Resolution::UnsupportedTarget(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::UnsupportedTargetKind),
        },
        Resolution::DeclaredUntracked(_) => ResolutionKinds {
            structural: None,
            boundary: Some(FindingKind::TargetDeclaredUntracked),
        },
        Resolution::Resolved { .. } | Resolution::External { .. } => ResolutionKinds {
            structural: None,
            boundary: None,
        },
    }
}
