use std::process::Command;

const BASE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn amiss(args: &[&str]) -> (i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (output.status.code().unwrap_or(-1), output.stdout)
}

/// The one CLI outcome every platform must produce identically: a root that
/// is not an eligible primary repository is the fatal repository-unavailable
/// projection, exit class two, as one canonical envelope. On a platform
/// without the handle/no-follow boundary this is also the outcome for every
/// root, per the contract's no-fallback rule.
#[test]
fn an_ineligible_root_is_the_unavailable_projection_on_every_platform() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().to_str().unwrap().to_owned();
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        BASE_A,
        "--candidate",
        HEAD_B,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2, "fatal exit class");
    assert_eq!(stdout.last(), Some(&b'\n'), "one wire line");
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let payload = envelope.get("payload").unwrap();
    let errors = payload.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["code"], "GIT_REPOSITORY_UNAVAILABLE");
    assert_eq!(errors[0]["phase"], "git");
    assert_eq!(payload["result"]["exit_code"], 2);
    assert_eq!(payload["result"]["complete"], false);
}
