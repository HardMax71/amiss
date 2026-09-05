use std::collections::BTreeSet;

use crate::digest::{Digest, hj_serde};
use crate::model::{Adapter, RepoPath};
use strum::IntoEnumIterator;

use super::model;
use super::{ADAPTER_CONTRACT_SCHEMA, AnalysisErrorCode, ErrorDetail, PAYLOAD_SCHEMA, error_row};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProvenance {
    pub version: String,
    pub digest: Digest,
}

/// Returns no envelope when the codes do not identify an invocation refusal.
///
/// # Errors
/// Returns serialization failures without partial output or a substitute digest.
pub fn invocation_failure_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> std::io::Result<Option<Vec<u8>>> {
    unavailable_evaluation_wire(engine, codes, None, None)
}

/// Returns no envelope when the codes do not identify an invocation refusal.
///
/// # Errors
/// Returns serialization failures without partial output or a substitute digest.
pub fn invocation_failure_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> serde_json::Result<Option<model::ReportEnvelope<model::ReportPayload<RepoPath>>>> {
    unavailable_evaluation_envelope(engine, codes, None, None)
}

/// Returns no envelope when the codes do not identify an invocation refusal.
///
/// # Errors
/// Returns serialization failures without partial output or a substitute digest.
pub fn unavailable_evaluation_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> std::io::Result<Option<Vec<u8>>> {
    unavailable_evaluation_envelope(
        engine,
        codes,
        evaluation_request_digest,
        controls_request_digest,
    )?
    .map(|envelope| {
        let mut wire = Vec::new();
        super::emit_report(&envelope, &mut wire)?;
        Ok(wire)
    })
    .transpose()
}

/// Returns no envelope when the codes do not identify an invocation refusal.
///
/// # Errors
/// Returns serialization failures without partial output or a substitute digest.
pub fn unavailable_evaluation_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> serde_json::Result<Option<model::ReportEnvelope<model::ReportPayload<RepoPath>>>> {
    if codes.is_empty() {
        return Ok(None);
    }
    let Some(reasons) = codes
        .iter()
        .map(|code| code.route()?.evaluation_reason)
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let mut errors: Vec<_> = codes.iter().copied().collect();
    errors.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    let errors: Vec<_> = errors
        .into_iter()
        .map(|code| {
            error_row(&ErrorDetail {
                code,
                path: None,
                path_bytes: None,
                resource: None,
            })
        })
        .collect();
    let payload = model::ReportPayload {
        schema: model::ReportPayloadSchema::Current,
        compatibility: model::ReportCompatibility::One,
        engine: engine_block(engine)?,
        evaluation: model::Evaluation::Unavailable(model::UnavailableEvaluation {
            status: model::UnavailableStatus::Unavailable,
            request_digest: evaluation_request_digest,
            reasons,
        }),
        controls: model::Controls::Unavailable(model::UnavailableControls {
            status: model::UnavailableStatus::Unavailable,
            request_digest: controls_request_digest,
            reasons: vec![model::ControlsUnavailableReason::NotParsed],
        }),
        feedback: model::Feedback::Unavailable(model::UnavailableFeedback {
            status: model::UnavailableStatus::Unavailable,
        }),
        result: model::ReportResult {
            complete: false,
            status: model::ReportStatus::Incomplete,
            exit_code: 2,
            finding_count: 0,
            error_count: u64::try_from(errors.len()).unwrap_or(u64::MAX),
        },
        summary: model::Summary {
            counts_complete: false,
            documents: model::DocumentCounts::default(),
            references: model::ReferenceCounts::default(),
            findings: model::FindingCounts::default(),
            governed_claims: 0,
            unattested_claims: 0,
        },
        documents: Vec::new(),
        observations: Vec::new(),
        findings: Vec::new(),
        errors,
    };
    let payload_digest = hj_serde(PAYLOAD_SCHEMA, |writer| {
        serde_json::to_writer(writer, &payload)
    })?;
    Ok(Some(model::ReportEnvelope {
        schema: model::ReportEnvelopeSchema::Current,
        payload,
        payload_digest,
    }))
}

/// The adapter descriptor and the digest of its canonical fields.
///
/// # Errors
/// Returns serialization failures without producing a partial digest.
pub fn adapter_contract(
    engine: &EngineProvenance,
    adapter: Adapter,
) -> serde_json::Result<(model::AdapterContractDescriptor, Digest)> {
    let metadata = adapter.metadata();
    let descriptor = model::AdapterContractDescriptor {
        schema: model::AdapterContractSchema::Current,
        adapter_id: adapter,
        parser_name: metadata.parser_name.to_owned(),
        parser_version: engine.version.clone(),
        grammar_profile: metadata.grammar_profile.to_owned(),
        frontmatter_contract: metadata.frontmatter_contract,
        source_projection: metadata.source_projection,
        structural_address: match metadata.structural_address {
            Some(model::AddressKind::AsciidocBlockPath) => {
                model::StructuralAddressKind::AsciidocBlockPath
            }
            Some(model::AddressKind::MarkdownAstNodePath) => {
                model::StructuralAddressKind::MarkdownAstNodePath
            }
            Some(model::AddressKind::MdxAstNodePath) => {
                model::StructuralAddressKind::MdxAstNodePath
            }
            Some(model::AddressKind::RstBlockPath) => model::StructuralAddressKind::RstBlockPath,
            None => model::StructuralAddressKind::None,
        },
    };
    let digest = hj_serde(ADAPTER_CONTRACT_SCHEMA, |writer| {
        serde_json::to_writer(writer, &descriptor)
    })?;
    Ok((descriptor, digest))
}

/// The engine identity and its complete adapter contracts.
///
/// # Errors
/// Returns a failure to serialize an adapter descriptor.
pub fn engine_block(engine: &EngineProvenance) -> serde_json::Result<model::Engine> {
    let adapters = Adapter::iter()
        .map(|adapter_id| {
            let (contract_descriptor, contract_digest) = adapter_contract(engine, adapter_id)?;
            Ok(model::ReportAdapter {
                adapter_id,
                contract_descriptor,
                contract_digest,
            })
        })
        .collect::<serde_json::Result<_>>()?;
    Ok(model::Engine {
        engine_contract: model::EngineContract::Current,
        engine_version: engine.version.clone(),
        engine_digest: engine.digest,
        action_provenance: model::ActionProvenance::Local(model::LocalActionProvenance {
            kind: model::LocalActionKind::Local,
        }),
        built_in_policy: model::BuiltInPolicy::Current,
        adapters,
    })
}
