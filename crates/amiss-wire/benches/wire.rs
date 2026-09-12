#![expect(clippy::panic, reason = "benchmark fixture setup fails loudly")]

use amiss_wire::controls::parse_organization_floor;
use amiss_wire::external::{
    EVIDENCE_SCHEMA, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ExternalEvidenceSchema, PLAN_PAYLOAD_SCHEMA, ProbeMethod, assess, evidence,
};
use divan::counter::BytesCount;
use divan::{Bencher, black_box};
use serde_json::Value;
use sha2::Digest as _;

fn main() {
    divan::main();
}

#[divan::bench(sample_count = 1_000)]
fn decode_organization_floor(bencher: Bencher<'_, '_>) {
    const FLOOR: &[u8] = include_bytes!("../tests/fixtures/organization-floor.json");
    bencher
        .counter(BytesCount::of_slice(FLOOR))
        .bench_local(|| parse_organization_floor(black_box(FLOOR)));
}

#[divan::bench(sample_count = 3, sample_size = 1)]
fn dense_external_assessment(bencher: Bencher<'_, '_>) {
    let (plan, evidence) = assessment_fixture(16_384);
    let engine_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/benchmark-engine")
            .chain_update([0_u8])
            .chain_update(b"null")
            .finalize()
            .0,
    );
    let validation = assess(&plan, &evidence, "0.0.0", engine_digest)
        .unwrap_or_else(|defect| panic!("dense assessment fixture: {defect:?}"));
    let document = amiss_wire::external::parse_assessment(&validation)
        .unwrap_or_else(|defect| panic!("dense assessment output: {defect}"));
    assert_eq!(document.payload.verdicts.len(), 16_384);

    let bytes = plan.len().saturating_add(evidence.len());
    bencher.counter(BytesCount::new(bytes)).bench_local(|| {
        assess(
            black_box(&plan),
            black_box(&evidence),
            black_box("0.0.0"),
            black_box(engine_digest),
        )
    });
}

fn assessment_fixture(count: usize) -> (Vec<u8>, Vec<u8>) {
    let destinations: Vec<String> = (0..count)
        .map(|index| format!("https://example.com/resource-{index:05}"))
        .collect();
    let mut document = amiss_wire::external::parse_plan(include_bytes!(
        "../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap_or_else(|defect| panic!("benchmark plan example: {defect}"));
    document.payload.introduced = destinations
        .iter()
        .map(|destination| amiss_wire::external::ExternalDestination {
            destination: destination.clone(),
            documents: vec!["docs/bench.md".to_owned()],
            scheme: "https".to_owned(),
            repository: None,
        })
        .collect();
    document.payload.removed.clear();
    document.payload.retained_count = 0;
    let payload = serde_json_canonicalizer::to_vec(&document.payload)
        .unwrap_or_else(|defect| panic!("benchmark payload: {defect}"));
    document.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize()
            .0,
    );
    let plan = serde_json_canonicalizer::to_vec(&document)
        .unwrap_or_else(|defect| panic!("benchmark plan: {defect}"));
    let rows = destinations
        .iter()
        .rev()
        .map(|destination| ExternalEvidenceRow::HttpProbe {
            destination: destination.clone(),
            method: ProbeMethod::Get,
            status: Some(200),
            failure: None,
            final_destination: None,
            redirect_chain_permanent: None,
            checked_at: "bench-instant".to_owned(),
        })
        .collect();
    let evidence = evidence(&ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: document.payload_digest,
        producer: ExternalEvidenceProducer {
            name: "benchmark".to_owned(),
            version: "0.0.0".to_owned(),
        },
        rows,
    })
    .unwrap_or_else(|defect| panic!("benchmark evidence is malformed: {defect}"));
    assert_eq!(
        serde_json::from_slice::<Value>(&evidence)
            .ok()
            .as_ref()
            .and_then(|value| value.get("schema").and_then(Value::as_str)),
        Some(EVIDENCE_SCHEMA)
    );
    (plan, evidence)
}
