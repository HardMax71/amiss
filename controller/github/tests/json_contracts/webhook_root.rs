use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::webhook::GitHubPayload;

#[test]
fn pull_payloads_reject_conflicting_event_markers_through_serde() {
    assert!(
        serde_json::from_str::<GitHubPayload>(r#"{"unknown":{"future":[null,false]}}"#).is_ok()
    );
    for field in [
        "review",
        "comment",
        "thread",
        "check_suite",
        "check_run",
        "workflow",
        "workflow_run",
    ] {
        for value in ["null", "false", "0", "[]", "{}", r#"{"id":1}"#] {
            let input = format!(r#"{{"{field}":{value}}}"#);
            assert!(
                serde_json::from_str::<GitHubPayload>(&input).is_err(),
                "{input}"
            );
        }
    }
}

#[test]
fn webhook_root_keeps_repository_identity_without_metadata() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY;
    let record: WorkflowRepositoryRecord = serde_json::from_slice(input).unwrap();
    let encoded = serde_json::to_vec(&record).unwrap();
    assert_eq!(
        serde_json::from_slice::<WorkflowRepositoryRecord>(&encoded).unwrap(),
        record
    );
    assert_eq!(record.owner.login, "octo-org");
    assert_eq!(record.full_name, "octo-org/octo-repo");
    assert!(serde_json::from_str::<GitHubPayload>(r#"{"repository":null}"#).is_err());
    assert_eq!(
        serde_json::from_str::<GitHubPayload>("{}")
            .unwrap()
            .repository,
        None
    );
}

#[test]
fn webhook_repository_metadata_does_not_expand_the_consumed_contract() {
    let record: WorkflowRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY).unwrap();
    let encoded = serde_json::to_string(&record).unwrap();
    for metadata in [
        r#""network_count":-1,"subscribers_count":false,"organization":null"#,
        r#""template_repository":{"owner":{},"permissions":[]},"unknown":true"#,
        r#""custom_properties":{"future":{"nested":[]}},"disabled":"future""#,
    ] {
        let input = encoded.replacen('{', &format!("{{{metadata},"), 1);
        assert_eq!(
            serde_json::from_str::<WorkflowRepositoryRecord>(&input).unwrap(),
            record
        );
    }
}
