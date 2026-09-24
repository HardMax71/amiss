use crate::envelope::document_digest;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{Document, Error, ErrorKind, fail};
use crate::model::Digest;
use crate::model::{Oid, RepoPathText};
use crate::resolution::Target;

use super::{EligibleFindingKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, TargetKind};
use crate::extraction::SourceConstruct;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum FindingKeyInputSchema {
    #[strum(serialize = "amiss/scanner-finding-key-input")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum FactSchema {
    #[strum(serialize = "amiss/scanner-fact")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ReferenceScopeKind {
    #[strum(serialize = "reference")]
    Reference,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum TargetIntentKind {
    #[strum(serialize = "repository-path")]
    RepositoryPath,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum OccurrenceKind {
    #[strum(serialize = "source-projection")]
    SourceProjection,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum FactEvidenceKind {
    #[strum(serialize = "reference")]
    Reference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIntent<P = RepoPathText> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_oid: Option<Oid>,
    pub fragment_digest: Option<Digest>,
    pub kind: TargetIntentKind,
    pub path: P,
    pub query_digest: Option<Digest>,
    pub target_kind: TargetKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingOccurrence {
    pub kind: OccurrenceKind,
    pub source_projection_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingScope {
    pub document: RepoPathText,
    pub kind: ReferenceScopeKind,
    pub normalized_target_intent: TargetIntent,
    pub occurrence: FindingOccurrence,
    pub source_construct: SourceConstruct,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingKeyInput<F = EligibleFindingKind, S = FindingScope> {
    pub finding_kind: F,
    pub schema: FindingKeyInputSchema,
    pub scope: S,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MissingResolution<P = RepoPathText> {
    HeadingAnchorNotFound {
        near: Option<String>,
        path: P,
    },
    LabelNotDeclared {},
    LineFragmentOutOfRange {
        path: P,
    },
    PathNotFound {
        near: Option<P>,
        path: P,
        #[serde(
            default = "Option::default",
            deserialize_with = "json_serde::deserialize_some",
            skip_serializing_if = "Option::is_none"
        )]
        same_object_at: Option<Nullable<P>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum StructuralResolution {
    Missing(MissingResolution),
    TypeMismatch { target: Target<RepoPathText> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactEvidence {
    pub kind: FactEvidenceKind,
    pub resolution: StructuralResolution,
    pub occurrence_multiplicity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact<K = FindingKeyInput, E = FactEvidence, F = EligibleFindingKind> {
    pub evidence: E,
    pub finding_kind: F,
    pub key_input: K,
    pub schema: FactSchema,
}

pub(super) fn fact_digests(path: &str, fact: &Fact) -> Result<(Digest, Digest), Error> {
    fact.validate()
        .map_err(|error| Error::new(path, error.kind))?;
    let key = {
        let mut writer = digest_io::IoWrapper(
            sha2::Sha256::new_with_prefix(FINDING_KEY_DOMAIN).chain_update([0_u8]),
        );
        serde_json::to_writer(&mut writer, &fact.key_input)
            .map(|()| Digest::from(writer.0.finalize().0))
    }
    .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    let digest = document_digest(FACT_DOMAIN, fact)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))?;
    Ok((key, digest))
}

impl Document for Fact {
    type Defect = Error;

    /// Checks that the finding kind, resolution, key and multiplicity agree.
    ///
    /// # Errors
    ///
    /// The public fields describe different structural findings or multiple occurrences.
    fn validate(&self) -> Result<(), Error> {
        let resolution_kind = match &self.evidence.resolution {
            StructuralResolution::Missing(_) => EligibleFindingKind::ExplicitTargetMissing,
            StructuralResolution::TypeMismatch { .. } => {
                EligibleFindingKind::ExplicitTargetTypeMismatch
            }
        };
        if self.finding_kind != self.key_input.finding_kind || self.finding_kind != resolution_kind
        {
            return fail("$", ErrorKind::Inconsistent);
        }
        (self.evidence.occurrence_multiplicity == 1)
            .then_some(())
            .ok_or_else(|| Error::new("$", ErrorKind::Inconsistent))
    }
}
