use std::collections::BTreeMap;

use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj};
use crate::digest::Digest;
use crate::json::Value;
use crate::publication::{DocsCandidate, PublicationProducer, docs_value, producer_value};

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
    pub target: LocalePageInventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalePageInventory {
    pub input_digest: Digest,
    pub complete: bool,
    pub pages: BTreeMap<String, Digest>,
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
    let source = evidence.required("source", decode_inventory)?;
    let target = evidence.required("target", decode_inventory)?;
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

fn decode_inventory(path: &str, value: Value) -> Result<LocalePageInventory, Error> {
    let mut inventory = Obj::new(path, value)?;
    let input_digest = inventory.required("input_digest", de::digest)?;
    let complete = inventory.required("complete", de::boolean)?;
    let pages = inventory.required("pages", |path, value| {
        de::sorted_map(path, value, PAGE_ITEMS_LIMIT, |path, value| {
            let mut page = Obj::new(path, value)?;
            let key = page.required("key", |path, value| {
                de::bounded_text(path, value, PAGE_KEY_BYTES)
            })?;
            let resource_digest = page.required("resource_digest", de::digest)?;
            page.finish()?;
            Ok((key, resource_digest))
        })
    })?;
    inventory.finish()?;
    Ok(LocalePageInventory {
        input_digest,
        complete,
        pages,
    })
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
        ("source", inventory_value(&evidence.source)),
        ("target", inventory_value(&evidence.target)),
    ])
}

fn inventory_value(inventory: &LocalePageInventory) -> Value {
    let pages = inventory
        .pages
        .iter()
        .map(|(key, resource_digest)| {
            object(vec![
                ("key", text(key)),
                ("resource_digest", text(&resource_digest.to_string())),
            ])
        })
        .collect();
    object(vec![
        ("input_digest", text(&inventory.input_digest.to_string())),
        ("complete", Value::Bool(inventory.complete)),
        ("pages", Value::array(pages)),
    ])
}
