#![expect(clippy::unwrap_used, reason = "integration process fixture")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use amiss_bootstrap::result::{BootstrapResult, parse_result};

fn invoke(root: &Path, constraint: &Path, report: &Path, result: &Path) -> Output {
    let unused = root.join("unused");
    Command::new(env!("CARGO_BIN_EXE_amiss-bootstrap"))
        .arg("exec")
        .arg("--action-repository")
        .arg(&unused)
        .arg("--repository")
        .arg(&unused)
        .arg("--constraint")
        .arg(constraint)
        .arg("--evaluation-request")
        .arg(&unused)
        .arg("--snapshot-request")
        .arg(&unused)
        .arg("--controls-request")
        .arg(&unused)
        .arg("--scratch")
        .arg(root)
        .arg("--report")
        .arg(report)
        .arg("--result")
        .arg(result)
        .output()
        .unwrap()
}

fn output_files(root: &Path) -> (PathBuf, PathBuf) {
    let report = root.join("report");
    let result = root.join("result");
    std::fs::write(&report, b"").unwrap();
    std::fs::write(&result, b"").unwrap();
    (report, result)
}

#[test]
fn a_missing_constraint_records_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let (report, result) = output_files(root.path());

    let output = invoke(root.path(), &root.path().join("missing"), &report, &result);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(std::fs::read(report).unwrap().is_empty());
    assert_eq!(
        parse_result(&std::fs::read(result).unwrap()),
        Some(BootstrapResult::Unavailable)
    );
}

#[test]
fn a_malformed_constraint_records_tampered_runtime() {
    let root = tempfile::tempdir().unwrap();
    let constraint = root.path().join("constraint");
    let (report, result) = output_files(root.path());
    std::fs::write(&constraint, b"not a constraint").unwrap();

    let output = invoke(root.path(), &constraint, &report, &result);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(std::fs::read(report).unwrap().is_empty());
    assert_eq!(
        parse_result(&std::fs::read(result).unwrap()),
        Some(BootstrapResult::TamperedRuntime)
    );
}

#[test]
fn an_existing_result_is_never_replaced() {
    existing_output_is_never_replaced(b"", b"controller-owned");
}

#[test]
fn an_existing_report_is_never_replaced() {
    existing_output_is_never_replaced(b"controller-owned", b"");
}

fn existing_output_is_never_replaced(report_bytes: &[u8], result_bytes: &[u8]) {
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("report");
    let result = root.path().join("result");
    std::fs::write(&report, report_bytes).unwrap();
    std::fs::write(&result, result_bytes).unwrap();

    let output = invoke(root.path(), &root.path().join("missing"), &report, &result);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(report).unwrap(), report_bytes);
    assert_eq!(std::fs::read(result).unwrap(), result_bytes);
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-invocation"));
}

#[test]
fn output_files_have_fixed_names() {
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("first");
    let result = root.path().join("second");
    std::fs::write(&report, b"").unwrap();
    std::fs::write(&result, b"").unwrap();

    let output = invoke(root.path(), &root.path().join("missing"), &report, &result);

    assert_eq!(output.status.code(), Some(2));
    assert!(std::fs::read(report).unwrap().is_empty());
    assert!(std::fs::read(result).unwrap().is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-invocation"));
}

#[test]
fn output_files_must_be_created_by_the_controller() {
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("report");
    let result = root.path().join("result");

    let output = invoke(root.path(), &root.path().join("missing"), &report, &result);

    assert_eq!(output.status.code(), Some(2));
    assert!(!report.exists());
    assert!(!result.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-invocation"));
}
