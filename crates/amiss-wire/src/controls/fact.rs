use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;
use crate::model::{Oid, RepoPathText};
use crate::resolution::Target;

use super::{EligibleFindingKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, SourceConstruct, TargetKind};

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
#[serde(remote = "Self", deny_unknown_fields)]
pub struct TargetIntent<P = RepoPathText> {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "json_serde::deserialize_some"
    )]
    pub commit_oid: Option<Oid>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fragment_digest: Option<Digest>,
    pub kind: TargetIntentKind,
    pub path: P,
    #[serde(deserialize_with = "Option::deserialize")]
    pub query_digest: Option<Digest>,
    pub target_kind: TargetKind,
}

impl<P: Serialize> Serialize for TargetIntent<P> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for TargetIntent<P> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct FindingOccurrence {
    pub kind: OccurrenceKind,
    pub source_projection_digest: Digest,
}

impl Serialize for FindingOccurrence {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for FindingOccurrence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct FindingScope {
    pub document: RepoPathText,
    pub kind: ReferenceScopeKind,
    pub normalized_target_intent: TargetIntent,
    pub occurrence: FindingOccurrence,
    pub source_construct: SourceConstruct,
}

impl Serialize for FindingScope {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for FindingScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    remote = "Self",
    deny_unknown_fields,
    bound(deserialize = "F: Deserialize<'de>, S: Deserialize<'de>")
)]
pub struct FindingKeyInput<F = EligibleFindingKind, S = FindingScope> {
    pub finding_kind: F,
    pub schema: FindingKeyInputSchema,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub scope: S,
}

impl<F: Serialize, S: Serialize> Serialize for FindingKeyInput<F, S> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, F: Deserialize<'de>, S: Deserialize<'de>> Deserialize<'de> for FindingKeyInput<F, S> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    remote = "Self",
    tag = "reason",
    rename_all = "kebab-case",
    deny_unknown_fields,
    bound(deserialize = "P: Deserialize<'de>")
)]
pub enum MissingResolution<P = RepoPathText> {
    HeadingAnchorNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<String>,
        path: P,
    },
    LabelNotDeclared {},
    LineFragmentOutOfRange {
        path: P,
    },
    PathNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<P>,
        path: P,
        #[serde(
            default,
            deserialize_with = "json_serde::deserialize_some",
            skip_serializing_if = "Option::is_none"
        )]
        same_object_at: Option<Nullable<P>>,
    },
}

impl<P: Serialize> Serialize for MissingResolution<P> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for MissingResolution<P> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    remote = "Self",
    tag = "kind",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum StructuralResolution {
    Missing(MissingResolution),
    TypeMismatch { target: Target<RepoPathText> },
}

impl Serialize for StructuralResolution {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for StructuralResolution {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct FactEvidence {
    pub kind: FactEvidenceKind,
    pub resolution: StructuralResolution,
    pub occurrence_multiplicity: u64,
}

impl Serialize for FactEvidence {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for FactEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    remote = "Self",
    deny_unknown_fields,
    bound(deserialize = "K: Deserialize<'de>, E: Deserialize<'de>, F: Deserialize<'de>")
)]
pub struct Fact<K = FindingKeyInput, E = FactEvidence, F = EligibleFindingKind> {
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub evidence: E,
    pub finding_kind: F,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub key_input: K,
    pub schema: FactSchema,
}

impl<K: Serialize, E: Serialize, F: Serialize> Serialize for Fact<K, E, F> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, K: Deserialize<'de>, E: Deserialize<'de>, F: Deserialize<'de>> Deserialize<'de>
    for Fact<K, E, F>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses and validates one structural finding fact.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, or inconsistent
/// finding, key, resolution, and multiplicity values.
pub fn parse_fact(bytes: &[u8]) -> Result<Fact, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let fact: Fact = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    fact.validate()?;
    Ok(fact)
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
    let digest = {
        let mut writer =
            digest_io::IoWrapper(sha2::Sha256::new_with_prefix(FACT_DOMAIN).chain_update([0_u8]));
        serde_json_canonicalizer::to_writer(fact, &mut writer)
            .map(|()| Digest::from(writer.0.finalize().0))
    }
    .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    Ok((key, digest))
}

impl Fact {
    /// Checks that the finding kind, resolution, key and multiplicity agree.
    ///
    /// # Errors
    ///
    /// The public fields describe different structural findings or multiple occurrences.
    pub fn validate(&self) -> Result<(), Error> {
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
