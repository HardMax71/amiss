use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::assessment::Nullable;
use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hb};
use crate::json::{self, Value};
use crate::model::ArtifactId;
use crate::publication::{
    DocsCandidate, PublicationProducer, PublicationResource, PublicationUriKind, decode_docs,
    decode_identity, decode_producer, validate_docs, validate_producer, validate_publication_uri,
};
use crate::semantic::producer_version_valid;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoveragePlanEnvelope {
    pub schema: PlanEnvelopeSchema,
    pub payload: LocaleCoveragePlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanEnvelopeSchema {
    #[serde(rename = "amiss/locale-coverage-plan-envelope")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanPayloadSchema {
    #[serde(rename = "amiss/locale-coverage-plan-payload")]
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
    AllSource,
    Named { keys: Vec<String> },
}

struct LocaleCoverageFacts {
    docs: DocsCandidate,
    scope: LocaleCoverageScope,
    producer: PublicationProducer,
}

/// Parses one closed, report-bound locale coverage plan.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, unknown fields, invalid identities, an ambiguous
/// locale pair, unsorted or repeated named page keys, or a payload digest mismatch.
pub fn parse_plan(bytes: &[u8]) -> Result<LocaleCoveragePlanEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > LOCALE_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: LocaleCoveragePlanEnvelope = de::deserialize_json(bytes)?;
    if plan_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for one locale coverage plan.
///
/// # Errors
///
/// Fails when a field violates the same closed grammar [`parse_plan`] enforces or the encoded
/// document exceeds its byte ceiling.
pub fn plan(input: &LocaleCoveragePlan) -> Result<Value, Error> {
    let payload_digest = plan_payload_digest(input)?;
    let document = LocaleCoveragePlanEnvelope {
        schema: PlanEnvelopeSchema::Current,
        payload: input.clone(),
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > LOCALE_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn plan_payload_digest(input: &LocaleCoveragePlan) -> Result<Digest, Error> {
    validate_plan(input)?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| hb(PLAN_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
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
    (!keys.is_empty())
        .then_some(())
        .ok_or_else(|| Error::new(&format!("{path}.keys"), ErrorKind::InvalidValue))?;
    (keys.len() <= PAGE_ITEMS_LIMIT)
        .then_some(())
        .ok_or_else(|| Error::new(&format!("{path}.keys"), ErrorKind::LimitExceeded))?;
    keys.iter().enumerate().try_for_each(|(index, key)| {
        (!key.is_empty() && key.len() <= PAGE_KEY_BYTES && !key.chars().any(char::is_control))
            .then_some(())
            .ok_or_else(|| Error::new(&format!("{path}.keys[{index}]"), ErrorKind::InvalidValue))
    })?;
    keys.iter()
        .zip(keys.iter().skip(1))
        .try_for_each(|(previous, current)| match previous.cmp(current) {
            Ordering::Less => Ok(()),
            Ordering::Equal => fail(&format!("{path}.keys"), ErrorKind::DuplicateMember),
            Ordering::Greater => fail(&format!("{path}.keys"), ErrorKind::UnsortedSet),
        })
}

fn decode_facts(parent: &mut Obj) -> Result<LocaleCoverageFacts, Error> {
    Ok(LocaleCoverageFacts {
        docs: parent.required("docs", decode_docs)?,
        scope: parent.required("scope", |path, value| {
            let mut scope = Obj::new(path, value)?;
            let site = scope.required("site", decode_identity)?;
            let source_locale =
                scope.required("source_locale", crate::semantic::decode_open_identity)?;
            let target_locale =
                scope.required("target_locale", crate::semantic::decode_open_identity)?;
            let channel = scope.required("channel", decode_identity)?;
            let version_path = scope.field("version");
            let version = de::decode_nullable(
                &version_path,
                scope.take("version")?,
                crate::semantic::decode_open_identity,
            )?
            .map_or(Nullable::Null, Nullable::Value);
            scope.finish()?;
            if source_locale == target_locale {
                return fail(path, ErrorKind::Inconsistent);
            }
            Ok(LocaleCoverageScope {
                site,
                source_locale,
                target_locale,
                channel,
                version,
            })
        })?,
        producer: parent.required("producer", decode_producer)?,
    })
}

fn scope_value(scope: &LocaleCoverageScope) -> Value {
    object(vec![
        ("site", text(scope.site.as_str())),
        ("source_locale", text(&scope.source_locale)),
        ("target_locale", text(&scope.target_locale)),
        ("channel", text(scope.channel.as_str())),
        (
            "version",
            match &scope.version {
                Nullable::Value(version) => text(version),
                Nullable::Null => Value::Null,
            },
        ),
    ])
}
