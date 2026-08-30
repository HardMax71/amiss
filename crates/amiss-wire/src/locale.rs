use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::{
    DocsCandidate, PublicationProducer, decode_docs, decode_identity, decode_producer, docs_value,
    producer_value,
};

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as LocaleCoverageVerdict;
pub use assessment::{
    ASSESSMENT_DOCUMENT_BYTES, ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAGE_ITEMS_LIMIT,
    ASSESSMENT_PAYLOAD_SCHEMA, LocaleCoverageAssessment, LocaleCoverageAssessmentEnvelope,
    LocaleCoverageReason, LocaleCoverageResult, LocaleFallbackResult, LocaleFallbackStatus,
    LocaleLineageResult, LocaleLineageStatus, assess, parse_assessment,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoveragePlanEnvelope {
    pub payload: LocaleCoveragePlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoveragePlan {
    pub report_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub scope: LocaleCoverageScope,
    pub producer: PublicationProducer,
    pub policy: LocaleCoveragePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageScope {
    pub site: ArtifactId,
    pub source_locale: String,
    pub target_locale: String,
    pub channel: ArtifactId,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoveragePolicy {
    pub identity: ArtifactId,
    pub context_digest: Digest,
    pub required: LocalePageRequirement,
    pub fallbacks: Vec<LocaleFallbackRule>,
    pub require_target_lineage: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleFallbackRule {
    pub class: ArtifactId,
    pub pages: LocalePageRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalePageRequirement {
    AllSource,
    Named(Vec<String>),
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
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        PLAN_ENVELOPE_SCHEMA,
        PLAN_PAYLOAD_SCHEMA,
        LOCALE_DOCUMENT_BYTES,
        decode_plan,
    )?;
    Ok(LocaleCoveragePlanEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for one locale coverage plan.
///
/// # Errors
///
/// Fails when a field violates the same closed grammar [`parse_plan`] enforces or the encoded
/// document exceeds its byte ceiling.
pub fn plan(input: &LocaleCoveragePlan) -> Result<Value, Error> {
    let payload = plan_value(input);
    let _validated = decode_plan("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        PLAN_ENVELOPE_SCHEMA,
        PLAN_PAYLOAD_SCHEMA,
        LOCALE_DOCUMENT_BYTES,
    )
}

fn decode_plan(path: &str, value: Value) -> Result<LocaleCoveragePlan, Error> {
    let decode_requirement = |path: &str, value: Value| {
        let mut requirement = Obj::new(path, value)?;
        let mode_path = requirement.field("mode");
        let mode = de::string(&mode_path, requirement.take("mode")?)?;
        match mode.as_str() {
            "all-source" => {
                requirement.finish()?;
                Ok(LocalePageRequirement::AllSource)
            }
            "named" => {
                let keys_path = requirement.field("keys");
                let keys = de::sorted_items(
                    &keys_path,
                    requirement.take("keys")?,
                    PAGE_ITEMS_LIMIT,
                    |path, value| de::bounded_text(path, value, PAGE_KEY_BYTES),
                    |key| key,
                )?;
                if keys.is_empty() {
                    return fail(&keys_path, ErrorKind::InvalidValue);
                }
                requirement.finish()?;
                Ok(LocalePageRequirement::Named(keys))
            }
            _ => fail(&mode_path, ErrorKind::InvalidValue),
        }
    };
    let mut plan = Obj::new(path, value)?;
    plan.required("schema", |path, value| {
        de::const_str(path, value, PLAN_PAYLOAD_SCHEMA)
    })?;
    let report_payload_digest = plan.required("report_payload_digest", de::digest)?;
    let facts = decode_facts(&mut plan)?;
    let policy = plan.required("policy", |path, value| {
        let mut policy = Obj::new(path, value)?;
        let identity = policy.required("identity", decode_identity)?;
        let context_digest = policy.required("context_digest", de::digest)?;
        let required = policy.required("required", decode_requirement)?;
        let require_target_lineage = policy.required("require_target_lineage", de::boolean)?;
        let fallbacks_path = policy.field("fallbacks");
        let fallbacks = de::sorted_items(
            &fallbacks_path,
            policy.take("fallbacks")?,
            PAGE_ITEMS_LIMIT,
            |path, value| {
                let mut rule = Obj::new(path, value)?;
                let class = rule.required("class", decode_identity)?;
                let pages = rule.required("pages", decode_requirement)?;
                rule.finish()?;
                Ok(LocaleFallbackRule { class, pages })
            },
            |rule| rule.class.as_str(),
        )?;
        policy.finish()?;
        Ok(LocaleCoveragePolicy {
            identity,
            context_digest,
            required,
            fallbacks,
            require_target_lineage,
        })
    })?;
    plan.finish()?;
    Ok(LocaleCoveragePlan {
        report_payload_digest,
        docs: facts.docs,
        scope: facts.scope,
        producer: facts.producer,
        policy,
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
            let version = de::nullable(scope.take("version")?)
                .map(|value| crate::semantic::decode_open_identity(&version_path, value))
                .transpose()?;
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

fn plan_value(plan: &LocaleCoveragePlan) -> Value {
    object(vec![
        ("schema", text(PLAN_PAYLOAD_SCHEMA)),
        (
            "report_payload_digest",
            text(&plan.report_payload_digest.to_string()),
        ),
        ("docs", docs_value(&plan.docs)),
        ("scope", scope_value(&plan.scope)),
        ("producer", producer_value(&plan.producer)),
        ("policy", policy_value(&plan.policy)),
    ])
}

fn scope_value(scope: &LocaleCoverageScope) -> Value {
    object(vec![
        ("site", text(scope.site.as_str())),
        ("source_locale", text(&scope.source_locale)),
        ("target_locale", text(&scope.target_locale)),
        ("channel", text(scope.channel.as_str())),
        (
            "version",
            scope
                .version
                .as_ref()
                .map_or(Value::Null, |version| text(version)),
        ),
    ])
}

fn policy_value(policy: &LocaleCoveragePolicy) -> Value {
    object(vec![
        ("identity", text(policy.identity.as_str())),
        ("context_digest", text(&policy.context_digest.to_string())),
        ("required", requirement_value(&policy.required)),
        (
            "require_target_lineage",
            Value::Bool(policy.require_target_lineage),
        ),
        (
            "fallbacks",
            Value::array(
                policy
                    .fallbacks
                    .iter()
                    .map(|rule| {
                        object(vec![
                            ("class", text(rule.class.as_str())),
                            ("pages", requirement_value(&rule.pages)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn requirement_value(requirement: &LocalePageRequirement) -> Value {
    match requirement {
        LocalePageRequirement::AllSource => object(vec![("mode", text("all-source"))]),
        LocalePageRequirement::Named(keys) => object(vec![
            ("mode", text("named")),
            (
                "keys",
                Value::array(keys.iter().map(|key| text(key)).collect()),
            ),
        ]),
    }
}
