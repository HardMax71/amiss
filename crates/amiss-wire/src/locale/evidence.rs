use std::collections::BTreeMap;

use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::{
    DocsCandidate, PublicationProducer, decode_identity, docs_value, producer_value,
};

use super::{LocaleCoverageScope, PAGE_KEY_BYTES, decode_facts, scope_value};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-evidence-payload";
pub const EVIDENCE_DOCUMENT_BYTES: u64 = crate::semantic::SEMANTIC_EVIDENCE_BYTES;
pub const PAGE_ITEMS_LIMIT: usize = crate::semantic::SEMANTIC_OBSERVATIONS_LIMIT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageEvidenceEnvelope {
    pub payload: LocaleCoverageEvidence,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageEvidence {
    pub plan_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub scope: LocaleCoverageScope,
    pub producer: PublicationProducer,
    pub source: LocalePageInventory,
    pub target: LocaleTargetInventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalePageInventory {
    pub input_digest: Digest,
    pub complete: bool,
    pub pages: BTreeMap<String, Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleTargetInventory {
    pub input_digest: Digest,
    pub complete: bool,
    pub pages: BTreeMap<String, LocaleTargetPage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleTargetPage {
    pub resource_digest: Digest,
    pub origin: LocaleTargetOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocaleTargetOrigin {
    TargetResource {
        based_on_source_digest: Option<Digest>,
    },
    Fallback {
        class: ArtifactId,
        source_resource_digest: Digest,
    },
}

struct Inventory<T> {
    input_digest: Digest,
    complete: bool,
    pages: BTreeMap<String, T>,
}

/// Parses one closed, digest-bound pair of locale page inventories.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, unknown fields, invalid bindings, unsorted,
/// repeated, or oversized page sets, invalid page keys, or a payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<LocaleCoverageEvidenceEnvelope, Error> {
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        EVIDENCE_DOCUMENT_BYTES,
        decode_evidence,
    )?;
    Ok(LocaleCoverageEvidenceEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for one pair of locale page inventories.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_evidence`] enforces or the
/// encoded document exceeds its byte ceiling.
pub fn evidence(input: &LocaleCoverageEvidence) -> Result<Value, Error> {
    let validated = decode_evidence("$.payload", evidence_value(input))?;
    let payload = evidence_value(&validated);
    crate::bounded_envelope::build(
        payload,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        EVIDENCE_DOCUMENT_BYTES,
    )
}

fn decode_evidence(path: &str, value: Value) -> Result<LocaleCoverageEvidence, Error> {
    let mut evidence = Obj::new(path, value)?;
    evidence.required("schema", |path, value| {
        de::const_str(path, value, EVIDENCE_PAYLOAD_SCHEMA)
    })?;
    let plan_payload_digest = evidence.required("plan_payload_digest", de::digest)?;
    let facts = decode_facts(&mut evidence)?;
    let source = evidence.required("source", |path, value| {
        let inventory = decode_inventory(path, value, |page| {
            page.required("resource_digest", de::digest)
        })?;
        Ok(LocalePageInventory {
            input_digest: inventory.input_digest,
            complete: inventory.complete,
            pages: inventory.pages,
        })
    })?;
    let target = evidence.required("target", |path, value| {
        let inventory = decode_inventory(path, value, |page| {
            let resource_digest = page.required("resource_digest", de::digest)?;
            let origin = page.required("origin", decode_origin)?;
            Ok(LocaleTargetPage {
                resource_digest,
                origin,
            })
        })?;
        Ok(LocaleTargetInventory {
            input_digest: inventory.input_digest,
            complete: inventory.complete,
            pages: inventory.pages,
        })
    })?;
    evidence.finish()?;
    source
        .pages
        .len()
        .checked_add(target.pages.len())
        .filter(|total| *total <= PAGE_ITEMS_LIMIT)
        .ok_or_else(|| Error::new(&format!("{path}.target.pages"), ErrorKind::LimitExceeded))?;
    Ok(LocaleCoverageEvidence {
        plan_payload_digest,
        docs: facts.docs,
        scope: facts.scope,
        producer: facts.producer,
        source,
        target,
    })
}

fn decode_inventory<T>(
    path: &str,
    value: Value,
    mut decode_page: impl FnMut(&mut Obj) -> Result<T, Error>,
) -> Result<Inventory<T>, Error> {
    let mut inventory = Obj::new(path, value)?;
    let input_digest = inventory.required("input_digest", de::digest)?;
    let complete = inventory.required("complete", de::boolean)?;
    let pages = inventory.required("pages", |path, value| {
        de::sorted_map(path, value, PAGE_ITEMS_LIMIT, |path, value| {
            let mut page = Obj::new(path, value)?;
            let key = page.required("key", |path, value| {
                de::bounded_text(path, value, PAGE_KEY_BYTES)
            })?;
            let item = decode_page(&mut page)?;
            page.finish()?;
            Ok((key, item))
        })
    })?;
    inventory.finish()?;
    Ok(Inventory {
        input_digest,
        complete,
        pages,
    })
}

fn decode_origin(path: &str, value: Value) -> Result<LocaleTargetOrigin, Error> {
    let mut origin = Obj::new(path, value)?;
    let kind_path = origin.field("kind");
    let kind = de::string(&kind_path, origin.take("kind")?)?;
    match kind.as_str() {
        "target-resource" => {
            let based_on_path = origin.field("based_on_source_digest");
            let based_on_source_digest = de::nullable(origin.take("based_on_source_digest")?)
                .map(|value| de::digest(&based_on_path, value))
                .transpose()?;
            origin.finish()?;
            Ok(LocaleTargetOrigin::TargetResource {
                based_on_source_digest,
            })
        }
        "fallback" => {
            let class = origin.required("class", decode_identity)?;
            let source_resource_digest = origin.required("source_resource_digest", de::digest)?;
            origin.finish()?;
            Ok(LocaleTargetOrigin::Fallback {
                class,
                source_resource_digest,
            })
        }
        _ => de::fail(&kind_path, ErrorKind::InvalidValue),
    }
}

fn evidence_value(evidence: &LocaleCoverageEvidence) -> Value {
    object(vec![
        ("schema", text(EVIDENCE_PAYLOAD_SCHEMA)),
        (
            "plan_payload_digest",
            text(&evidence.plan_payload_digest.to_string()),
        ),
        ("docs", docs_value(&evidence.docs)),
        ("scope", scope_value(&evidence.scope)),
        ("producer", producer_value(&evidence.producer)),
        (
            "source",
            inventory_value(
                evidence.source.input_digest,
                evidence.source.complete,
                &evidence.source.pages,
                |key, resource_digest| {
                    object(vec![
                        ("key", text(key)),
                        ("resource_digest", text(&resource_digest.to_string())),
                    ])
                },
            ),
        ),
        (
            "target",
            inventory_value(
                evidence.target.input_digest,
                evidence.target.complete,
                &evidence.target.pages,
                |key, page| {
                    object(vec![
                        ("key", text(key)),
                        ("resource_digest", text(&page.resource_digest.to_string())),
                        (
                            "origin",
                            match &page.origin {
                                LocaleTargetOrigin::TargetResource {
                                    based_on_source_digest,
                                } => object(vec![
                                    ("kind", text("target-resource")),
                                    (
                                        "based_on_source_digest",
                                        based_on_source_digest.map_or(Value::Null, |digest| {
                                            text(&digest.to_string())
                                        }),
                                    ),
                                ]),
                                LocaleTargetOrigin::Fallback {
                                    class,
                                    source_resource_digest,
                                } => object(vec![
                                    ("kind", text("fallback")),
                                    ("class", text(class.as_str())),
                                    (
                                        "source_resource_digest",
                                        text(&source_resource_digest.to_string()),
                                    ),
                                ]),
                            },
                        ),
                    ])
                },
            ),
        ),
    ])
}

fn inventory_value<T>(
    input_digest: Digest,
    complete: bool,
    pages: &BTreeMap<String, T>,
    encode_page: impl Fn(&str, &T) -> Value,
) -> Value {
    let pages = pages
        .iter()
        .map(|(key, page)| encode_page(key, page))
        .collect();
    object(vec![
        ("input_digest", text(&input_digest.to_string())),
        ("complete", Value::Bool(complete)),
        ("pages", Value::array(pages)),
    ])
}
