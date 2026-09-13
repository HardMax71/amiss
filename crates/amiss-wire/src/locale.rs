use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{Error, ErrorKind, fail};
use crate::envelope::Payload;
use crate::model::ArtifactId;
use crate::model::Digest;
use crate::publication::{
    DocsCandidate, PublicationProducer, PublicationResource, PublicationUriKind, validate_docs,
    validate_producer, validate_publication_uri,
};
use crate::semantic::producer_version_valid;

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as LocaleCoverageVerdict;
pub use assessment::{
    ASSESSMENT_DOCUMENT_BYTES, ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAGE_ITEMS_LIMIT,
    ASSESSMENT_PAYLOAD_SCHEMA, AssessmentEnvelopeSchema, AssessmentPayloadSchema,
    LocaleCoverageAssessment, LocaleCoverageReason, LocaleCoverageResult, LocaleFallbackResult,
    LocaleFallbackStatus, LocaleLineageResult, LocaleLineageStatus, LocaleProductResult, assess,
};
pub use evidence::{
    EVIDENCE_DOCUMENT_BYTES, EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA,
    EvidenceEnvelopeSchema, EvidencePayloadSchema, LocaleCoverageEvidence, LocalePageInventory,
    LocaleSourcePage, LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage,
    PAGE_ITEMS_LIMIT,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-plan-payload";
pub const LOCALE_DOCUMENT_BYTES: u64 = 65_536;
pub const PAGE_KEY_BYTES: usize = crate::semantic::RECORD_KEY_BYTES;

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum PlanEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/locale-coverage-plan-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoveragePlan {
    pub schema: PlanPayloadSchema,
    pub report_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub scope: LocaleCoverageScope,
    pub product: Nullable<PublicationResource>,
    pub producer: PublicationProducer,
    pub policy: LocaleCoveragePolicy,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PlanPayloadSchema {
    #[strum(serialize = "amiss/locale-coverage-plan-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoverageScope {
    pub site: ArtifactId,
    pub source_locale: String,
    pub target_locale: String,
    pub channel: ArtifactId,
    pub version: Nullable<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoveragePolicy {
    pub identity: ArtifactId,
    pub context_digest: Digest,
    pub required: LocalePageRequirement,
    pub fallbacks: Vec<LocaleFallbackRule>,
    pub require_target_lineage: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleFallbackRule {
    pub class: ArtifactId,
    pub pages: LocalePageRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LocalePageRequirement {
    AllSource {},
    Named { keys: Vec<String> },
}

impl Payload for LocaleCoveragePlan {
    type Schema = PlanEnvelopeSchema;
    type Defect = Error;
    const DOMAIN: &'static str = PLAN_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = LOCALE_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate_plan(self)
    }
}

fn validate_plan(plan: &LocaleCoveragePlan) -> Result<(), Error> {
    validate_docs("$.payload.docs", &plan.docs)?;
    validate_scope("$.payload.scope", &plan.scope)?;
    if let Nullable::Value(product) = &plan.product {
        validate_publication_uri(
            "$.payload.product.uri",
            &product.uri,
            PublicationUriKind::Resource,
        )?;
    }
    validate_producer("$.payload.producer", &plan.producer)?;
    validate_requirement("$.payload.policy.required", &plan.policy.required)?;
    (plan.policy.fallbacks.len() <= PAGE_ITEMS_LIMIT)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload.policy.fallbacks", ErrorKind::LimitExceeded))?;
    plan.policy
        .fallbacks
        .iter()
        .zip(plan.policy.fallbacks.iter().skip(1))
        .try_for_each(
            |(previous, current)| match previous.class.cmp(&current.class) {
                Ordering::Less => Ok(()),
                Ordering::Equal => fail("$.payload.policy.fallbacks", ErrorKind::DuplicateMember),
                Ordering::Greater => fail("$.payload.policy.fallbacks", ErrorKind::UnsortedSet),
            },
        )?;
    plan.policy
        .fallbacks
        .iter()
        .enumerate()
        .try_for_each(|(index, rule)| {
            validate_requirement(
                &format!("$.payload.policy.fallbacks[{index}].pages"),
                &rule.pages,
            )
        })
}

fn validate_scope(path: &str, scope: &LocaleCoverageScope) -> Result<(), Error> {
    for (field, value) in [
        ("source_locale", scope.source_locale.as_str()),
        ("target_locale", scope.target_locale.as_str()),
    ] {
        if !producer_version_valid(value) {
            return fail(&format!("{path}.{field}"), ErrorKind::InvalidValue);
        }
    }
    if let Nullable::Value(version) = &scope.version
        && !producer_version_valid(version)
    {
        return fail(&format!("{path}.version"), ErrorKind::InvalidValue);
    }
    (scope.source_locale != scope.target_locale)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::Inconsistent))
}

fn validate_requirement(path: &str, requirement: &LocalePageRequirement) -> Result<(), Error> {
    let LocalePageRequirement::Named { keys } = requirement else {
        return Ok(());
    };
    let keys_path = format!("{path}.keys");
    (!keys.is_empty())
        .then_some(())
        .ok_or_else(|| Error::new(&keys_path, ErrorKind::InvalidValue))?;
    validate_page_keys(&keys_path, keys.iter().map(String::as_str), "")
}

fn validate_page_keys<'a>(
    path: &str,
    keys: impl ExactSizeIterator<Item = &'a str>,
    key_suffix: &str,
) -> Result<(), Error> {
    (keys.len() <= PAGE_ITEMS_LIMIT)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    let mut previous: Option<&str> = None;
    for (index, key) in keys.enumerate() {
        if key.is_empty() || key.len() > PAGE_KEY_BYTES || key.chars().any(char::is_control) {
            return fail(
                &format!("{path}[{index}]{key_suffix}"),
                ErrorKind::InvalidValue,
            );
        }
        if let Some(previous) = previous {
            match previous.cmp(key) {
                Ordering::Less => {}
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
            }
        }
        previous = Some(key);
    }
    Ok(())
}
