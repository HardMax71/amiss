#![expect(clippy::panic, reason = "benchmark fixture setup fails loudly")]

use amiss_wire::controls::parse_organization_floor;
use amiss_wire::digest::{hb, hj};
use amiss_wire::external::{
    EVIDENCE_SCHEMA, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ExternalEvidenceSchema, PLAN_PAYLOAD_SCHEMA, ProbeMethod, assess, evidence,
};
use amiss_wire::json::{Value, canonical, canonical_length, parse};
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// A synthetic wire-shaped value: wide sorted-on-emit objects, escape-dense
/// strings, and nested rows, around eight megabytes canonical.
fn synthetic_value() -> Value {
    let mut rows = Vec::new();
    for index in 0..8_192_usize {
        let text = format!("row {index} \"quoted\" and\ttabbed and plain padding text");
        rows.push(Value::object(vec![
            ("path".to_owned(), Value::string(text.repeat(8))),
            (
                "index".to_owned(),
                Value::Integer(i64::try_from(index).unwrap_or(0)),
            ),
            (
                "nested".to_owned(),
                Value::array(vec![
                    Value::string("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
                    Value::Bool(index.is_multiple_of(2)),
                    Value::Null,
                ]),
            ),
        ]));
    }
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string("bench/synthetic".to_owned()),
        ),
        ("rows".to_owned(), Value::array(rows)),
    ])
}

#[divan::bench(sample_count = 20)]
fn canonicalize(bencher: Bencher<'_, '_>) {
    let value = synthetic_value();
    let length = canonical_length(&value);
    bencher
        .counter(BytesCount::new(length))
        .bench_local(|| canonical(black_box(&value)));
}

#[divan::bench(sample_count = 20)]
fn counting_pass(bencher: Bencher<'_, '_>) {
    let value = synthetic_value();
    let length = canonical_length(&value);
    bencher
        .counter(BytesCount::new(length))
        .bench_local(|| canonical_length(black_box(&value)));
}

#[divan::bench(sample_count = 20)]
fn digest_value(bencher: Bencher<'_, '_>) {
    let value = synthetic_value();
    let length = canonical_length(&value);
    bencher
        .counter(BytesCount::new(length))
        .bench_local(|| hj("amiss/scanner-report-payload", black_box(&value)));
}

#[divan::bench(sample_count = 20)]
fn parse_wire(bencher: Bencher<'_, '_>) {
    let bytes = canonical(&synthetic_value());
    bencher
        .counter(BytesCount::of_slice(&bytes))
        .bench_local(|| parse(black_box(&bytes)));
}

#[divan::bench(sample_count = 20)]
fn digest_bytes(bencher: Bencher<'_, '_>) {
    let bytes = canonical(&synthetic_value());
    bencher
        .counter(BytesCount::of_slice(&bytes))
        .bench_local(|| hb("amiss/raw-evidence", black_box(&bytes)));
}

#[divan::bench(sample_count = 10_000)]
fn format_digest(bencher: Bencher<'_, '_>) {
    let digest = hb("amiss/bench", b"digest formatting");
    bencher.bench_local(|| black_box(digest).to_string());
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
    let engine_digest = hj("amiss/benchmark-engine", &Value::Null);
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
    document.payload_digest = hb(PLAN_PAYLOAD_SCHEMA, &payload);
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
        parse(&evidence)
            .ok()
            .as_ref()
            .and_then(|value| value.text("schema")),
        Some(EVIDENCE_SCHEMA)
    );
    (plan, evidence)
}
