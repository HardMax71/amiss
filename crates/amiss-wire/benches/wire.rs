use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{Value, canonical, canonical_length, parse};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// A synthetic wire-shaped value: wide sorted-on-emit objects, escape-dense
/// strings, and nested rows, around eight megabytes canonical.
fn synthetic_value() -> Value {
    let mut rows = Vec::new();
    for index in 0..8_192_usize {
        let text = format!("row {index} \"quoted\" and\ttabbed and plain padding text");
        rows.push(Value::Object(vec![
            ("path".to_owned(), Value::String(text.repeat(8))),
            (
                "index".to_owned(),
                Value::Integer(i64::try_from(index).unwrap_or(0)),
            ),
            (
                "nested".to_owned(),
                Value::Array(vec![
                    Value::String("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
                    Value::Bool(index % 2 == 0),
                    Value::Null,
                ]),
            ),
        ]));
    }
    Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String("bench/synthetic/v1".to_owned()),
        ),
        ("rows".to_owned(), Value::Array(rows)),
    ])
}

fn wire(bencher: &mut Criterion) {
    let value = synthetic_value();
    let bytes = canonical(&value);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);

    let mut group = bencher.benchmark_group("wire");
    group.throughput(Throughput::Bytes(length));
    group.bench_function("canonical", |bench| bench.iter(|| canonical(&value)));
    group.bench_function("counting-pass", |bench| {
        bench.iter(|| canonical_length(&value));
    });
    group.bench_function("hj", |bench| {
        bench.iter(|| hj("amiss/scanner-report-payload/v1", &value));
    });
    group.bench_function("parse", |bench| bench.iter(|| parse(&bytes)));
    group.bench_function("hb", |bench| {
        bench.iter(|| hb("amiss/raw-evidence/v1", &bytes));
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = wire
}
criterion_main!(benches);
