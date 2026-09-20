use std::path::Path;
use std::process::Command;

use amiss_wire::envelope::Payload as _;
use amiss_wire::report::model::{ReportEnvelope, ReportPayload};

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
    let claim = "The subject holds [alpha][amiss:subject-line].\n\n\
                 [amiss:subject-line]: <amiss:value?path=subject.txt&line=L1> \"alpha\"\n";
    let base = [("subject.txt", "alpha\n"), ("docs/claims.md", claim)];
    amiss_fixtures::commit_pair(&base, &[("subject.txt", "beta\n")]).unwrap()
}

/// A candidate whose new document declines three references for two reasons:
/// two destinations naming a site route, and one query the grammar leaves
/// unevaluated on a path the tree does hold.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn declined_fixture() -> amiss_fixtures::CommitPair {
    let routes = "[a](/docs/guide.md) [b](/README) [c](guide.md?plain=1) [d](//example.com/page)\n";
    let base = [("README", "start\n"), ("docs/guide.md", "# Guide\n")];
    amiss_fixtures::commit_pair(&base, &[("docs/routes.md", routes)]).unwrap()
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

/// The typed report the binary wrote, read back through the production reader.
#[expect(clippy::unwrap_used, reason = "differential test against the binary")]
pub(crate) fn report(stdout: &[u8]) -> ReportEnvelope {
    <ReportPayload>::parse(stdout.strip_suffix(b"\n").unwrap_or(stdout)).unwrap()
}
