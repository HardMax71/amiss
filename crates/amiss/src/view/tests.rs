#![cfg(test)]

use std::hint::black_box;
use std::time::Instant;

use amiss_wire::json::Value;

use super::View;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn object(members: &[(&str, Value)]) -> Value {
    Value::object(
        members
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

/// The one object shape the projection reads is the wire's raw-bytes atom.
/// Anything else is a dash, since guessing would print bytes the wire never
/// said were bytes.
#[test]
fn only_the_bytes_atom_is_read_as_bytes() {
    let hex = Value::string("646f6373");
    let row = object(&[
        ("path", object(&[("bytes_hex", hex.clone())])),
        ("target", object(&[("hex", hex)])),
        ("code", Value::string("plain")),
        ("count", Value::Integer(3)),
    ]);
    let view = View::of(&row);

    assert_eq!(view.atom_or_dash("path"), "\"docs\"");
    assert_eq!(
        view.atom_or_dash("target"),
        "-",
        "another single-member object is not the bytes atom"
    );
    assert_eq!(view.atom_or_dash("code"), "\"plain\"");
    assert_eq!(view.atom_or_dash("count"), "-");
    assert_eq!(view.atom_or_dash("absent"), "-");
}

fn measurement_report(finding_count: usize) -> Value {
    let findings = (0..finding_count)
        .map(|index| {
            super::object(vec![
                (
                    "description",
                    super::string("the referenced target is absent from the candidate tree"),
                ),
                ("effective_disposition", super::string("fail")),
                (
                    "finding_key",
                    super::string(&format!("sha256:{index:064x}")),
                ),
                ("fix", Value::Null),
                ("kind", super::string("explicit-target-missing")),
                (
                    "location",
                    super::object(vec![
                        ("path", super::string(&format!("docs/guide-{index:05}.md"))),
                        (
                            "span",
                            super::object(vec![
                                ("end_column", Value::Integer(20)),
                                ("end_line", Value::Integer(1)),
                                ("start_column", Value::Integer(1)),
                                ("start_line", Value::Integer(1)),
                            ]),
                        ),
                    ]),
                ),
            ])
        })
        .collect();
    super::object(vec![(
        "payload",
        super::object(vec![
            ("errors", Value::array(Vec::new())),
            ("findings", Value::array(findings)),
            (
                "result",
                super::object(vec![
                    ("complete", Value::Bool(true)),
                    ("exit_code", Value::Integer(1)),
                ]),
            ),
        ]),
    )])
}

fn measure<T, F: Fn() -> T>(label: &str, project: F) {
    let mut samples = [std::time::Duration::ZERO; 7];
    for elapsed in &mut samples {
        let start = Instant::now();
        let output = black_box(project());
        *elapsed = start.elapsed();
        drop(output);
    }
    samples.sort_unstable();
    let median = samples.get(3).copied().unwrap_or_default();

    let profiler = dhat::Profiler::builder().testing().build();
    let output = black_box(project());
    let stats = dhat::HeapStats::get();
    drop(profiler);
    drop(output);
    eprintln!(
        "measure {label}-10k: median {median:?}, {} allocations, {} total bytes, {} peak bytes",
        stats.total_blocks, stats.total_bytes, stats.max_bytes
    );
}

/// Measures projection construction, excluding report creation and JSON serialization.
#[test]
#[ignore = "promotion evidence, run explicitly in release"]
fn large_projection_latency_and_memory() {
    let envelope = measurement_report(10_000);
    measure("sarif", || crate::sarif::log(&envelope));
    measure("code-quality", || crate::codequality::issues(&envelope));
}
