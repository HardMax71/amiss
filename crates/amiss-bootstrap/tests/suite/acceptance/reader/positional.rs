use std::sync::LazyLock;

use amiss_bootstrap::supervise::{AcceptanceDefect, Expectations, accept};
use amiss_wire::digest::hb;
use amiss_wire::report::model;
use amiss_wire::requests::CandidateSnapshot;

use super::{accepted_report, corrupt};

static CASES: LazyLock<(Expectations, Vec<Vec<u8>>)> = LazyLock::new(|| {
    let (wire, expectations) = accepted_report();
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let payload = &report.payload;
    let engine = &payload.engine;
    let model::ActionProvenance::Local(action) = &engine.action_provenance else {
        panic!("the fixture has local action provenance");
    };
    let descriptor = &engine.adapters.first().unwrap().contract_descriptor;
    let model::Evaluation::Resolved(evaluation) = &payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let (
        model::BaseSnapshot::Git(base),
        model::Snapshot::Available(CandidateSnapshot::Git(candidate)),
    ) = (&evaluation.base, &evaluation.candidate)
    else {
        panic!("the fixture describes a commit pair");
    };
    let result = &payload.result;
    let summary = &payload.summary;
    let documents = &summary.documents;
    let findings = &summary.findings;
    let references = &summary.references;
    let mut envelope =
        serde_json_canonicalizer::to_vec(&(&report.payload, report.payload_digest, report.schema))
            .unwrap();
    envelope.push(b'\n');
    let shapes = [
        (
            serde_json_canonicalizer::to_string(payload).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &payload.compatibility,
                &payload.controls,
                &payload.documents,
                &payload.engine,
                &payload.errors,
                &payload.evaluation,
                &payload.feedback,
                &payload.findings,
                &payload.observations,
                &payload.result,
                &payload.schema,
                &payload.summary,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(engine).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &engine.action_provenance,
                &engine.adapters,
                &engine.built_in_policy,
                &engine.engine_contract,
                &engine.engine_digest,
                &engine.engine_version,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(action).unwrap(),
            serde_json_canonicalizer::to_string(&(action.kind,)).unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(descriptor).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &descriptor.adapter_id,
                &descriptor.frontmatter_contract,
                &descriptor.grammar_profile,
                &descriptor.parser_name,
                &descriptor.parser_version,
                &descriptor.schema,
                &descriptor.source_projection,
                &descriptor.structural_address,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(evaluation).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &evaluation.base,
                &evaluation.candidate,
                &evaluation.candidate_ref,
                &evaluation.default_branch_ref,
                &evaluation.evaluation_instant,
                &evaluation.event_kind,
                &evaluation.finality,
                &evaluation.forge,
                &evaluation.index_only_materialized_paths,
                &evaluation.materialization,
                &evaluation.mode,
                &evaluation.repository,
                &evaluation.skip_worktree_paths,
                &evaluation.target_ref,
                &evaluation.trusted_time,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(base).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &base.commit_oid,
                &base.kind,
                &base.object_format,
                &base.tree_oid,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(candidate).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &candidate.commit_oid,
                &candidate.kind,
                &candidate.object_format,
                &candidate.tree_oid,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(result).unwrap(),
            serde_json_canonicalizer::to_string(&(
                result.complete,
                result.error_count,
                result.exit_code,
                result.finding_count,
                result.status,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(summary).unwrap(),
            serde_json_canonicalizer::to_string(&(
                summary.counts_complete,
                &summary.documents,
                &summary.findings,
                summary.governed_claims,
                &summary.references,
                summary.unattested_claims,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(documents).unwrap(),
            serde_json_canonicalizer::to_string(&[
                documents.discovered,
                documents.excluded_builtin,
                documents.frontmatter_bytes,
                documents.frontmatter_documents,
                documents.frontmatter_regions,
                documents.opaque_html_bytes,
                documents.opaque_html_documents,
                documents.opaque_html_regions,
                documents.opaque_mdx_bytes,
                documents.opaque_mdx_documents,
                documents.opaque_mdx_regions,
                documents.outside_document_set,
                documents.scanned,
                documents.unlinked,
                documents.unsupported,
            ])
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(findings).unwrap(),
            serde_json_canonicalizer::to_string(&[
                findings.analysis_errors,
                findings.debt_tolerated,
                findings.fail,
                findings.introduced,
                findings.not_applicable,
                findings.pre_existing,
                findings.record,
                findings.resolved,
                findings.total,
                findings.unknown,
                findings.unsupported_capabilities,
                findings.waived,
                findings.warn,
            ])
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_string(references).unwrap(),
            serde_json_canonicalizer::to_string(&[
                references.explicit_local,
                references.external_out_of_scope,
                references.extracted,
                references.missing,
                references.resolved,
                references.same_repository,
                references.unsupported,
            ])
            .unwrap(),
        ),
    ]
    .map(|(object, positional)| corrupt(&report, &object, &positional));
    (
        expectations,
        std::iter::once(envelope).chain(shapes).collect(),
    )
});

#[test]
fn core_objects_cannot_be_replaced_by_positional_arrays() {
    let (expectations, wires) = &*CASES;
    assert_eq!(wires.len(), 13);
    for (index, wire) in wires.iter().enumerate() {
        assert_eq!(
            accept(wire, expectations),
            Err(AcceptanceDefect::Shape),
            "positional case {index}"
        );
    }
    assert_eq!(
        hb("amiss/test-bootstrap-positional-fixtures", &wires.concat()).to_string(),
        "sha256:84abd873cb1a52a1fc4dbcdd70e777b9a0a1bd026c0c3df4ba2796d688a58e59"
    );
}
