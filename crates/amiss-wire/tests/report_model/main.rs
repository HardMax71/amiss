use amiss_wire::report::model::{
    AnalysisError, Controls, DocumentResult, Engine, Evaluation, Feedback, Finding,
    MissingResolution, ObservationComparison, ReportEnvelope, Resolution, ResolutionTarget,
    Summary, VersionScope,
};

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
fn report_without_finding_rows_streams_in_canonical_order() {
    let mut envelope: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    envelope.payload.findings.clear();
    assert_eq!(
        serde_json::to_vec(&envelope).unwrap(),
        serde_json_canonicalizer::to_vec(&envelope).unwrap(),
    );
}

#[test]
fn every_observation_variant_streams_in_canonical_order() -> Result<(), Box<dyn std::error::Error>>
{
    const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    const OID: &str = "0000000000000000000000000000000000000000";

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
