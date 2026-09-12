use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::assessment::ordered;
use crate::codec::{self, Document, Envelope, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::{
    DocsCandidate, PublicationProducer, PublicationResource, bounded_text, open_identity,
};

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as LocaleCoverageVerdict;
pub use assessment::{
    ASSESSMENT_DOCUMENT_BYTES, ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAGE_ITEMS_LIMIT,
    ASSESSMENT_PAYLOAD_SCHEMA, LocaleCoverageAssessment, LocaleCoverageAssessmentEnvelope,
    LocaleCoverageReason, LocaleCoverageResult, LocaleFallbackResult, LocaleFallbackStatus,
    LocaleLineageResult, LocaleLineageStatus, LocaleProductResult, assess, parse_assessment,
};
pub use evidence::{
    EVIDENCE_DOCUMENT_BYTES, EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA,
    LocaleCoverageEvidence, LocaleCoverageEvidenceEnvelope, LocalePageInventory,
    LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage, PAGE_ITEMS_LIMIT, evidence,
    parse_evidence,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-plan-payload";
pub const LOCALE_DOCUMENT_BYTES: u64 = 65_536;
pub const PAGE_KEY_BYTES: usize = crate::semantic::RECORD_KEY_BYTES;

pub type LocaleCoveragePlanEnvelope = Envelope<LocaleCoveragePlan>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleCoveragePlan {
    pub schema: Schema<Self>,
    pub report_payload_digest: Digest,
    #[garde(dive)]
    pub docs: DocsCandidate,
    #[garde(dive)]
    pub scope: LocaleCoverageScope,
    #[garde(dive)]
    #[serde(deserialize_with = "crate::codec::nullable")]
    pub product: Option<PublicationResource>,
    #[garde(dive)]
    pub producer: PublicationProducer,
    #[garde(dive)]
    pub policy: LocaleCoveragePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleCoverageScope {
    pub site: ArtifactId,
    #[garde(custom(open_identity))]
    pub source_locale: String,
    #[garde(custom(open_identity))]
    pub target_locale: String,
    pub channel: ArtifactId,
    #[serde(deserialize_with = "crate::codec::nullable")]
    #[garde(inner(custom(open_identity)))]
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleCoveragePolicy {
    pub identity: ArtifactId,
    pub context_digest: Digest,
    #[garde(dive)]
    pub required: LocalePageRequirement,
    #[garde(dive)]
    pub fallbacks: Vec<LocaleFallbackRule>,
    pub require_target_lineage: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct LocaleFallbackRule {
    pub class: ArtifactId,
    #[garde(dive)]
    pub pages: LocalePageRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(tag = "mode", rename_all = "kebab-case")]
#[serde(remote = "Self")]
pub enum LocalePageRequirement {
    AllSource {},
    Named { keys: Vec<String> },
}

impl Serialize for LocalePageRequirement {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LocalePageRequirement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

/// Reads a closed, bounded locale coverage plan and verifies its digest.
///
/// # Errors
///
/// The plan violates its shape, field constraints, or digest binding.
pub fn parse_plan(bytes: &[u8]) -> Result<LocaleCoveragePlanEnvelope, Error> {
    LocaleCoveragePlanEnvelope::parse(bytes)
}

/// Seals one locale coverage plan after checking its domain laws.
///
/// # Errors
///
/// The plan violates its contract or exceeds the document byte ceiling.
pub fn plan(input: &LocaleCoveragePlan) -> Result<Value, Error> {
    codec::seal_value(input)
}

impl Document for LocaleCoveragePlan {
    const PAYLOAD_SCHEMA: &'static str = PLAN_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = PLAN_ENVELOPE_SCHEMA;
    const LIMIT: u64 = LOCALE_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        self.docs.check(&format!("{root}.docs"))?;
        self.scope.check(&format!("{root}.scope"))?;
        self.policy
            .required
            .check(&format!("{root}.policy.required"))?;
        let fallbacks = format!("{root}.policy.fallbacks");
        ordered(
            &fallbacks,
            &self.policy.fallbacks,
            PAGE_ITEMS_LIMIT,
            |rule| &rule.class,
        )?;
        for (index, rule) in self.policy.fallbacks.iter().enumerate() {
            rule.pages.check(&format!("{fallbacks}[{index}].pages"))?;
        }
        Ok(())
    }
}

impl LocaleCoverageScope {
    fn check(&self, root: &str) -> Result<(), Error> {
        if self.source_locale == self.target_locale {
            return fail(root, ErrorKind::Inconsistent);
        }
        Ok(())
    }
}

impl LocalePageRequirement {
    fn check(&self, root: &str) -> Result<(), Error> {
        if let Self::Named { keys } = self {
            let path = format!("{root}.keys");
            check_page_keys(&path, keys)?;
            if keys.is_empty() {
                return fail(&path, ErrorKind::InvalidValue);
            }
        }
        Ok(())
    }
}

fn check_page_keys(path: &str, keys: &[String]) -> Result<(), Error> {
    ordered(path, keys, PAGE_ITEMS_LIMIT, |key| key)?;
    for (index, key) in keys.iter().enumerate() {
        if !bounded_text(key, PAGE_KEY_BYTES) {
            return fail(&format!("{path}[{index}]"), ErrorKind::InvalidValue);
        }
    }
    Ok(())
}
