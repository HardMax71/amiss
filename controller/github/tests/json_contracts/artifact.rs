use std::io::Cursor;

use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::artifact::{ArtifactRunRecord, WorkflowArtifactPage};

const CAPTURE: &str = include_str!("../fixtures/workflow-artifacts.json");

#[test]
fn artifact_inputs_keep_consumed_facts_and_ignore_provider_metadata() {
    let page: WorkflowArtifactPage = serde_json::from_str(CAPTURE).unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.artifacts.len(), 1);
    let artifact = &page.artifacts[0];
    assert_eq!(artifact.id, 10_126_747_789);
    assert_eq!(artifact.name, "coverage-lcov");
    assert_eq!(artifact.workflow_run.id, 34_409_057_444);
    assert_eq!(
        artifact.workflow_run.head_sha.as_str(),
        "316badb35996a3ff460b1e2d4b8460f92571c438"
    );
    let minimal = serde_json::to_vec(&page).unwrap();
    assert!(minimal.len() < CAPTURE.len());
    assert_eq!(
        serde_json::from_slice::<WorkflowArtifactPage>(&minimal).unwrap(),
        page
    );
    for (old, new) in [
        (
            "\"total_count\":",
            "\"extra\":{\"future\":[null,true]},\"total_count\":",
        ),
        (
            "\"node_id\":",
            "\"extra\":{\"future\":[null,true]},\"node_id\":",
        ),
        (
            "\"repository_id\":",
            "\"extra\":{\"future\":[null,true]},\"repository_id\":",
        ),
        (
            "\"created_at\":\"2026-09-09T21:58:03Z\"",
            "\"created_at\":[]",
        ),
    ] {
        assert_eq!(CAPTURE.matches(old).count(), 1, "{old}");
        let changed = CAPTURE.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<WorkflowArtifactPage>(&changed).unwrap(),
            page,
            "{new}"
        );
    }
}

#[test]
fn artifact_consumed_fields_are_required() {
    let page: WorkflowArtifactPage = serde_json::from_str(CAPTURE).unwrap();
    for field in [
        "total_count",
        "artifacts",
        "id",
        "name",
        "size_in_bytes",
        "expired",
        "digest",
        "workflow_run",
    ] {
        let missing = CAPTURE.replacen(&format!("\"{field}\":"), "\"unused\":", 1);
        assert!(
            serde_json::from_str::<WorkflowArtifactPage>(&missing).is_err(),
            "{field}"
        );
    }
    let linked = serde_json::to_string(&page.artifacts[0].workflow_run).unwrap();
    for field in ["id", "repository_id", "head_repository_id", "head_sha"] {
        let missing = linked.replacen(&format!("\"{field}\":"), "\"unused\":", 1);
        assert!(
            serde_json::from_str::<ArtifactRunRecord>(&missing).is_err(),
            "{field}"
        );
    }
}

#[test]
fn artifact_consumed_fields_reject_invalid_values_and_duplicates() {
    let page: WorkflowArtifactPage = serde_json::from_str(CAPTURE).unwrap();
    let digest = serde_json::to_string(&page.artifacts[0].digest).unwrap();
    let linked = serde_json::to_string(&page.artifacts[0].workflow_run).unwrap();
    let minimal = serde_json::to_string(&page).unwrap();
    for changed in [
        minimal.replace(&format!("\"digest\":{digest}"), "\"digest\":null"),
        minimal.replace(
            &format!("\"workflow_run\":{linked}"),
            "\"workflow_run\":null",
        ),
    ] {
        assert!(serde_json::from_str::<WorkflowArtifactPage>(&changed).is_err());
    }
    for (old, new) in [
        ("\"id\":10126747789", "\"id\":null"),
        ("\"id\":34409057444", "\"id\":null"),
        ("\"name\":\"coverage-lcov\"", "\"name\":false"),
        (
            "\"repository_id\":",
            "\"repository_id\":0,\"repository_id\":",
        ),
        (
            "\"total_count\":",
            "\"\\u0074otal_count\":0,\"total_count\":",
        ),
        ("\"size_in_bytes\":304817", "\"size_in_bytes\":-1"),
        ("\"size_in_bytes\":304817", "\"size_in_bytes\":1.5"),
        (
            "\"size_in_bytes\":304817",
            "\"size_in_bytes\":18446744073709551616",
        ),
        ("\"expired\":false", "\"expired\":null"),
        (
            "sha256:e3e923ec8685c807056777eaad765b287cd8bb03d09482a2ddd4b7f2c81fd713",
            "sha256:bad",
        ),
        ("316badb35996a3ff460b1e2d4b8460f92571c438", "not-an-oid"),
    ] {
        assert_eq!(CAPTURE.matches(old).count(), 1, "{old}");
        let changed = CAPTURE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowArtifactPage>(&changed).is_err(),
            "{new}"
        );
    }
}

#[test]
fn artifact_native_decoding_retains_transport_limits_and_complete_input() {
    let page: WorkflowArtifactPage = serde_json::from_str(CAPTURE).unwrap();
    let positional = serde_json::to_vec(&(&page.total_count, &page.artifacts)).unwrap();
    for bytes in [CAPTURE.as_bytes(), positional.as_slice()] {
        let (decoded, consumed) =
            decode_bounded_json(Cursor::new(bytes), None, bytes.len(), |bytes| {
                serde_json::from_slice::<WorkflowArtifactPage>(bytes)
            })
            .unwrap();
        assert_eq!(decoded, page);
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decode_bounded_json(Cursor::new(bytes), None, bytes.len() - 1, |bytes| {
                serde_json::from_slice::<WorkflowArtifactPage>(bytes)
            }),
            Err(ProviderError::InvalidResponse)
        );
    }
    for suffix in [" {}", " trailing"] {
        let trailing = format!("{CAPTURE}{suffix}");
        assert!(serde_json::from_str::<WorkflowArtifactPage>(&trailing).is_err());
    }
}
