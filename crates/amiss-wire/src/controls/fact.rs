use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb, hj_serde};
use crate::model::{Oid, RepoPathText};
use crate::resolution::Target;

use super::{
    EligibleFindingKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, SourceConstruct, TargetKind, root,
};

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
pub struct TargetIntent {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "json_serde::deserialize_some"
    )]
    pub commit_oid: Option<Oid>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fragment_digest: Option<Digest>,
    pub kind: TargetIntentKind,
    pub path: RepoPathText,
    #[serde(deserialize_with = "Option::deserialize")]
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
pub struct FindingKeyInput {
    pub finding_kind: EligibleFindingKind,
    pub schema: FindingKeyInputSchema,
    pub scope: FindingScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MissingResolution {
    PathNotFound {
        path: RepoPathText,
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<RepoPathText>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "json_serde::deserialize_some"
        )]
        same_object_at: Option<Nullable<RepoPathText>>,
    },
    LineFragmentOutOfRange {
        path: RepoPathText,
    },
    HeadingAnchorNotFound {
        path: RepoPathText,
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<String>,
    },
    LabelNotDeclared,
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
pub struct Fact {
    pub schema: FactSchema,
    pub finding_kind: EligibleFindingKind,
    pub key_input: FindingKeyInput,
    pub evidence: FactEvidence,
}

/// Parses and validates one structural finding fact.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, or inconsistent
/// finding, key, resolution, and multiplicity values.
pub fn parse_fact(bytes: &[u8]) -> Result<Fact, Error> {
    root(bytes)?;
    let fact = de::deserialize_json(bytes)?;
    validate_fact("$", &fact)?;
    Ok(fact)
}

/// Produces one valid structural fact's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_fact`] enforces, or the
/// typed value cannot be serialized.
pub fn canonical_fact(fact: &Fact) -> Result<(Vec<u8>, Digest), Error> {
    validate_fact("$", fact)?;
    let bytes = serde_json_canonicalizer::to_vec(fact)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(FACT_DOMAIN, &bytes);
    Ok((bytes, digest))
}

pub(super) fn fact_digests(path: &str, fact: &Fact) -> Result<(Digest, Digest), Error> {
    validate_fact(path, fact)?;
    let key = hj_serde(FINDING_KEY_DOMAIN, |writer| {
        serde_json::to_writer(writer, &fact.key_input)
    })
    .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    let digest = hj_serde(FACT_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(fact, &mut writer)
    })
    .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    Ok((key, digest))
}

fn validate_fact(path: &str, fact: &Fact) -> Result<(), Error> {
    let resolution_kind = match &fact.evidence.resolution {
        StructuralResolution::Missing(_) => EligibleFindingKind::ExplicitTargetMissing,
        StructuralResolution::TypeMismatch { .. } => {
            EligibleFindingKind::ExplicitTargetTypeMismatch
        }
    };
    if fact.finding_kind != fact.key_input.finding_kind || fact.finding_kind != resolution_kind {
        return fail(path, ErrorKind::Inconsistent);
    }
    (fact.evidence.occurrence_multiplicity == 1)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::Inconsistent))
}
