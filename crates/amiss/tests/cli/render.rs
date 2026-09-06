use std::fs;
use std::process::{Command, Stdio};

use amiss_wire::json::{self, Value};
use amiss_wire::report::PAYLOAD_SCHEMA;

use crate::support::{amiss, fixture};

fn check(
    repo: &str,
    base: &str,
    candidate: &str,
    profile: &str,
    format: &str,
) -> (i32, Vec<u8>, String) {
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
        format,
    ])
}

#[expect(clippy::panic, reason = "test mutation of a known report shape")]
fn member_mut<'value>(value: &'value mut Value, name: &str) -> &'value mut Value {
    let Value::Object(members) = value else {
        panic!("expected an object");
    };
    members
        .iter_mut()
        .find(|(key, _value)| key == name)
        .map_or_else(|| panic!("missing {name}"), |(_key, value)| value)
}

fn bind_digest(envelope: &mut Value) -> serde_json::Result<()> {
    let digest = amiss_wire::digest::hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(member_mut(envelope, "payload"))?,
    )
    .to_string();
    *member_mut(envelope, "payload_digest") = Value::string(digest);
    Ok(())
}

fn write_value(path: &str, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = serde_json_canonicalizer::to_vec(value)?;
    bytes.push(b'\n');
    Ok(fs::write(path, bytes)?)
}

#[test]
fn every_projection_replays_identical_bytes_and_the_recorded_verdict() {
    let fx = fixture();
    let absent = format!("{}/absent", fx.repo);
    let cases = [
        (fx.repo.as_str(), "observe", 0),
        (fx.repo.as_str(), "enforce", 1),
        (absent.as_str(), "observe", 2),
    ];

    for (repo, profile, expected_code) in cases {
        let (code, report, stderr) = check(repo, &fx.base, &fx.candidate, profile, "json");
        assert_eq!((code, stderr.as_str()), (expected_code, ""));
        let report_path = format!("{}/render-{expected_code}.json", fx.repo);
        assert!(
            fs::write(&report_path, &report).is_ok(),
            "write report fixture"
        );

        for format in ["human", "sarif", "codequality"] {
            let (origin_code, origin, origin_stderr) =
                check(repo, &fx.base, &fx.candidate, profile, format);
            let (render_code, rendered, render_stderr) =
                amiss(&["render", "--report", &report_path, "--format", format]);
            assert_eq!(
                (render_code, render_stderr.as_str()),
                (expected_code, ""),
                "render {format} for exit {expected_code}"
            );
            assert_eq!(origin_code, expected_code);
            assert_eq!(origin_stderr, "");
            assert_eq!(
                rendered, origin,
                "render {format} must be the originating projection for exit {expected_code}"
            );
        }
    }
}

#[test]
fn untrusted_reports_are_refused_before_projection() {
    let fx = fixture();
    let (_code, bytes, _stderr) = check(&fx.repo, &fx.base, &fx.candidate, "observe", "json");
    let report = json::parse(&bytes).unwrap_or(Value::Null);

    let mut digest_mismatch = report.clone();
    let payload = member_mut(&mut digest_mismatch, "payload");
    let result = member_mut(payload, "result");
    *member_mut(result, "status") = Value::string("fail");
    let mismatch_path = format!("{}/mismatch.json", fx.repo);
    write_value(&mismatch_path, &digest_mismatch).unwrap();

    let mut unsupported = report.clone();
    let payload = member_mut(&mut unsupported, "payload");
    *member_mut(payload, "compatibility") = Value::string("2");
    bind_digest(&mut unsupported).unwrap();
    let unsupported_path = format!("{}/unsupported.json", fx.repo);
    write_value(&unsupported_path, &unsupported).unwrap();

    let mut invalid_result = report;
    let payload = member_mut(&mut invalid_result, "payload");
    let result = member_mut(payload, "result");
    *member_mut(result, "status") = Value::string("fail");
    bind_digest(&mut invalid_result).unwrap();
    let result_path = format!("{}/result.json", fx.repo);
    write_value(&result_path, &invalid_result).unwrap();

    for (path, reason) in [
        (mismatch_path.as_str(), "does not match its recorded digest"),
        (unsupported_path.as_str(), "unsupported wire compatibility"),
        (result_path.as_str(), "invalid result tuple"),
    ] {
        let (code, stdout, stderr) = amiss(&["render", "--report", path, "--format", "sarif"]);
        assert_eq!(code, 2, "{path}");
        assert!(stdout.is_empty(), "{path}");
        assert!(stderr.contains(reason), "{path}: {stderr}");
    }
}

#[test]
fn a_closed_pipe_preserves_the_report_verdict() {
    let fx = fixture();
    let (code, report, _stderr) = check(&fx.repo, &fx.base, &fx.candidate, "enforce", "json");
    assert_eq!(code, 1);
    let report_path = format!("{}/blocking.json", fx.repo);
    assert!(
        fs::write(&report_path, report).is_ok(),
        "write report fixture"
    );

    for format in ["human", "sarif", "codequality", "junit"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_amiss"))
            .args(["render", "--report", &report_path, "--format", format])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn amiss render");
        drop(child.stdout.take());
        let output = child.wait_with_output().expect("collect amiss render");
        assert_eq!(output.status.code(), Some(1), "{format}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("REPORT_CONSTRUCTION_FAILED"),
            "{format}"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_oversized_report_is_refused_after_the_bound() {
    let (code, stdout, stderr) =
        amiss(&["render", "--report", "/dev/zero", "--format", "codequality"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("larger than a scanner report can be"),
        "{stderr}"
    );
}
