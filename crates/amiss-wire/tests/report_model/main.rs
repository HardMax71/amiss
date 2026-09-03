use amiss_wire::report::model::{Controls, Engine, Evaluation, Summary};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn published_provenance_blocks_match_the_models() {
    let document: serde_json::Value = serde_json::from_slice(REPORT).unwrap();
    let payload = document.get("payload").unwrap();
    let _: Controls = serde_json::from_value(payload.get("controls").unwrap().clone()).unwrap();
    let _: Engine = serde_json::from_value(payload.get("engine").unwrap().clone()).unwrap();
    let _: Evaluation = serde_json::from_value(payload.get("evaluation").unwrap().clone()).unwrap();
    let _: Summary = serde_json::from_value(payload.get("summary").unwrap().clone()).unwrap();
}
