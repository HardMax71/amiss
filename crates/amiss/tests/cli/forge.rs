use std::fs;
use std::path::Path;

use crate::support::{amiss, payload};

#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test assertion helper"
)]
fn assert_historical_request(repo: &str, report: &[u8], destination: &str) {
    let report_path = format!("{repo}/report.json");
    fs::write(&report_path, report).unwrap();
    let (code, stdout, stderr) = amiss(&[
        "external-plan",
        "--report",
        &report_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let plan: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let history = plan["payload"]["introduced"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["destination"] == destination))
        .unwrap_or_else(|| panic!("unavailable exact history enters the provider plan"));
    assert_eq!(history["scheme"], "https");
    assert_eq!(history["repository"]["host"], "ghes.example");
    assert_eq!(history["repository"]["dialect"], "github");
    assert_eq!(history["repository"]["owner"], "acme");
    assert_eq!(history["repository"]["name"], "widget");
    assert_eq!(history["repository"]["form"], "blob");
    assert_eq!(
        history["repository"]["tail"],
        "0123456789012345678901234567890123456789/docs/guide.md"
    );
}

/// A self-hosted GitHub-dialect forge, end to end: the declared host opens
/// recognition for its own URLs, github.com URLs in the same run are a
/// different site, the dialect and host land in the evaluation, and the
/// emitted bytes validate against the report schema.
#[test]
fn a_declared_forge_host_is_recognized_and_reported_end_to_end() {
    let fx = amiss_fixtures::commit_pair(
        &[("docs/guide.md", "# Guide\n")],
        &[(
            "docs/guide.md",
            "# Guide\n\n[self](https://ghes.example/acme/widget/blob/main/docs/guide.md) \
             [history](https://ghes.example/acme/widget/blob/0123456789012345678901234567890123456789/docs/guide.md) \
             and [dotcom](https://github.com/acme/widget/blob/main/docs/guide.md)\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--repository",
        "ghes.example/acme/widget",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--forge",
        "github",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "emitted bytes match the active report schema"
    );

    let payload = payload(&stdout);
    assert_eq!(payload["evaluation"]["forge"], "github");
    assert_eq!(payload["evaluation"]["repository"]["host"], "ghes.example");
    let references = &payload["summary"]["references"];
    assert_eq!(
        references["same_repository"], 2,
        "the declared host's current and historical URLs are this repository"
    );
    assert_eq!(
        references["external_out_of_scope"], 1,
        "github.com is a foreign site when the identity lives elsewhere"
    );
    assert_eq!(references["resolved"], 1);
    let history = payload["observations"]
        .as_array()
        .and_then(|observations| {
            observations.iter().find_map(|observation| {
                let scope = &observation["candidate"]["resolution"]["scope"];
                (scope["kind"] == "known-commit").then_some(&observation["candidate"])
            })
        })
        .unwrap_or_else(|| panic!("the immutable scope is reported"));
    let scope = &history["resolution"]["scope"];
    assert_eq!(
        scope["commit_oid"],
        "0123456789012345678901234567890123456789"
    );
    assert_eq!(scope["path"], "docs/guide.md");
    let historical_destination = "https://ghes.example/acme/widget/blob/0123456789012345678901234567890123456789/docs/guide.md";
    assert_eq!(history["external_destination"], historical_destination);
    assert_historical_request(&fx.repo, &stdout, historical_destination);
}

/// A self-hosted GitLab with nested groups, end to end: the explicit dialect
/// rides an unknown host, the separator form resolves, the owner echoes its
/// group path, and the emitted bytes validate against the report schema.
#[test]
fn a_nested_group_gitlab_identity_works_end_to_end() {
    let fx = amiss_fixtures::commit_pair(
        &[("docs/guide.md", "# Guide\n")],
        &[(
            "docs/guide.md",
            "# Guide\n\n[self](https://git.example.internal/group/subgroup/widget/-/blob/main/docs/guide.md) \
             and [lines](https://git.example.internal/group/subgroup/widget/-/blob/main/docs/guide.md#L2-3)\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--repository",
        "git.example.internal/group/subgroup/widget",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--forge",
        "gitlab",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "emitted bytes match the active report schema"
    );

    let payload = payload(&stdout);
    assert_eq!(payload["evaluation"]["forge"], "gitlab");
    assert_eq!(
        payload["evaluation"]["repository"]["owner"],
        "group/subgroup"
    );
    let references = &payload["summary"]["references"];
    assert_eq!(references["same_repository"], 2);
    assert_eq!(references["resolved"], 2);
    assert_eq!(references["missing"], 0);
}

/// Codeberg end to end with no flag at all: the known-host table names the
/// gitea dialect, the typed branch form resolves, a tag link is
/// version-scoped out, and the emitted bytes validate against the report
/// schema.
#[test]
fn a_codeberg_identity_defaults_to_the_gitea_dialect_end_to_end() {
    let fx = amiss_fixtures::commit_pair(
        &[("docs/guide.md", "# Guide\n")],
        &[(
            "docs/guide.md",
            "# Guide\n\n[self](https://codeberg.org/acme/widget/src/branch/main/docs/guide.md) \
             and [pinned](https://codeberg.org/acme/widget/src/tag/v1.0/docs/guide.md)\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--repository",
        "codeberg.org/acme/widget",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "emitted bytes match the active report schema"
    );

    let payload = payload(&stdout);
    assert_eq!(payload["evaluation"]["forge"], "gitea");
    let references = &payload["summary"]["references"];
    assert_eq!(
        references["same_repository"], 1,
        "the branch form is this repository; the tag form never earns the intent"
    );
    assert_eq!(references["resolved"], 1);
    assert_eq!(
        references["unsupported"], 1,
        "the tag link is an unsupported intent, version-scoped out"
    );
}
