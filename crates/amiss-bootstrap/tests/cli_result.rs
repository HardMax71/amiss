#![expect(clippy::unwrap_used, reason = "integration process fixture")]

use std::path::Path;
use std::process::{Command, Output};

use amiss_bootstrap::result::{BootstrapResult, parse_result};

fn invoke(root: &Path, constraint: &Path, result: &Path) -> Output {
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
        .arg("--result")
        .arg(result)
        .output()
        .unwrap()
}

#[test]
fn a_missing_constraint_records_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let result = root.path().join("result");

    let output = invoke(root.path(), &root.path().join("missing"), &result);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        parse_result(&std::fs::read(result).unwrap()),
        Some(BootstrapResult::Unavailable)
    );
}

#[test]
fn a_malformed_constraint_records_tampered_runtime() {
    let root = tempfile::tempdir().unwrap();
    let constraint = root.path().join("constraint");
    let result = root.path().join("result");
    std::fs::write(&constraint, b"not a constraint").unwrap();

    let output = invoke(root.path(), &constraint, &result);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        parse_result(&std::fs::read(result).unwrap()),
        Some(BootstrapResult::TamperedRuntime)
    );
}

#[test]
fn an_existing_result_is_never_replaced() {
    let root = tempfile::tempdir().unwrap();
    let result = root.path().join("result");
    std::fs::write(&result, b"controller-owned").unwrap();

    let output = invoke(root.path(), &root.path().join("missing"), &result);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(std::fs::read(result).unwrap(), b"controller-owned");
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-invocation"));
}
