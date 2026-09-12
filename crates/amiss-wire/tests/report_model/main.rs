mod producer_facts;
mod producer_paths;
mod projection;

use amiss_wire::report::model::{
    BaseSnapshot, Evaluation, FindingFactEvidence, FindingKeyScope, MissingResolution, RepoPath,
    ReportEnvelope, Resolution, Snapshot,
};
use amiss_wire::requests::CandidateSnapshot;
use amiss_wire::resolution::{Target, VersionScope};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn entire_report_streams_in_canonical_order() {
    let envelope: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let mut output = Vec::new();
    amiss_wire::report::emit_report(&envelope, &mut output).unwrap();
    assert_eq!(output, REPORT);
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
        assert_canonical::<Target<RepoPath>>(&template.replace("$digest", DIGEST))?;
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
        assert_canonical::<VersionScope<RepoPath>>(&template.replace("$oid", OID))?;
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
        let wire = template.replace("$digest", DIGEST);
        let evidence: FindingFactEvidence = serde_json::from_str(&wire)?;
        assert_eq!(
            serde_json_canonicalizer::to_vec(&evidence)?,
            wire.as_bytes()
        );
        assert!(serde_json::to_string(&evidence)?.starts_with(&format!(
            "{{\"kind\":{},",
            serde_json::to_string(&evidence.to_string())?
        )));
        assert!(
            serde_json::from_str::<FindingFactEvidence>(&wire.replacen(
                '{',
                "{\"unexpected\":true,",
                1
            ))
            .is_err()
        );
    }
    Ok(())
}

fn assert_canonical<T>(wire: &str) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let value: T = serde_json::from_str(wire)?;
    assert_eq!(
        serde_json_canonicalizer::to_vec(&value)?,
        wire.as_bytes(),
        "{wire}",
    );
    Ok(())
}
