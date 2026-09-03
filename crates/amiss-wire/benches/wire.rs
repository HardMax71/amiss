#![expect(clippy::panic, reason = "benchmark fixture setup fails loudly")]

use amiss_wire::controls::parse_organization_floor;
use amiss_wire::digest::{hb, hj};
use amiss_wire::external::{
    EVIDENCE_SCHEMA, PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA, assess, evidence_file,
    probe_evidence_row,
};
use amiss_wire::json::{Value, canonical, canonical_length, parse};
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

const SAMPLE_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

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
    let validation = assess(&plan, &evidence, "0.0.0", SAMPLE_DIGEST)
        .unwrap_or_else(|defect| panic!("dense assessment fixture: {defect:?}"));
    let verdict_count = validation
        .member("payload")
        .and_then(|payload| payload.member("verdicts"))
        .and_then(|verdicts| match verdicts {
            Value::Array(rows) => Some(rows.len()),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::String(_)
            | Value::Object(_) => None,
        });
    assert_eq!(verdict_count, Some(16_384));

    let bytes = canonical_length(&plan).saturating_add(canonical_length(&evidence));
    bencher.counter(BytesCount::new(bytes)).bench_local(|| {
        assess(
            black_box(&plan),
            black_box(&evidence),
            black_box("0.0.0"),
            black_box(SAMPLE_DIGEST),
        )
    });
}

fn assessment_fixture(count: usize) -> (Value, Value) {
    let destinations: Vec<String> = (0..count)
        .map(|index| format!("https://example.com/resource-{index:05}"))
        .collect();
    let introduced = destinations
        .iter()
        .map(|destination| {
            Value::object(vec![
                ("destination".to_owned(), Value::string(destination.clone())),
                (
                    "documents".to_owned(),
                    Value::array(vec![Value::string("docs/bench.md".to_owned())]),
                ),
                ("scheme".to_owned(), Value::string("https".to_owned())),
            ])
        })
        .collect();
    let payload = Value::object(vec![
        ("introduced".to_owned(), Value::array(introduced)),
        (
            "report".to_owned(),
            Value::object(vec![(
                "payload_digest".to_owned(),
                Value::string(SAMPLE_DIGEST.to_owned()),
            )]),
        ),
        (
            "schema".to_owned(),
            Value::string(PLAN_PAYLOAD_SCHEMA.to_owned()),
        ),
    ]);
    let payload_digest = hj(PLAN_PAYLOAD_SCHEMA, &payload).to_string();
    let plan = Value::object(vec![
        ("payload".to_owned(), payload),
        ("payload_digest".to_owned(), Value::string(payload_digest)),
        (
            "schema".to_owned(),
            Value::string(PLAN_ENVELOPE_SCHEMA.to_owned()),
        ),
    ]);
    let rows = destinations
        .iter()
        .rev()
        .map(|destination| {
            probe_evidence_row(destination, "get", Some(200), None, None, "bench-instant")
        })
        .collect();
    let evidence = evidence_file(&plan, "benchmark", "0.0.0", rows)
        .unwrap_or_else(|| panic!("benchmark plan has no payload digest"));
    assert_eq!(evidence.text("schema"), Some(EVIDENCE_SCHEMA));
    (plan, evidence)
}
