use std::fs;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::support::{amiss, fixture};

#[derive(Default)]
struct Summary {
    tests: usize,
    failures: usize,
    errors: usize,
    cases: usize,
    failure_rows: usize,
    error_rows: usize,
    notes: usize,
}

fn summary(xml: &[u8]) -> Result<Summary, String> {
    let mut parsed = Summary::default();
    let mut reader = Reader::from_reader(xml);
    loop {
        let event = reader.read_event().map_err(|defect| defect.to_string())?;
        let start = match &event {
            Event::Start(start) | Event::Empty(start) => Some(start),
            Event::Decl(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::DocType(_)
            | Event::PI(_)
            | Event::GeneralRef(_) => None,
            Event::Eof => break,
        };
        let Some(start) = start else {
            continue;
        };
        match start.name().as_ref() {
            b"testsuite" => {
                for attribute in start.attributes() {
                    let attribute = attribute.map_err(|defect| defect.to_string())?;
                    let count = || {
                        std::str::from_utf8(attribute.value.as_ref())
                            .map_err(|defect| defect.to_string())?
                            .parse::<usize>()
                            .map_err(|defect| defect.to_string())
                    };
                    match attribute.key.as_ref() {
                        b"tests" => parsed.tests = count()?,
                        b"failures" => parsed.failures = count()?,
                        b"errors" => parsed.errors = count()?,
                        b"name" | b"time" | b"skipped" => {}
                        _ => return Err("unexpected suite attribute".to_owned()),
                    }
                }
            }
            b"testcase" => parsed.cases = parsed.cases.saturating_add(1),
            b"failure" => parsed.failure_rows = parsed.failure_rows.saturating_add(1),
            b"error" => parsed.error_rows = parsed.error_rows.saturating_add(1),
            b"system-out" => parsed.notes = parsed.notes.saturating_add(1),
            b"testsuites" => {}
            _ => return Err("unexpected JUnit element".to_owned()),
        }
    }
    Ok(parsed)
}

fn check(repo: &str, base: &str, candidate: &str, profile: &str) -> (i32, Vec<u8>, String) {
    amiss(&[
        "check",
        "--repo",
        repo,
        "--object-format",
        "sha1",
        "--base",
        base,
        "--candidate",
        candidate,
        "--profile",
        profile,
        "--format",
        "json",
    ])
}

#[test]
fn junit_replays_report_rows_and_the_recorded_verdict_deterministically() {
    let fx = fixture();
    let absent = format!("{}/absent", fx.repo);
    let cases = [
        (fx.repo.as_str(), "observe", 0),
        (fx.repo.as_str(), "enforce", 1),
        (absent.as_str(), "observe", 2),
    ];

    for (repo, profile, expected_code) in cases {
        let (code, report, stderr) = check(repo, &fx.base, &fx.candidate, profile);
        assert_eq!((code, stderr.as_str()), (expected_code, ""));
        let report_path = format!("{}/junit-{expected_code}.json", fx.repo);
        fs::write(&report_path, &report).expect("write report");

        let args = ["render", "--report", &report_path, "--format", "junit"];
        let (render_code, first, render_stderr) = amiss(&args);
        let (again_code, second, again_stderr) = amiss(&args);
        assert_eq!((render_code, render_stderr.as_str()), (expected_code, ""));
        assert_eq!((again_code, again_stderr.as_str()), (expected_code, ""));
        assert_eq!(
            first, second,
            "identical reports yield identical JUnit bytes"
        );

        let envelope: serde_json::Value = serde_json::from_slice(&report).expect("valid report");
        let payload = envelope.get("payload").expect("report payload");
        let findings = payload["findings"].as_array().expect("finding rows");
        let errors = payload["errors"].as_array().expect("error rows");
        let expected_failures = findings
            .iter()
            .filter(|row| row["effective_disposition"] == "fail")
            .count();
        let parsed = summary(&first).expect("valid JUnit projection");
        assert_eq!(
            parsed.tests,
            findings.len().saturating_add(errors.len()).max(1)
        );
        assert_eq!(parsed.cases, parsed.tests);
        assert_eq!(parsed.failures, expected_failures);
        assert_eq!(parsed.failure_rows, expected_failures);
        assert_eq!(parsed.errors, errors.len());
        assert_eq!(parsed.error_rows, errors.len());
        assert_eq!(
            parsed.notes,
            findings.len().saturating_sub(expected_failures)
        );
        for finding in findings {
            let key = finding["finding_key"].as_str().expect("finding key");
            assert!(
                first
                    .windows(key.len())
                    .any(|window| window == key.as_bytes())
            );
        }
    }
}

#[test]
fn junit_is_a_valid_render_format_but_not_a_scan_format() {
    let (code, stdout, stderr) = amiss(&["check", "--format", "junit"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("INVALID_INVOCATION"));
}
