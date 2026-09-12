use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::ArtifactId;
use crate::model::Digest;
use crate::publication::{
    DocsCandidate, PublicationProducer, PublicationResource, PublicationUriKind, validate_docs,
    validate_producer, validate_publication_uri,
};

use super::{LocaleCoverageScope, validate_page_keys, validate_scope};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-evidence-payload";
pub const EVIDENCE_DOCUMENT_BYTES: u64 = crate::semantic::SEMANTIC_EVIDENCE_BYTES;
pub const PAGE_ITEMS_LIMIT: usize = crate::semantic::SEMANTIC_OBSERVATIONS_LIMIT;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self", bound(deserialize = "T: Deserialize<'de>"))]
pub struct LocaleCoverageEvidenceEnvelope<T = LocaleCoverageEvidence> {
    pub schema: EvidenceEnvelopeSchema,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub payload: T,
    pub payload_digest: Digest,
}

impl<T: Serialize> Serialize for LocaleCoverageEvidenceEnvelope<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for LocaleCoverageEvidenceEnvelope<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidenceEnvelopeSchema {
    #[strum(serialize = "amiss/locale-coverage-evidence-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct LocaleCoverageEvidence {
    pub schema: EvidencePayloadSchema,
    pub plan_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub scope: LocaleCoverageScope,
    pub producer: PublicationProducer,
    pub source: LocalePageInventory,
    pub target: LocaleTargetInventory,
}

impl Serialize for LocaleCoverageEvidence {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocaleCoverageEvidence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidencePayloadSchema {
    #[strum(serialize = "amiss/locale-coverage-evidence-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct LocalePageInventory {
    pub input_digest: Digest,
    pub product: Nullable<PublicationResource>,
    pub complete: bool,
    pub pages: Vec<LocaleSourcePage>,
}

impl Serialize for LocalePageInventory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocalePageInventory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct LocaleSourcePage {
    pub key: String,
    pub resource_digest: Digest,
}

impl Serialize for LocaleSourcePage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocaleSourcePage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct LocaleTargetInventory {
    pub input_digest: Digest,
    pub product: Nullable<PublicationResource>,
    pub complete: bool,
    pub pages: Vec<LocaleTargetPage>,
}

impl Serialize for LocaleTargetInventory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocaleTargetInventory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct LocaleTargetPage {
    pub key: String,
    pub resource_digest: Digest,
    pub origin: LocaleTargetOrigin,
}

impl Serialize for LocaleTargetPage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocaleTargetPage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[serde(remote = "Self")]
pub enum LocaleTargetOrigin {
    TargetResource {
        based_on_source_digest: Nullable<Digest>,
    },
    Fallback {
        class: ArtifactId,
        source_resource_digest: Digest,
    },
}

impl Serialize for LocaleTargetOrigin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocaleTargetOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses one closed, digest-bound pair of locale page inventories.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, unknown fields, invalid bindings, unsorted,
/// repeated, or oversized page sets, invalid page keys, or a payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<LocaleCoverageEvidenceEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > EVIDENCE_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let document: LocaleCoverageEvidenceEnvelope =
        serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if evidence_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for one pair of locale page inventories.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_evidence`] enforces or the
/// encoded document exceeds its byte ceiling.
pub fn evidence(input: &LocaleCoverageEvidence) -> Result<Vec<u8>, Error> {
    let payload_digest = evidence_payload_digest(input)?;
    let document = LocaleCoverageEvidenceEnvelope {
        schema: EvidenceEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > EVIDENCE_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    de::JsonProfile::validate(&canonical)?;
    Ok(canonical)
}

pub(super) fn evidence_payload_digest(input: &LocaleCoverageEvidence) -> Result<Digest, Error> {
    validate_evidence(input)?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| {
            Digest::from(
                sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(&canonical)
                    .finalize()
                    .0,
            )
        })
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn validate_evidence(evidence: &LocaleCoverageEvidence) -> Result<(), Error> {
    validate_docs("$.payload.docs", &evidence.docs)?;
    validate_scope("$.payload.scope", &evidence.scope)?;
    validate_producer("$.payload.producer", &evidence.producer)?;
    for (field, product) in [
        ("source", &evidence.source.product),
        ("target", &evidence.target.product),
    ] {
        if let Nullable::Value(product) = product {
            validate_publication_uri(
                &format!("$.payload.{field}.product.uri"),
                &product.uri,
                PublicationUriKind::Resource,
            )?;
        }
    }
    validate_page_keys(
        "$.payload.source.pages",
        evidence.source.pages.iter().map(|page| page.key.as_str()),
        ".key",
    )?;
    validate_page_keys(
        "$.payload.target.pages",
        evidence.target.pages.iter().map(|page| page.key.as_str()),
        ".key",
    )?;
    evidence
        .source
        .pages
        .len()
        .checked_add(evidence.target.pages.len())
        .filter(|total| *total <= PAGE_ITEMS_LIMIT)
        .ok_or_else(|| Error::new("$.payload.target.pages", ErrorKind::LimitExceeded))?;
    Ok(())
}
