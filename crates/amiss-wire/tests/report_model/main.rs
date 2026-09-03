use amiss_wire::report::model::{
    AnalysisError, Controls, DocumentResult, Engine, Evaluation, Feedback, Finding,
    ObservationComparison, Summary,
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
}
