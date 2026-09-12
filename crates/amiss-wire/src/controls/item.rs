use super::mapping::wire_fields;
use serde::{Deserialize, Deserializer, Serialize};

use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::model::{Oid, RepoPathText};
use crate::resolution::{Missing, Resolution, Target};

use super::fact::{Fact, FindingKeyInput, FindingScope, TargetIntent};
use super::{
    EligibleFindingKind, FACT_DOMAIN, FINDING_KEY_DOMAIN, SourceConstruct, TargetKind, non_null,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum KeySchema {
    #[serde(rename = "amiss/scanner-finding-key-input")]
    FindingKeyInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum FactSchema {
    #[serde(rename = "amiss/scanner-fact")]
    Fact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyInput {
    schema: KeySchema,
    finding_kind: EligibleFindingKind,
    scope: Scope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    kind: ReferenceKind,
    document: RepoPathText,
    source_construct: SourceConstruct,
    normalized_target_intent: Intent,
    occurrence: Occurrence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    kind: IntentKind,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    commit_oid: Option<Oid>,
    path: RepoPathText,
    target_kind: TargetKind,
    #[serde(deserialize_with = "codec::nullable")]
    query_digest: Option<Digest>,
    #[serde(deserialize_with = "codec::nullable")]
    fragment_digest: Option<Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Occurrence {
    kind: OccurrenceKind,
    source_projection_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactInput {
    schema: FactSchema,
    finding_kind: EligibleFindingKind,
    key_input: KeyInput,
    evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    kind: ReferenceKind,
    resolution: EligibleResolution,
    occurrence_multiplicity: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[serde(remote = "Self")]
enum EligibleResolution {
    Missing(MissingInput),
    TypeMismatch { target: Target<RepoPathText> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case", deny_unknown_fields)]
#[serde(remote = "Self")]
enum MissingInput {
    PathNotFound {
        path: RepoPathText,
        #[serde(deserialize_with = "codec::nullable")]
        near: Option<RepoPathText>,
        #[serde(default, skip_serializing_if = "NullablePresence::is_absent")]
        same_object_at: NullablePresence<RepoPathText>,
    },
    LineFragmentOutOfRange {
        path: RepoPathText,
    },
    HeadingAnchorNotFound {
        path: RepoPathText,
        #[serde(deserialize_with = "codec::nullable")]
        near: Option<String>,
    },
    LabelNotDeclared {},
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum NullablePresence<T> {
    #[default]
    Absent,
    Present(Option<T>),
}

impl<T> NullablePresence<T> {
    const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    fn into_option(self) -> Option<T> {
        match self {
            Self::Absent => None,
            Self::Present(value) => value,
        }
    }
}

impl<T: Serialize> Serialize for NullablePresence<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent => serializer.serialize_none(),
            Self::Present(value) => value.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NullablePresence<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::deserialize(deserializer).map(Self::Present)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ReferenceKind {
    Reference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum IntentKind {
    RepositoryPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum OccurrenceKind {
    SourceProjection,
}

wire_fields! {
    KeyInput <=> FindingKeyInput (input) {
        fields [finding_kind],
        mapped [scope],
        wire { schema: KeySchema::FindingKeyInput },
        domain {}
    }
}

wire_fields! {
    Scope <=> FindingScope (input) {
        fields [document, source_construct],
        mapped [normalized_target_intent],
        wire {
            kind: ReferenceKind::Reference,
            occurrence: Occurrence {
                kind: OccurrenceKind::SourceProjection,
                source_projection_digest: input.source_projection_digest,
            }
        },
        domain { source_projection_digest: input.occurrence.source_projection_digest }
    }
}

wire_fields! {
    Intent <=> TargetIntent (input) {
        fields [commit_oid, path, target_kind, query_digest, fragment_digest],
        mapped [],
        wire { kind: IntentKind::RepositoryPath },
        domain {}
    }
}

impl From<MissingInput> for Missing<RepoPathText> {
    fn from(input: MissingInput) -> Self {
        match input {
            MissingInput::PathNotFound {
                path,
                near,
                same_object_at,
            } => Self::PathNotFound {
                path,
                near,
                same_object_at: same_object_at.into_option(),
            },
            MissingInput::LineFragmentOutOfRange { path } => Self::LineFragmentOutOfRange { path },
            MissingInput::HeadingAnchorNotFound { path, near } => {
                Self::HeadingAnchorNotFound { path, near }
            }
            MissingInput::LabelNotDeclared {} => Self::LabelNotDeclared,
        }
    }
}

impl FactInput {
    pub(super) fn check(
        self,
        path: &str,
        finding_key: Digest,
        fact_digest: Digest,
        fact_field: &str,
    ) -> Result<Fact, Error> {
        let fact_path = format!("{path}.{fact_field}");
        let Evidence {
            occurrence_multiplicity,
            ..
        } = &self.evidence;
        if *occurrence_multiplicity != 1 {
            return fail(
                &format!("{fact_path}.evidence.occurrence_multiplicity"),
                ErrorKind::InvalidValue,
            );
        }
        let expected_key = codec::digest(FINDING_KEY_DOMAIN, &self.key_input)?;
        let expected_fact = codec::digest(FACT_DOMAIN, &self)?;
        let Evidence { resolution, .. } = self.evidence;
        let resolution = match resolution {
            EligibleResolution::Missing(missing) => Resolution::Missing(missing.into()),
            EligibleResolution::TypeMismatch { target } => Resolution::TypeMismatch(target),
        };
        let fact = Fact::new(self.key_input.into(), resolution)
            .filter(|fact| fact.finding_kind() == self.finding_kind)
            .ok_or_else(|| Error::new(&fact_path, ErrorKind::Inconsistent))?;
        if expected_key != finding_key {
            return fail(&format!("{path}.finding_key"), ErrorKind::DigestMismatch);
        }
        if expected_fact != fact_digest {
            return fail(
                &format!("{path}.{fact_field}_digest"),
                ErrorKind::DigestMismatch,
            );
        }
        Ok(fact)
    }
}

pub(super) fn check_reason(path: &str, reason: &str) -> Result<(), Error> {
    if (1..=1024).contains(&reason.chars().count()) && reason.chars().any(|c| !c.is_whitespace()) {
        Ok(())
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

impl Serialize for EligibleResolution {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for EligibleResolution {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for MissingInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for MissingInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}
