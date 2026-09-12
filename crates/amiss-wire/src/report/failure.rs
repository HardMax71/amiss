use super::error::{ErrorRow, error_projection};
use super::{
    ADAPTER_CONTRACT_SCHEMA, AnalysisErrorCode, BUILT_IN_POLICY, COMPATIBILITY, ENGINE_CONTRACT,
    ENVELOPE_SCHEMA, ErrorDetail, PAYLOAD_SCHEMA,
};
use crate::codec;
use crate::de::Error;
use crate::digest::Digest;
use crate::json::{Value, canonical};
use crate::model::Adapter;
use serde::Serialize;
use std::collections::BTreeSet;
use strum::IntoEnumIterator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProvenance {
    pub version: String,
    pub digest: Digest,
}

/// Builds the canonical fatal-incomplete wire (`JCS(envelope) || LF`) for an
/// invocation rejection: every detail array empty, every count zero, unavailable
/// evaluation and controls with their reason sets, exit class 2.
///
/// Returns `None` when `codes` is empty or contains a non-invocation code, or emission fails.
#[must_use]
pub fn invocation_failure_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> Option<Vec<u8>> {
    unavailable_evaluation_wire(engine, codes, None, None)
}

/// The envelope value behind [`invocation_failure_wire`], for emission
/// through the reserved fatal serializer.
#[must_use]
pub fn invocation_failure_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> Option<Value> {
    unavailable_evaluation_envelope(engine, codes, None, None)
}

/// The fatal unavailable-evaluation envelope for the request-wire lane: the
/// same closed projection, carrying each request's diagnostic digest where
/// its byte stream was completely captured.
///
/// Returns `None` when no code is supplied or a code has no evaluation
/// reason, or the projection cannot be emitted, exactly as the invocation form.
#[must_use]
pub fn unavailable_evaluation_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> Option<Vec<u8>> {
    let envelope = unavailable_evaluation_envelope(
        engine,
        codes,
        evaluation_request_digest,
        controls_request_digest,
    )?;
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    Some(wire)
}

/// The envelope value behind [`unavailable_evaluation_wire`], for emission
/// through the reserved fatal serializer.
#[must_use]
pub fn unavailable_evaluation_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> Option<Value> {
    if codes.is_empty() {
        return None;
    }
    let mut reasons = Vec::new();
    let mut errors = Vec::new();
    for code in codes {
        let route = code.route()?;
        reasons.push(route.evaluation_reason?);
        errors.push(ErrorDetail {
            code: *code,
            path: None,
            path_bytes: None,
            resource: None,
        });
    }
    errors.sort_by(|a, b| a.code.as_ref().cmp(b.code.as_ref()));
    let error_rows = errors
        .iter()
        .map(|detail| error_projection(detail, detail.phase()))
        .collect();
    let payload = FailurePayload {
        compatibility: COMPATIBILITY,
        controls: Unavailable {
            reasons: vec!["not-parsed"],
            request_digest: controls_request_digest,
            status: "unavailable",
        },
        documents: [],
        engine: engine_projection(engine).ok()?,
        errors: error_rows,
        evaluation: Unavailable {
            reasons,
            request_digest: evaluation_request_digest,
            status: "unavailable",
        },
        feedback: Status {
            status: "unavailable",
        },
        findings: [],
        observations: [],
        result: FailureResult {
            complete: false,
            error_count: errors.len(),
            exit_code: 2,
            finding_count: 0,
            status: "incomplete",
        },
        schema: PAYLOAD_SCHEMA,
        summary: Summary::default(),
    };
    let payload_digest = codec::digest(PAYLOAD_SCHEMA, &payload).ok()?;
    codec::to_value(&FailureEnvelope {
        payload,
        payload_digest,
        schema: ENVELOPE_SCHEMA,
    })
    .ok()
}

#[derive(Serialize)]
struct FailureEnvelope<'a> {
    payload: FailurePayload<'a>,
    payload_digest: Digest,
    schema: &'static str,
}

#[derive(Serialize)]
struct FailurePayload<'a> {
    compatibility: &'static str,
    controls: Unavailable,
    documents: [(); 0],
    engine: EngineBlock<'a>,
    errors: Vec<ErrorRow<'a>>,
    evaluation: Unavailable,
    feedback: Status,
    findings: [(); 0],
    observations: [(); 0],
    result: FailureResult,
    schema: &'static str,
    summary: Summary,
}

#[derive(Serialize)]
struct Unavailable {
    reasons: Vec<&'static str>,
    request_digest: Option<Digest>,
    status: &'static str,
}

#[derive(Serialize)]
struct Status {
    status: &'static str,
}

#[derive(Serialize)]
struct FailureResult {
    complete: bool,
    error_count: usize,
    exit_code: u8,
    finding_count: u8,
    status: &'static str,
}

#[derive(Serialize)]
struct AdapterDescriptor<'a> {
    adapter_id: Adapter,
    frontmatter_contract: &'static str,
    grammar_profile: &'static str,
    parser_name: &'static str,
    parser_version: &'a str,
    schema: &'static str,
    source_projection: &'static str,
    structural_address: &'static str,
}

fn adapter_descriptor(engine: &EngineProvenance, adapter: Adapter) -> AdapterDescriptor<'_> {
    let metadata = adapter.metadata();
    AdapterDescriptor {
        adapter_id: adapter,
        frontmatter_contract: metadata.frontmatter_contract,
        grammar_profile: metadata.grammar_profile,
        parser_name: metadata.parser_name,
        parser_version: &engine.version,
        schema: ADAPTER_CONTRACT_SCHEMA,
        source_projection: metadata.source_projection,
        structural_address: metadata.structural_address,
    }
}

/// One adapter's complete contract descriptor and its digest.
///
/// # Errors
///
/// The contract cannot be represented in the strict JSON profile.
pub fn adapter_contract(
    engine: &EngineProvenance,
    adapter: Adapter,
) -> Result<(Value, Digest), Error> {
    let descriptor = adapter_descriptor(engine, adapter);
    let digest = codec::digest(ADAPTER_CONTRACT_SCHEMA, &descriptor)?;
    Ok((codec::to_value(&descriptor)?, digest))
}

#[derive(Serialize)]
struct AdapterContract<'a> {
    adapter_id: Adapter,
    contract_descriptor: AdapterDescriptor<'a>,
    contract_digest: Digest,
}

#[derive(Serialize)]
struct EngineBlock<'a> {
    action_provenance: LocalProvenance,
    adapters: Vec<AdapterContract<'a>>,
    built_in_policy: &'static str,
    engine_contract: &'static str,
    engine_digest: Digest,
    engine_version: &'a str,
}

#[derive(Serialize)]
struct LocalProvenance {
    kind: &'static str,
}

fn engine_projection(engine: &EngineProvenance) -> Result<EngineBlock<'_>, Error> {
    let adapters = Adapter::iter()
        .map(|adapter| {
            let descriptor = adapter_descriptor(engine, adapter);
            Ok(AdapterContract {
                adapter_id: adapter,
                contract_digest: codec::digest(ADAPTER_CONTRACT_SCHEMA, &descriptor)?,
                contract_descriptor: descriptor,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(EngineBlock {
        action_provenance: LocalProvenance { kind: "local" },
        adapters,
        built_in_policy: BUILT_IN_POLICY,
        engine_contract: ENGINE_CONTRACT,
        engine_digest: engine.digest,
        engine_version: &engine.version,
    })
}

/// The engine block with provenance, policy, and adapter contracts.
///
/// # Errors
///
/// A contract cannot be represented in the strict JSON profile.
pub fn engine_block(engine: &EngineProvenance) -> Result<Value, Error> {
    codec::to_value(&engine_projection(engine)?)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DocumentCounts {
    pub discovered: u64,
    pub excluded_builtin: u64,
    pub frontmatter_bytes: u64,
    pub frontmatter_documents: u64,
    pub frontmatter_regions: u64,
    pub opaque_html_bytes: u64,
    pub opaque_html_documents: u64,
    pub opaque_html_regions: u64,
    pub opaque_mdx_bytes: u64,
    pub opaque_mdx_documents: u64,
    pub opaque_mdx_regions: u64,
    pub outside_document_set: u64,
    pub scanned: u64,
    pub unlinked: u64,
    pub unsupported: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReferenceCounts {
    pub explicit_local: u64,
    pub external_out_of_scope: u64,
    pub extracted: u64,
    pub missing: u64,
    pub resolved: u64,
    pub same_repository: u64,
    pub unsupported: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FindingCounts {
    pub analysis_errors: u64,
    pub debt_tolerated: u64,
    pub fail: u64,
    pub introduced: u64,
    pub not_applicable: u64,
    pub pre_existing: u64,
    pub record: u64,
    pub resolved: u64,
    pub total: u64,
    pub unknown: u64,
    pub unsupported_capabilities: u64,
    pub waived: u64,
    pub warn: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub counts_complete: bool,
    pub documents: DocumentCounts,
    pub findings: FindingCounts,
    pub governed_claims: u64,
    pub references: ReferenceCounts,
    pub unattested_claims: u64,
}
