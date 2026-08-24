use crate::digest::Digest;
use crate::model::{Oid, RepoPathText};
use crate::resolution::Resolution;

use super::{EligibleFindingKind, SourceConstruct, TargetKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetIntent {
    pub commit_oid: Option<Oid>,
    pub path: RepoPathText,
    pub target_kind: TargetKind,
    pub query_digest: Option<Digest>,
    pub fragment_digest: Option<Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingScope {
    pub document: RepoPathText,
    pub source_construct: SourceConstruct,
    pub normalized_target_intent: TargetIntent,
    pub source_projection_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingKeyInput {
    pub finding_kind: EligibleFindingKind,
    pub scope: FindingScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    key_input: FindingKeyInput,
    resolution: Resolution<RepoPathText>,
}

impl Fact {
    /// Builds a structural fact only when the key kind and resolution family agree.
    /// Non-structural resolution families are not eligible for control items.
    #[must_use]
    pub fn new(key_input: FindingKeyInput, resolution: Resolution<RepoPathText>) -> Option<Self> {
        let expected = match &resolution {
            Resolution::Missing(_) => EligibleFindingKind::ExplicitTargetMissing,
            Resolution::TypeMismatch(_) => EligibleFindingKind::ExplicitTargetTypeMismatch,
            Resolution::Resolved(_)
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion(_)
            | Resolution::Invalid(_)
            | Resolution::External(_) => return None,
        };
        (key_input.finding_kind == expected).then_some(Self {
            key_input,
            resolution,
        })
    }

    /// The finding kind fixed by the validated key and resolution family.
    #[must_use]
    pub const fn finding_kind(&self) -> EligibleFindingKind {
        self.key_input.finding_kind
    }

    /// The canonical finding-key preimage embedded in this fact.
    #[must_use]
    pub const fn key_input(&self) -> &FindingKeyInput {
        &self.key_input
    }

    /// The structural resolution evidence embedded in this fact.
    #[must_use]
    pub const fn resolution(&self) -> &Resolution<RepoPathText> {
        &self.resolution
    }
}
