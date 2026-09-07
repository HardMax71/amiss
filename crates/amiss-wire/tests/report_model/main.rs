mod producer_facts;
mod producer_paths;
mod projection;

use amiss_wire::report::model::{
    AnalysisError, BaseSnapshot, Controls, DocumentResult, Engine, Evaluation, ExceptionDiagnostic,
    Feedback, Finding, FindingFactEvidence, FindingKeyScope, MissingResolution,
    ObservationComparison, ProjectionDifference, ProjectionSource, ReportEnvelope, Resolution,
    ResolutionTarget, Snapshot, Summary, VersionScope,
};
use amiss_wire::requests::CandidateSnapshot;

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn published_provenance_blocks_match_the_models() {
    let document: serde_json::Value = serde_json::from_slice(REPORT).unwrap();
    let payload = document.get("payload").unwrap();
    let _: Controls = serde_json::from_value(payload.get("controls").unwrap().clone()).unwrap();
    let _: Engine = serde_json::from_value(payload.get("engine").unwrap().clone()).unwrap();
    let _: Evaluation = serde_json::from_value(payload.get("evaluation").unwrap().clone()).unwrap();
    let _: Summary = serde_json::from_value(payload.get("summary").unwrap().clone()).unwrap();
    let _: Vec<AnalysisError> =
        serde_json::from_value(payload.get("errors").unwrap().clone()).unwrap();
    let _: Vec<DocumentResult> =
        serde_json::from_value(payload.get("documents").unwrap().clone()).unwrap();
    let _: Vec<ObservationComparison> =
        serde_json::from_value(payload.get("observations").unwrap().clone()).unwrap();
    let _: Feedback = serde_json::from_value(payload.get("feedback").unwrap().clone()).unwrap();
    let _: Vec<Finding> = serde_json::from_value(payload.get("findings").unwrap().clone()).unwrap();
    let _: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
}

#[test]
fn entire_report_streams_in_canonical_order() {
    let envelope: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    assert_eq!(
        serde_json::to_vec(&envelope).unwrap(),
        REPORT.strip_suffix(b"\n").unwrap_or(REPORT),
    );
}

#[test]
fn report_snapshots_refuse_unknown_members() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let encoded = serde_json::to_string(&report).unwrap();
    let Evaluation::Resolved(evaluation) = report.payload.evaluation else {
        panic!("the report fixture resolves its snapshots");
    };
    for snapshot in [
        serde_json::to_string(&evaluation.base).unwrap(),
        serde_json::to_string(&evaluation.candidate).unwrap(),
    ] {
        let invalid = snapshot.replacen('{', "{\"future_member\":true,", 1);
        let changed = encoded.replace(&snapshot, &invalid);
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<ReportEnvelope>(&changed).is_err());
    }
    let identity: amiss_wire::requests::CandidateIdentity = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/candidate-identity-index.json"
    ))
    .unwrap();
    let snapshot = serde_json::to_string(&identity.candidate).unwrap();
    let invalid = snapshot.replacen('{', "{\"future_member\":true,", 1);
    assert!(serde_json::from_str::<Snapshot>(&invalid).is_err());
}

#[test]
fn every_report_variant_streams_in_canonical_order() -> Result<(), Box<dyn std::error::Error>> {
    const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    const OID: &str = "0000000000000000000000000000000000000000";

    let git = format!(
        r#"{{"commit_oid":"{OID}","kind":"git-commit","object_format":"sha1","tree_oid":"{OID}"}}"#,
    );
    assert_canonical::<BaseSnapshot>(&git)?;
    assert_canonical::<CandidateSnapshot>(&git)?;
    assert_canonical::<Snapshot>(&git)?;
    let synthetic = format!(
        r#"{{"base_commit_oid":"{OID}","base_object_format":"sha1","entry_count":0,"identity_scope":"complete-logical-index","index_projection_digest":"{DIGEST}","kind":"index","snapshot_digest":"{DIGEST}","snapshot_schema":"amiss/scanner-snapshot"}}"#,
    );
    assert_canonical::<CandidateSnapshot>(&synthetic)?;
    assert_canonical::<Snapshot>(&synthetic)?;
    let unavailable = r#"{"kind":"unavailable","reasons":["not-evaluated"],"request_digest":null}"#;
    assert_canonical::<BaseSnapshot>(unavailable)?;
    assert_canonical::<Snapshot>(unavailable)?;

    for template in [
        r#"{"content":{"kind":"lfs-pointer","raw_digest":"$digest"},"kind":"blob","mode":"100644","path":"a.md"}"#,
        r#"{"kind":"tree","path":"a"}"#,
    ] {
        assert_canonical::<ResolutionTarget>(&template.replace("$digest", DIGEST))?;
    }
    for wire in [
        r#"{"near":null,"path":"a.md","reason":"heading-anchor-not-found"}"#,
        r#"{"reason":"label-not-declared"}"#,
        r#"{"path":"a.md","reason":"line-fragment-out-of-range"}"#,
        r#"{"near":null,"path":"a.md","reason":"path-not-found"}"#,
    ] {
        assert_canonical::<MissingResolution>(wire)?;
    }
    for template in [
        r#"{"commit_oid":"$oid","kind":"known-commit","path":"a.md"}"#,
        r#"{"kind":"known-path","path":"a.md"}"#,
        r#"{"kind":"unknown-path"}"#,
    ] {
        assert_canonical::<VersionScope>(&template.replace("$oid", OID))?;
    }
    for wire in [
        r#"{"declared_by":".gitignore","kind":"declared-untracked","path":"a.md"}"#,
        r#"{"kind":"external","reason":"url"}"#,
        r#"{"kind":"invalid","reason":"syntax"}"#,
        r#"{"kind":"missing","path":"a.md","reason":"line-fragment-out-of-range"}"#,
        r#"{"kind":"resolved","target":{"kind":"tree","path":"a"}}"#,
        r#"{"kind":"type-mismatch","target":{"kind":"tree","path":"a"}}"#,
        r#"{"kind":"unsupported-semantics","reason":"network-path"}"#,
        r#"{"kind":"unsupported-target","path":"a","reason":"symlink"}"#,
        r#"{"kind":"unsupported-version","scope":{"kind":"unknown-path"}}"#,
    ] {
        assert_canonical::<Resolution>(wire)?;
    }
    for template in [
        r#"{"control_path":null,"kind":"control","rule_id":"rule"}"#,
        r#"{"document":"a.md","kind":"document"}"#,
        r#"{"kind":"observation","observation_id":"$digest"}"#,
        r#"{"document":"a.md","kind":"reference","normalized_target_intent":{"fragment_digest":null,"kind":"repository-path","path":"a.md","query_digest":null,"target_kind":"blob"},"occurrence":{"kind":"source-projection","source_projection_digest":"$digest"},"source_construct":"markdown-inline-link"}"#,
    ] {
        assert_canonical::<FindingKeyScope>(&template.replace("$digest", DIGEST))?;
    }
    for wire in [
        r#"{"first_line":1,"kind":"blob-lines","last_line":2,"path":"a.md"}"#,
        r#"{"end_marker":"end","kind":"named-region","path":"a.md","start_marker":"start"}"#,
        r#"{"kind":"record-set","set":"records"}"#,
        r#"{"key":"name","kind":"record-value","set":"records"}"#,
        r#"{"kind":"tree-paths","maximum_depth":1,"root":"docs"}"#,
    ] {
        assert_canonical::<ProjectionSource>(wire)?;
    }
    for wire in [
        r#"{"expected_count":1,"kind":"count","observed_count":null}"#,
        r#"{"expected_records":1,"extra_omitted":0,"extra_preview":[],"extra_records":0,"kind":"rows","missing_omitted":0,"missing_preview":[],"missing_records":0,"observed_records":1,"ordering_only":false}"#,
    ] {
        assert_canonical::<ProjectionDifference>(wire)?;
    }
    for template in [
        r#"{"accepted_fact_digest":"$digest","adoption_tree":{"object_format":"sha1","tree_oid":"$oid"},"created_at":"2026-01-01T00:00:00Z","current_fact_digest":"$digest","debt_id":"debt","debt_snapshot_digest":"$digest","expires_at":"2026-01-02T00:00:00Z","kind":"debt","owner":"team:docs","reason":"reason"}"#,
        r#"{"authorized_fact_digest":"$digest","candidate_tree":{"object_format":"sha1","tree_oid":"$oid"},"created_at":"2026-01-01T00:00:00Z","current_fact_digest":null,"expires_at":"2026-01-02T00:00:00Z","finding_key":"$digest","issuer":"service:amiss","kind":"waiver","not_before":"2026-01-01T00:00:00Z","owner":"team:docs","reason":"reason","residual_disposition":"warn","waiver_bundle_digest":"$digest","waiver_id":"waiver"}"#,
    ] {
        let wire = template.replace("$digest", DIGEST).replace("$oid", OID);
        assert_canonical::<ExceptionDiagnostic>(&wire)?;
    }
    for template in [
        r#"{"claim_digest":"$digest","destination":"/new","kind":"broken-redirect","reason":"missing-route","route":"/old","source":"a.md"}"#,
        r#"{"claim_kind":"value","expected_digest":"$digest","kind":"claim","line":1,"name":"version","observed":"line-differs","observed_digest":null,"sources":[],"target_path":"a.md"}"#,
        r#"{"base_control_digest":null,"base_control_state":null,"candidate_control_digest":null,"candidate_control_state":null,"control_path":null,"exception":null,"kind":"control","rule_id":"rule"}"#,
        r#"{"document_result":{"base":null,"candidate":null,"change":"unchanged","classification":"structured-markdown","path":"a.md"},"kind":"document"}"#,
        r#"{"claim_digests":["$digest"],"kind":"duplicate-route","route":"/docs","sources":["a.md"]}"#,
        r#"{"comparison":{"alternatives":{"base":[],"candidate":[]},"base":null,"candidate":null,"correlation":"none","correlation_reason":"new-observation","impact":"not-applicable","source_change":"unknown","target_change":"not-comparable"},"kind":"observation"}"#,
        r#"{"expected_bytes":null,"expected_digest":null,"kind":"projection","name":"names","observed":"sink-absent","observed_bytes":null,"observed_digest":null,"projection":"sorted-rows-v1","sink":"previous-code","source":{"kind":"record-set","set":"records"},"sources":[]}"#,
        r#"{"kind":"reference","occurrence_multiplicity":1,"resolution":{"kind":"external","reason":"url"}}"#,
    ] {
        assert_canonical::<FindingFactEvidence>(&template.replace("$digest", DIGEST))?;
    }
    Ok(())
}

fn assert_canonical<T>(wire: &str) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let value: T = serde_json::from_str(wire)?;
    assert_eq!(
        serde_json::to_vec(&value)?,
        serde_json_canonicalizer::to_vec(&value)?,
        "{wire}",
    );
    Ok(())
}
