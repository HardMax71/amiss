use std::collections::BTreeMap;

use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{self, Document, Envelope, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::{DocsCandidate, PublicationProducer, PublicationResource, bounded_text};

use super::{LocaleCoverageScope, PAGE_KEY_BYTES};

mod pages;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-evidence-payload";
pub const EVIDENCE_DOCUMENT_BYTES: u64 = crate::semantic::SEMANTIC_EVIDENCE_BYTES;
pub const PAGE_ITEMS_LIMIT: usize = crate::semantic::SEMANTIC_OBSERVATIONS_LIMIT;

pub type LocaleCoverageEvidenceEnvelope = Envelope<LocaleCoverageEvidence>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleCoverageEvidence {
    pub schema: Schema<Self>,
    pub plan_payload_digest: Digest,
    #[garde(dive)]
    pub docs: DocsCandidate,
    #[garde(dive)]
    pub scope: LocaleCoverageScope,
    #[garde(dive)]
    pub producer: PublicationProducer,
    #[garde(dive)]
    pub source: LocalePageInventory,
    #[garde(dive)]
    pub target: LocaleTargetInventory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocalePageInventory {
    pub input_digest: Digest,
    #[garde(dive)]
    #[serde(deserialize_with = "crate::codec::nullable")]
    pub product: Option<PublicationResource>,
    pub complete: bool,
    #[serde(with = "pages")]
    pub pages: BTreeMap<String, Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleTargetInventory {
    pub input_digest: Digest,
    #[garde(dive)]
    #[serde(deserialize_with = "crate::codec::nullable")]
    pub product: Option<PublicationResource>,
    pub complete: bool,
    #[serde(with = "pages")]
    pub pages: BTreeMap<String, LocaleTargetPage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleTargetPage {
    pub resource_digest: Digest,
    pub origin: LocaleTargetOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[serde(remote = "Self")]
pub enum LocaleTargetOrigin {
    TargetResource {
        #[serde(deserialize_with = "crate::codec::nullable")]
        based_on_source_digest: Option<Digest>,
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
        Self::deserialize(codec::object(deserializer))
    }
}

/// Reads a bounded pair of locale inventories and verifies their digest.
///
/// # Errors
///
/// The evidence violates its shape, domain constraints, or payload binding.
pub fn parse_evidence(bytes: &[u8]) -> Result<LocaleCoverageEvidenceEnvelope, Error> {
    LocaleCoverageEvidenceEnvelope::parse(bytes)
}

/// Seals one pair of locale page inventories after validating their constraints.
///
/// # Errors
///
/// The evidence violates its contract or exceeds its page or byte ceiling.
pub fn evidence(input: &LocaleCoverageEvidence) -> Result<Value, Error> {
    codec::seal_value(input)
}

impl Document for LocaleCoverageEvidence {
    const PAYLOAD_SCHEMA: &'static str = EVIDENCE_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = EVIDENCE_ENVELOPE_SCHEMA;
    const LIMIT: u64 = EVIDENCE_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        self.docs.check(&format!("{root}.docs"))?;
        self.scope.check(&format!("{root}.scope"))?;
        self.source
            .pages
            .len()
            .checked_add(self.target.pages.len())
            .filter(|total| *total <= PAGE_ITEMS_LIMIT)
            .ok_or_else(|| Error::new(&format!("{root}.target.pages"), ErrorKind::LimitExceeded))?;
        let keys = self
            .source
            .pages
            .keys()
            .enumerate()
            .map(|(index, key)| ("source", index, key))
            .chain(
                self.target
                    .pages
                    .keys()
                    .enumerate()
                    .map(|(index, key)| ("target", index, key)),
            );
        for (side, index, key) in keys {
            if !bounded_text(key, PAGE_KEY_BYTES) {
                return fail(
                    &format!("{root}.{side}.pages[{index}].key"),
                    ErrorKind::InvalidValue,
                );
            }
        }
        Ok(())
    }
}
