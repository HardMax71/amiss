use std::path::Path;
use std::process::Command;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn fixture() -> amiss_fixtures::CommitPair {
    amiss_fixtures::commit_pair(
        &[
            ("README", "See [the guide](docs/guide.md).\n"),
            ("docs/guide.md", "# Guide\n\n[home](../README)\n"),
        ],
        &[(
            "docs/guide.md",
            "# Guide\n\n[home](../README) and [gone](missing.md)\n",
        )],
    )
    .unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn claim_fixture() -> amiss_fixtures::CommitPair {
    amiss_fixtures::commit_pair(
        &[
            ("subject.txt", "alpha\n"),
            (
                "docs/claims.md",
                "The subject holds [alpha][amiss:subject-line].\n\n\
                 [amiss:subject-line]: <amiss:value?path=subject.txt&line=L1> \"alpha\"\n",
            ),
        ],
        &[("subject.txt", "beta\n")],
    )
    .unwrap()
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
pub(crate) fn amiss(args: &[&str]) -> (i32, Vec<u8>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (
        output.status.code().unwrap_or(-1),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[expect(clippy::unwrap_used, reason = "differential test against the binary")]
pub(crate) fn payload(stdout: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    envelope.get("payload").cloned().unwrap()
}
