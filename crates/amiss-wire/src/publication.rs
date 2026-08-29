use crate::controls::value::{object, repository, text};
use crate::controls::{decode_enum, decode_repository};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value, canonical_length};
use crate::model::{ArtifactId, Oid, RepositoryIdentity};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/publication-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/publication-plan-payload";
pub const PUBLICATION_DOCUMENT_BYTES: u64 = 65_536;
pub const PUBLICATION_URI_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationPlanEnvelope {
    pub payload: PublicationPlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationPlan {
    pub report_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub target: PublicationTarget,
    pub site: CompletedSite,
    pub product: PublicationResource,
    pub producer: PublicationProducer,
    pub relation: PublicationRelation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsCandidate {
    pub repository: RepositoryIdentity,
    pub commit: Oid,
    pub tree: Oid,
    pub candidate_identity_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationTarget {
    pub provider: ArtifactId,
    pub instance: ArtifactId,
    pub environment: ArtifactId,
    pub channel: ArtifactId,
    pub canonical_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedSite {
    pub artifact: PublicationResource,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationResource {
    pub uri: String,
    pub digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationProducer {
    pub identity: ArtifactId,
    pub version: String,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationRelation {
    pub identity: ArtifactId,
    pub context_digest: Digest,
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
    let value = json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let mut envelope = Obj::new("$", value)?;
    envelope.required("schema", |path, value| {
        de::const_str(path, value, PLAN_ENVELOPE_SCHEMA)
    })?;
    let payload_value = envelope.take("payload")?;
    let payload_digest = envelope.required("payload_digest", de::digest)?;
    envelope.finish()?;
    if hj(PLAN_PAYLOAD_SCHEMA, &payload_value) != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(PublicationPlanEnvelope {
        payload: decode_plan("$.payload", payload_value)?,
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
    let payload = plan_value(input);
    let _validated = decode_plan("$.payload", payload.clone())?;
    let payload_digest = hj(PLAN_PAYLOAD_SCHEMA, &payload);
    let value = object(vec![
        ("schema", text(PLAN_ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", text(&payload_digest.to_string())),
    ]);
    if canonical_length(&value) > PUBLICATION_DOCUMENT_BYTES {
        fail("$", ErrorKind::LimitExceeded)
    } else {
        Ok(value)
    }
}

fn decode_plan(path: &str, value: Value) -> Result<PublicationPlan, Error> {
    let mut plan = Obj::new(path, value)?;
    plan.required("schema", |path, value| {
        de::const_str(path, value, PLAN_PAYLOAD_SCHEMA)
    })?;
    let report_payload_digest = plan.required("report_payload_digest", de::digest)?;
    let docs = plan.required("docs", |path, value| {
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
            commit,
            tree,
            candidate_identity_digest,
        })
    })?;
    let target = plan.required("target", |path, value| {
        let mut target = Obj::new(path, value)?;
        let provider = target.required("provider", decode_identity)?;
        let instance = target.required("instance", decode_identity)?;
        let environment = target.required("environment", decode_identity)?;
        let channel = target.required("channel", decode_identity)?;
        let canonical_url = target.required("canonical_url", |path, value| {
            let raw = decode_bounded_text(path, value, PUBLICATION_URI_BYTES)?;
            if canonical_url_valid(&raw) {
                Ok(raw)
            } else {
                fail(path, ErrorKind::InvalidValue)
            }
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
    let site = plan.required("site", |path, value| {
        let mut site = Obj::new(path, value)?;
        let artifact = site.required("artifact", decode_resource)?;
        let input_digest = site.required("input_digest", de::digest)?;
        site.finish()?;
        Ok(CompletedSite {
            artifact,
            input_digest,
        })
    })?;
    let product = plan.required("product", decode_resource)?;
    let producer = plan.required("producer", |path, value| {
        let mut producer = Obj::new(path, value)?;
        let identity = producer.required("identity", decode_identity)?;
        let version = producer.required("version", decode_producer_version)?;
        let context_digest = producer.required("context_digest", de::digest)?;
        producer.finish()?;
        Ok(PublicationProducer {
            identity,
            version,
            context_digest,
        })
    })?;
    let relation = plan.required("relation", |path, value| {
        let mut relation = Obj::new(path, value)?;
        let identity = relation.required("identity", decode_identity)?;
        let context_digest = relation.required("context_digest", de::digest)?;
        relation.finish()?;
        Ok(PublicationRelation {
            identity,
            context_digest,
        })
    })?;
    plan.finish()?;
    Ok(PublicationPlan {
        report_payload_digest,
        docs,
        target,
        site,
        product,
        producer,
        relation,
    })
}

fn decode_resource(path: &str, value: Value) -> Result<PublicationResource, Error> {
    let mut resource = Obj::new(path, value)?;
    let uri = resource.required("uri", decode_resource_uri)?;
    let digest = resource.required("digest", de::digest)?;
    resource.finish()?;
    Ok(PublicationResource { uri, digest })
}

fn decode_identity(path: &str, value: Value) -> Result<ArtifactId, Error> {
    let raw = de::string(path, value)?;
    ArtifactId::new(raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_producer_version(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    if crate::semantic::producer_version_valid(&raw) {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_bounded_text(path: &str, value: Value, limit: usize) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    if raw.is_empty() || raw.len() > limit || raw.chars().any(char::is_control) {
        fail(path, ErrorKind::InvalidValue)
    } else {
        Ok(raw)
    }
}

fn canonical_url_valid(raw: &str) -> bool {
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
                && authority
                    .rsplit_once(':')
                    .map_or(!authority.is_empty(), |(host, port)| {
                        !host.is_empty() && !host.contains(':') && port.parse::<u16>().is_ok()
                    })
        }
    });
    authority_valid && !raw.contains(['?', '#']) && crate::uri::http_destination_valid(raw)
}

fn decode_resource_uri(path: &str, value: Value) -> Result<String, Error> {
    let raw = decode_bounded_text(path, value, PUBLICATION_URI_BYTES)?;
    let (without_query, query) = raw
        .split_once('?')
        .map_or((raw.as_str(), None), |(path, query)| (path, Some(query)));
    let valid = crate::uri::scheme(without_query).is_some_and(|scheme| {
        scheme.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'.' | b'-')
        }) && without_query
            .get(scheme.len().saturating_add(1)..)
            .is_some_and(|body| !body.is_empty())
            && !raw.contains('#')
            && crate::uri::absolute_valid(without_query, scheme, query)
    });
    if valid {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn plan_value(plan: &PublicationPlan) -> Value {
    object(vec![
        ("schema", text(PLAN_PAYLOAD_SCHEMA)),
        (
            "report_payload_digest",
            text(&plan.report_payload_digest.to_string()),
        ),
        ("docs", docs_value(&plan.docs)),
        ("target", target_value(&plan.target)),
        ("site", site_value(&plan.site)),
        ("product", resource_value(&plan.product)),
        ("producer", producer_value(&plan.producer)),
        ("relation", relation_value(&plan.relation)),
    ])
}

fn docs_value(docs: &DocsCandidate) -> Value {
    object(vec![
        ("repository", repository(&docs.repository)),
        ("object_format", text(docs.commit.object_format().as_ref())),
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

fn resource_value(resource: &PublicationResource) -> Value {
    object(vec![
        ("uri", text(&resource.uri)),
        ("digest", text(&resource.digest.to_string())),
    ])
}

fn producer_value(producer: &PublicationProducer) -> Value {
    object(vec![
        ("identity", text(producer.identity.as_str())),
        ("version", text(&producer.version)),
        ("context_digest", text(&producer.context_digest.to_string())),
    ])
}

fn relation_value(relation: &PublicationRelation) -> Value {
    object(vec![
        ("identity", text(relation.identity.as_str())),
        ("context_digest", text(&relation.context_digest.to_string())),
    ])
}
