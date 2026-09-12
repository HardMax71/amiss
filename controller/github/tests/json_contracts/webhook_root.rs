use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::repository::template::{TemplateOwner, TemplateRepository};
use amiss_controller_github::webhook::GitHubPayload;

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

#[test]
fn webhook_template_keeps_its_complete_distinct_optional_contract() {
    let input = include_bytes!("../fixtures/webhook-template-repository.json");
    let template: TemplateRepository = serde_json::from_slice(input).unwrap();
    let encoded = serde_json::to_vec(&template).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(input).unwrap(),
        amiss_fixtures::canonical_json(&encoded).unwrap()
    );
    assert_eq!(
        amiss_wire::read_json::<TemplateRepository>(input, u64::MAX).unwrap(),
        template
    );
    assert_eq!(
        serde_json::to_string(&TemplateRepository::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&TemplateOwner::default()).unwrap(),
        "{}"
    );
    for input in [
        r#"{"id":9007199254740992}"#,
        r#"{"id":1.0}"#,
        r#"{"id":-0}"#,
        r#"{"owner":{"id":9007199254740992}}"#,
        r#"{"owner":{"id":1e0}}"#,
        r#"{"owner":{"site_admin":null}}"#,
        r#"{"owner":{"id":1,"\u0069d":2}}"#,
        r#"{"topics":null}"#,
        r#"{"topics":[null]}"#,
        r#"{"merge_commit_title":{"PR_TITLE":null}}"#,
        r#"{"merge_commit_message":"other"}"#,
        r#"{"squash_merge_commit_title":true}"#,
        r#"{"squash_merge_commit_message":"PR_TITLE"}"#,
    ] {
        assert!(
            serde_json::from_str::<TemplateRepository>(input).is_err(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<TemplateRepository>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
    for input in ["[]", r#"{"owner":[]}"#, r#"{"permissions":[]}"#, "{} null"] {
        assert!(
            amiss_wire::read_json::<TemplateRepository>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}
