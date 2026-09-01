use serde::{Deserialize, Serialize};

use crate::controls::value::{object, repository, text};
use crate::controls::{decode_enum, decode_repository};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hb};
use crate::json::{self, Value};
use crate::model::{ArtifactId, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as PublicationVerdict;
pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, PublicationAssessment,
    PublicationAssessmentEnvelope, PublicationReason, assess, parse_assessment,
};
pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, PublicationDeployment, PublicationEvidence,
    PublicationEvidenceEnvelope, evidence, parse_evidence,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/publication-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/publication-plan-payload";
pub const PUBLICATION_DOCUMENT_BYTES: u64 = 65_536;
pub const PUBLICATION_URI_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationPlanEnvelope {
    pub payload: PublicationPlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPlan {
    pub report_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub target: PublicationTarget,
    pub site: CompletedSite,
    pub product: PublicationResource,
    pub producer: PublicationProducer,
    pub relation: PublicationRelation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocsCandidate {
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    #[serde(rename = "commit_oid")]
    pub commit: Oid,
    #[serde(rename = "tree_oid")]
    pub tree: Oid,
    pub candidate_identity_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationTarget {
    pub provider: ArtifactId,
    pub instance: ArtifactId,
    pub environment: ArtifactId,
    pub channel: ArtifactId,
    pub canonical_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedSite {
    pub artifact: PublicationResource,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationResource {
    pub uri: String,
    pub digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationProducer {
    pub identity: ArtifactId,
    pub version: String,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRelation {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum PlanEnvelope<T> {
    #[serde(rename = "amiss/publication-plan-envelope")]
    Current { payload: T, payload_digest: Digest },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum PlanPayload<T> {
    #[serde(rename = "amiss/publication-plan-payload")]
    Current(T),
}

struct PublicationFacts {
    producer: PublicationProducer,
    docs: DocsCandidate,
    target: PublicationTarget,
    site: CompletedSite,
    product: PublicationResource,
}

/// Parses one closed, digest-bound publication plan.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or URI, inconsistent Git object formats, or a payload digest mismatch.
pub fn parse_plan(bytes: &[u8]) -> Result<PublicationPlanEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: PlanEnvelope<PlanPayload<PublicationPlan>> = serde_json::from_slice(bytes)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let PlanEnvelope::Current {
        payload,
        payload_digest,
    } = document;
    let PlanPayload::Current(payload) = payload;
    if plan_payload_digest(&payload)? != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(PublicationPlanEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for one publication plan.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_plan`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn plan(input: &PublicationPlan) -> Result<Value, Error> {
    let payload_digest = plan_payload_digest(input)?;
    let document = PlanEnvelope::Current {
        payload: PlanPayload::Current(input),
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

pub(super) fn plan_payload_digest(input: &PublicationPlan) -> Result<Digest, Error> {
    validate_facts(
        "$.payload",
        &input.docs,
        &input.target,
        &input.site,
        &input.product,
        &input.producer,
    )?;
    serde_json_canonicalizer::to_vec(&PlanPayload::Current(input))
        .map(|canonical| hb(PLAN_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn decode_facts(parent: &mut Obj) -> Result<PublicationFacts, Error> {
    let docs = parent.required("docs", decode_docs)?;
    let target = parent.required("target", |path, value| {
        let mut target = Obj::new(path, value)?;
        let provider = target.required("provider", decode_identity)?;
        let instance = target.required("instance", decode_identity)?;
        let environment = target.required("environment", decode_identity)?;
        let channel = target.required("channel", decode_identity)?;
        let canonical_url = target.required("canonical_url", |path, value| {
            let raw = de::string(path, value)?;
            validate_publication_uri(path, &raw, PublicationUriKind::CanonicalUrl)?;
            Ok(raw)
        })?;
        target.finish()?;
        Ok(PublicationTarget {
            provider,
            instance,
            environment,
            channel,
            canonical_url,
        })
    })?;
    let site = parent.required("site", |path, value| {
        let mut site = Obj::new(path, value)?;
        let artifact = site.required("artifact", decode_resource)?;
        let input_digest = site.required("input_digest", de::digest)?;
        site.finish()?;
        Ok(CompletedSite {
            artifact,
            input_digest,
        })
    })?;
    let product = parent.required("product", decode_resource)?;
    let producer = parent.required("producer", decode_producer)?;
    Ok(PublicationFacts {
        producer,
        docs,
        target,
        site,
        product,
    })
}

pub(crate) fn decode_docs(path: &str, value: Value) -> Result<DocsCandidate, Error> {
    let mut docs = Obj::new(path, value)?;
    let repository = docs.required("repository", decode_repository)?;
    let object_format = docs.required("object_format", decode_enum)?;
    let commit_path = docs.field("commit_oid");
    let commit = Oid::new(
        object_format,
        de::string(&commit_path, docs.take("commit_oid")?)?,
    )
    .ok_or_else(|| Error::new(&commit_path, ErrorKind::InvalidValue))?;
    let tree_path = docs.field("tree_oid");
    let tree = Oid::new(
        object_format,
        de::string(&tree_path, docs.take("tree_oid")?)?,
    )
    .ok_or_else(|| Error::new(&tree_path, ErrorKind::InvalidValue))?;
    let candidate_identity_digest = docs.required("candidate_identity_digest", de::digest)?;
    docs.finish()?;
    Ok(DocsCandidate {
        repository,
        object_format,
        commit,
        tree,
        candidate_identity_digest,
    })
}

pub(crate) fn decode_resource(path: &str, value: Value) -> Result<PublicationResource, Error> {
    let mut resource = Obj::new(path, value)?;
    let uri = resource.required("uri", decode_resource_uri)?;
    let digest = resource.required("digest", de::digest)?;
    resource.finish()?;
    Ok(PublicationResource { uri, digest })
}

pub(crate) fn decode_identity(path: &str, value: Value) -> Result<ArtifactId, Error> {
    let raw = de::string(path, value)?;
    ArtifactId::new(raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn decode_producer(path: &str, value: Value) -> Result<PublicationProducer, Error> {
    let mut producer = Obj::new(path, value)?;
    let identity = producer.required("identity", decode_identity)?;
    let version = producer.required("version", crate::semantic::decode_open_identity)?;
    let context_digest = producer.required("context_digest", de::digest)?;
    producer.finish()?;
    Ok(PublicationProducer {
        identity,
        version,
        context_digest,
    })
}

fn decode_resource_uri(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    validate_publication_uri(path, &raw, PublicationUriKind::Resource)?;
    Ok(raw)
}

#[derive(Clone, Copy)]
enum PublicationUriKind {
    CanonicalUrl,
    Resource,
}

fn validate_publication_uri(path: &str, raw: &str, kind: PublicationUriKind) -> Result<(), Error> {
    let grammar_valid = match kind {
        PublicationUriKind::CanonicalUrl => {
            let authority = raw
                .strip_prefix("https://")
                .and_then(|rest| rest.split('/').next());
            let authority_valid = authority.is_some_and(|authority| {
                if let Some(bracketed) = authority.strip_prefix('[') {
                    bracketed.split_once(']').is_some_and(|(host, port)| {
                        !host.is_empty()
                            && (port.is_empty()
                                || port
                                    .strip_prefix(':')
                                    .is_some_and(|port| port.parse::<u16>().is_ok()))
                    })
                } else {
                    !authority.contains(['[', ']', '@'])
                        && authority.rsplit_once(':').map_or(
                            !authority.is_empty(),
                            |(host, port)| {
                                !host.is_empty()
                                    && !host.contains(':')
                                    && port.parse::<u16>().is_ok()
                            },
                        )
                }
            });
            authority_valid && !raw.contains('?') && crate::uri::http_destination_valid(raw)
        }
        PublicationUriKind::Resource => {
            let (without_query, query) = raw
                .split_once('?')
                .map_or((raw, None), |(path, query)| (path, Some(query)));
            crate::uri::scheme(without_query).is_some_and(|scheme| {
                scheme.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'+' | b'.' | b'-')
                }) && without_query
                    .get(scheme.len().saturating_add(1)..)
                    .is_some_and(|body| !body.is_empty())
                    && crate::uri::absolute_valid(without_query, scheme, query)
            })
        }
    };
    (raw.len() <= PUBLICATION_URI_BYTES && !raw.contains('#') && grammar_valid)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn validate_facts(
    path: &str,
    docs: &DocsCandidate,
    target: &PublicationTarget,
    site: &CompletedSite,
    product: &PublicationResource,
    producer: &PublicationProducer,
) -> Result<(), Error> {
    if RepositoryIdentity::new(
        docs.repository.host().to_owned(),
        docs.repository.owner().to_owned(),
        docs.repository.name().to_owned(),
    )
    .as_ref()
        != Some(&docs.repository)
    {
        return fail(&format!("{path}.docs.repository"), ErrorKind::InvalidValue);
    }
    for (field, oid) in [("commit_oid", &docs.commit), ("tree_oid", &docs.tree)] {
        if oid.object_format() != docs.object_format {
            return fail(&format!("{path}.docs.{field}"), ErrorKind::InvalidValue);
        }
    }
    validate_publication_uri(
        &format!("{path}.target.canonical_url"),
        &target.canonical_url,
        PublicationUriKind::CanonicalUrl,
    )?;
    for (field, resource) in [("site.artifact", &site.artifact), ("product", product)] {
        validate_publication_uri(
            &format!("{path}.{field}.uri"),
            &resource.uri,
            PublicationUriKind::Resource,
        )?;
    }
    if !crate::semantic::producer_version_valid(&producer.version) {
        return fail(&format!("{path}.producer.version"), ErrorKind::InvalidValue);
    }
    Ok(())
}

pub(crate) fn docs_value(docs: &DocsCandidate) -> Value {
    object(vec![
        ("repository", repository(&docs.repository)),
        ("object_format", text(docs.object_format.as_ref())),
        ("commit_oid", text(docs.commit.as_str())),
        ("tree_oid", text(docs.tree.as_str())),
        (
            "candidate_identity_digest",
            text(&docs.candidate_identity_digest.to_string()),
        ),
    ])
}

fn target_value(target: &PublicationTarget) -> Value {
    object(vec![
        ("provider", text(target.provider.as_str())),
        ("instance", text(target.instance.as_str())),
        ("environment", text(target.environment.as_str())),
        ("channel", text(target.channel.as_str())),
        ("canonical_url", text(&target.canonical_url)),
    ])
}

fn site_value(site: &CompletedSite) -> Value {
    object(vec![
        ("artifact", resource_value(&site.artifact)),
        ("input_digest", text(&site.input_digest.to_string())),
    ])
}

pub(crate) fn resource_value(resource: &PublicationResource) -> Value {
    object(vec![
        ("uri", text(&resource.uri)),
        ("digest", text(&resource.digest.to_string())),
    ])
}

pub(crate) fn producer_value(producer: &PublicationProducer) -> Value {
    object(vec![
        ("identity", text(producer.identity.as_str())),
        ("version", text(&producer.version)),
        ("context_digest", text(&producer.context_digest.to_string())),
    ])
}
