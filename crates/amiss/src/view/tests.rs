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

fn measurement_report(finding_count: usize) -> amiss_wire::report::model::ReportPayload {
    use amiss_wire::model::RepoPathText;
    use amiss_wire::report::model::{RepoPath, ReportEnvelope};

    let report: ReportEnvelope = serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let mut payload = report.payload;
    let template = payload.findings[0].clone();
    payload.findings = (0..finding_count)
        .map(|index| {
            let mut finding = template.clone();
            finding.description =
                "the referenced target is absent from the candidate tree".to_owned();
            finding.effective_disposition = amiss_wire::report::Disposition::Fail;
            finding.finding_key = format!("sha256:{index:064x}").parse().unwrap();
            finding.fix = None;
            finding.kind = amiss_wire::report::FindingKind::ExplicitTargetMissing;
            finding.location.path = Some(RepoPath::Text(
                RepoPathText::new(format!("docs/guide-{index:05}.md")).unwrap(),
            ));
            finding.location.span = Some(amiss_wire::report::model::SourceSpan {
                end_byte: 19,
                start_byte: 0,
                end_column: 20,
                end_line: 1,
                start_column: 1,
                start_line: 1,
            });
            finding
        })
        .collect();
    payload.errors.clear();
    payload.result.complete = true;
    payload.result.exit_code = 1;
    payload
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
    measure("sarif", || {
        crate::sarif::log(&envelope, |path| match path {
            amiss_wire::report::model::RepoPath::Text(text) => Some(text.as_str()),
            amiss_wire::report::model::RepoPath::Bytes(_) => None,
        })
    });
    measure("code-quality", || {
        crate::codequality::issues(&envelope, |path| {
            std::borrow::Cow::Borrowed(match path {
                amiss_wire::report::model::RepoPath::Text(text) => text.as_str(),
                amiss_wire::report::model::RepoPath::Bytes(bytes) => &bytes.bytes_hex,
            })
        })
    });
}
