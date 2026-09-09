use amiss_controller_github::artifact::{ArtifactRunRecord, WorkflowArtifactPage};
use amiss_wire::assessment::Nullable;

const CAPTURE: &str = include_str!("../fixtures/workflow-artifacts.json");

#[test]
fn artifact_capture_keeps_every_field_and_typed_identity() {
    let page: WorkflowArtifactPage = amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.artifacts.len(), 1);
    let artifact = &page.artifacts[0];
    assert_eq!(artifact.id, 10_126_747_789);
    assert_eq!(artifact.name, "coverage-lcov");
    let Some(Nullable::Value(run)) = &artifact.workflow_run else {
        panic!("the captured artifact has a linked run");
    };
    assert_eq!(run.id, Some(34_409_057_444));
    assert_eq!(
        run.head_sha.as_ref().unwrap().as_str(),
        "316badb35996a3ff460b1e2d4b8460f92571c438"
    );
    assert_eq!(
        run.head_branch.as_deref(),
        Some("github/typed-reference-verification")
    );
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&page).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(CAPTURE.as_bytes()).unwrap()
    );
}

#[test]
fn artifact_presence_preserves_missing_null_and_supplied_fields() {
    let mut page: WorkflowArtifactPage =
        amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    let digest = page.artifacts[0].digest;
    let run = page.artifacts[0].workflow_run.clone();
    for digest in [None, Some(Nullable::Null), digest] {
        for run in [None, Some(Nullable::Null), run.clone()] {
            let artifact = &mut page.artifacts[0];
            artifact.digest = digest;
            artifact.workflow_run = run;
            artifact.created_at = None;
            artifact.expires_at = None;
            artifact.updated_at = None;
            let encoded = serde_json::to_vec(&page).unwrap();
            assert_eq!(
                amiss_wire::read_json::<WorkflowArtifactPage>(&encoded, u64::MAX).unwrap(),
                page
            );
        }
    }
    let empty: ArtifactRunRecord = amiss_wire::read_json(b"{}", u64::MAX).unwrap();
    assert_eq!(
        empty,
        ArtifactRunRecord {
            id: None,
            repository_id: None,
            head_repository_id: None,
            head_branch: None,
            head_sha: None,
        }
    );
    assert_eq!(serde_json::to_vec(&empty).unwrap(), b"{}");
    for field in [
        "id",
        "repository_id",
        "head_repository_id",
        "head_branch",
        "head_sha",
    ] {
        let null = format!("{{\"{field}\":null}}");
        assert!(serde_json::from_str::<ArtifactRunRecord>(&null).is_err());
        assert!(amiss_wire::read_json::<ArtifactRunRecord>(null.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn artifact_metadata_is_required_and_unknown_data_is_refused() {
    for field in [
        "\"id\":10126747789,",
        "\"node_id\":\"MDg6QXJ0aWZhY3QxMDEyNjc0Nzc4OQ==\",",
        "\"name\":\"coverage-lcov\",",
        "\"size_in_bytes\":304817,",
        "\"url\":\"https://api.github.com/repos/HardMax71/amiss/actions/artifacts/10126747789\",",
        "\"archive_download_url\":\"https://api.github.com/repos/HardMax71/amiss/actions/artifacts/10126747789/zip\",",
        "\"expired\":false,",
        "\"created_at\":\"2026-09-09T21:58:03Z\",",
        "\"updated_at\":\"2026-09-09T21:58:03Z\",",
        "\"expires_at\":\"2026-12-08T21:50:03Z\",",
    ] {
        assert_eq!(CAPTURE.matches(field).count(), 1, "{field}");
        let missing = CAPTURE.replacen(field, "", 1);
        assert!(
            serde_json::from_str::<WorkflowArtifactPage>(&missing).is_err(),
            "{field}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowArtifactPage>(missing.as_bytes(), u64::MAX).is_err(),
            "{field}"
        );
    }
    for (old, new) in [
        ("\"total_count\":1", "\"extra\":true,\"total_count\":1"),
        ("\"node_id\":", "\"extra\":true,\"node_id\":"),
        ("\"repository_id\":", "\"extra\":true,\"repository_id\":"),
        ("\"node_id\":", "\"\\u006eode_id\":null,\"node_id\":"),
        (
            "\"repository_id\":",
            "\"repository_id\":0,\"repository_id\":",
        ),
        ("\"size_in_bytes\":304817", "\"size_in_bytes\":-1"),
        ("\"size_in_bytes\":304817", "\"size_in_bytes\":1.5"),
        ("\"expired\":false", "\"expired\":null"),
        (
            "\"created_at\":\"2026-09-09T21:58:03Z\"",
            "\"created_at\":[]",
        ),
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
        assert!(
            amiss_wire::read_json::<WorkflowArtifactPage>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
}

#[test]
fn artifact_objects_keep_strict_shape_numbers_and_complete_input() {
    let page: WorkflowArtifactPage = amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    let artifact = &page.artifacts[0];
    let Some(Nullable::Value(run)) = &artifact.workflow_run else {
        panic!("the captured artifact has a linked run");
    };
    let named_run = serde_json::to_string(run).unwrap();
    let positional_run = serde_json::to_string(&(
        &run.id,
        &run.repository_id,
        &run.head_repository_id,
        &run.head_branch,
        &run.head_sha,
    ))
    .unwrap();
    let named = serde_json::to_string(&page).unwrap();
    assert_eq!(named.matches(&named_run).count(), 1);
    for changed in [
        serde_json::to_string(&(&page.total_count, &page.artifacts)).unwrap(),
        named.replacen(&named_run, &positional_run, 1),
        format!("{named} {{}}"),
        format!("{named} trailing"),
        CAPTURE.replacen(
            "\"size_in_bytes\":304817",
            "\"size_in_bytes\":9007199254740992",
            1,
        ),
    ] {
        assert!(
            amiss_wire::read_json::<WorkflowArtifactPage>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
}
