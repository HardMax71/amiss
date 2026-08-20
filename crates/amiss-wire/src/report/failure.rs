use std::collections::BTreeSet;

use crate::digest::{Digest, hj};
use crate::json::{Value, canonical};
use crate::model::Adapter;

use super::error::error_row;
use super::{
    ADAPTER_CONTRACT_SCHEMA, AnalysisErrorCode, BUILT_IN_POLICY, COMPATIBILITY, ENGINE_CONTRACT,
    ENVELOPE_SCHEMA, ErrorDetail, PAYLOAD_SCHEMA, object, string,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProvenance {
    pub version: String,
    pub digest: Digest,
}

/// Builds the canonical fatal-incomplete wire (`JCS(envelope) || LF`) for an
/// invocation rejection: every detail array empty, every count zero, unavailable
/// evaluation and controls with their reason sets, exit class 2.
///
/// Returns `None` when `codes` is empty or contains a non-invocation code.
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
/// reason, exactly as the invocation form.
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
    let mut errors: Vec<(AnalysisErrorCode, &'static str)> = Vec::new();
    for code in codes {
        let route = code.route()?;
        reasons.push(Value::String(route.evaluation_reason?.into()));
        errors.push((*code, route.phase));
    }
    errors.sort_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));
    let error_rows: Vec<Value> = errors
        .iter()
        .map(|(code, phase)| {
            error_row(
                &ErrorDetail {
                    code: *code,
                    path: None,
                    path_bytes: None,
                    resource: None,
                },
                phase,
            )
        })
        .collect();
    let error_count = i64::try_from(error_rows.len()).ok()?;

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string(COMPATIBILITY)),
        ("engine", engine_block(engine)),
        (
            "evaluation",
            object(vec![
                ("status", string("unavailable")),
                (
                    "request_digest",
                    evaluation_request_digest
                        .map_or(Value::Null, |digest| string(&digest.to_string())),
                ),
                ("reasons", Value::Array(reasons.into_boxed_slice())),
            ]),
        ),
        (
            "controls",
            object(vec![
                ("status", string("unavailable")),
                (
                    "request_digest",
                    controls_request_digest
                        .map_or(Value::Null, |digest| string(&digest.to_string())),
                ),
                ("reasons", Value::Array(Box::new([string("not-parsed")]))),
            ]),
        ),
        ("feedback", object(vec![("status", string("unavailable"))])),
        (
            "result",
            object(vec![
                ("complete", Value::Bool(false)),
                ("status", string("incomplete")),
                ("exit_code", Value::Integer(2)),
                ("finding_count", Value::Integer(0)),
                ("error_count", Value::Integer(error_count)),
            ]),
        ),
        ("summary", zero_summary()),
        ("documents", Value::Array(Box::default())),
        ("observations", Value::Array(Box::default())),
        ("findings", Value::Array(Box::default())),
        ("errors", Value::Array(error_rows.into_boxed_slice())),
    ]);

    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    Some(object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&payload_digest.to_string())),
    ]))
}

/// One adapter's complete contract descriptor and its digest, which every
/// occurrence embeds through its observation-identity input.
#[must_use]
pub fn adapter_contract(engine: &EngineProvenance, adapter: Adapter) -> (Value, Digest) {
    let descriptor = object(vec![
        ("schema", string(ADAPTER_CONTRACT_SCHEMA)),
        ("adapter_id", string(adapter.adapter_id())),
        ("parser_name", string(adapter.parser_name())),
        ("parser_version", string(&engine.version)),
        ("grammar_profile", string(adapter.grammar_profile())),
        (
            "frontmatter_contract",
            string(adapter.frontmatter_contract()),
        ),
        ("source_projection", string(adapter.source_projection())),
        ("structural_address", string(adapter.structural_address())),
    ]);
    let digest = hj(ADAPTER_CONTRACT_SCHEMA, &descriptor);
    (descriptor, digest)
}

/// The complete engine block: contract, version, digest, provenance, policy
/// version, and the three adapter descriptors with their digests.
#[must_use]
pub fn engine_block(engine: &EngineProvenance) -> Value {
    let adapter_rows: Vec<Value> = Adapter::all()
        .map(|adapter| {
            let (descriptor, digest) = adapter_contract(engine, adapter);
            object(vec![
                ("adapter_id", string(adapter.adapter_id())),
                ("contract_descriptor", descriptor),
                ("contract_digest", string(&digest.to_string())),
            ])
        })
        .collect();
    object(vec![
        ("engine_contract", string(ENGINE_CONTRACT)),
        ("engine_version", string(&engine.version)),
        ("engine_digest", string(&engine.digest.to_string())),
        ("action_provenance", object(vec![("kind", string("local"))])),
        ("built_in_policy", string(BUILT_IN_POLICY)),
        ("adapters", Value::Array(adapter_rows.into_boxed_slice())),
    ])
}

fn zero_summary() -> Value {
    let documents = [
        "discovered",
        "outside_document_set",
        "scanned",
        "unsupported",
        "excluded_builtin",
        "unlinked",
        "frontmatter_documents",
        "opaque_mdx_documents",
        "opaque_html_documents",
        "opaque_mdx_regions",
        "opaque_mdx_bytes",
        "opaque_html_regions",
        "opaque_html_bytes",
        "frontmatter_regions",
        "frontmatter_bytes",
    ];
    let references = [
        "extracted",
        "explicit_local",
        "same_repository",
        "external_out_of_scope",
        "unsupported",
        "resolved",
        "missing",
    ];
    let findings = [
        "total",
        "record",
        "warn",
        "fail",
        "introduced",
        "pre_existing",
        "resolved",
        "unknown",
        "not_applicable",
        "debt_tolerated",
        "waived",
        "analysis_errors",
        "unsupported_capabilities",
    ];
    object(vec![
        ("counts_complete", Value::Bool(false)),
        ("documents", zero_counts(&documents)),
        ("references", zero_counts(&references)),
        ("findings", zero_counts(&findings)),
        ("governed_claims", Value::Integer(0)),
        ("unattested_claims", Value::Integer(0)),
    ])
}

fn zero_counts(fields: &[&str]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|field| ((*field).into(), Value::Integer(0)))
            .collect(),
    )
}
