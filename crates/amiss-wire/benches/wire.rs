use amiss_wire::digest::{hb, hj};
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
                    Value::Bool(index.is_multiple_of(2)),
                    Value::Null,
                ]),
            ),
        ]));
    }
    Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String("bench/synthetic".to_owned()),
        ),
        ("rows".to_owned(), Value::Array(rows)),
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
