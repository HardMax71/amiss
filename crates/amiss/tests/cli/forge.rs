use std::fs;
use std::path::Path;

use crate::support::{amiss, payload};

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
        references["same_repository"], 1,
        "the declared host's URL is this repository"
    );
    assert_eq!(
        references["external_out_of_scope"], 1,
        "github.com is a foreign site when the identity lives elsewhere"
    );
    assert_eq!(references["resolved"], 1);
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
