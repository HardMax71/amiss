use amiss_controller_github::webhook::repository::{WorkflowOwner, WorkflowRepository};
use amiss_wire::assessment::Nullable;

const REPOSITORY: &[u8] = include_bytes!("../fixtures/webhook-workflow-repository.json");

#[test]
fn workflow_webhook_repository_keeps_the_repository_and_nullable_owner() {
    let record: WorkflowRepository = serde_json::from_slice(REPOSITORY).unwrap();
    let encoded = serde_json::to_vec(&record).unwrap();
    assert_eq!(
        serde_json::from_slice::<WorkflowRepository>(&encoded).unwrap(),
        record
    );
    assert_eq!(record.id, 300_029_405);
    assert_eq!(record.full_name, "octo-org/octo-repo");
    assert_eq!(record.owner.as_ref().unwrap().login, "octo-org");
    assert!(
        amiss_controller::decode_bounded_json::<WorkflowRepository, _>(
            REPOSITORY,
            None,
            REPOSITORY.len() - 1,
            |bytes| serde_json::from_slice(bytes),
        )
        .is_err()
    );
    let mut nullable = record;
    nullable.owner = None;
    nullable.description = Nullable::Null;
    let encoded = serde_json::to_vec(&nullable).unwrap();
    assert_eq!(
        amiss_wire::read_json::<WorkflowRepository>(&encoded, u64::MAX).unwrap(),
        nullable
    );
}

#[test]
fn workflow_repository_members_are_required_even_when_nullable() {
    let record: WorkflowRepository = serde_json::from_slice(REPOSITORY).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    for (name, value) in [
        ("node_id", &record.node_id),
        ("name", &record.name),
        ("full_name", &record.full_name),
        ("html_url", &record.html_url),
        ("url", &record.url),
        ("archive_url", &record.archive_url),
        ("assignees_url", &record.assignees_url),
        ("blobs_url", &record.blobs_url),
        ("branches_url", &record.branches_url),
        ("collaborators_url", &record.collaborators_url),
        ("comments_url", &record.comments_url),
        ("commits_url", &record.commits_url),
        ("compare_url", &record.compare_url),
        ("contents_url", &record.contents_url),
        ("contributors_url", &record.contributors_url),
        ("deployments_url", &record.deployments_url),
        ("downloads_url", &record.downloads_url),
        ("events_url", &record.events_url),
        ("forks_url", &record.forks_url),
        ("git_commits_url", &record.git_commits_url),
        ("git_refs_url", &record.git_refs_url),
        ("git_tags_url", &record.git_tags_url),
        ("hooks_url", &record.hooks_url),
        ("issue_comment_url", &record.issue_comment_url),
        ("issue_events_url", &record.issue_events_url),
        ("issues_url", &record.issues_url),
        ("keys_url", &record.keys_url),
        ("labels_url", &record.labels_url),
        ("languages_url", &record.languages_url),
        ("merges_url", &record.merges_url),
        ("milestones_url", &record.milestones_url),
        ("notifications_url", &record.notifications_url),
        ("pulls_url", &record.pulls_url),
        ("releases_url", &record.releases_url),
        ("stargazers_url", &record.stargazers_url),
        ("statuses_url", &record.statuses_url),
        ("subscribers_url", &record.subscribers_url),
        ("subscription_url", &record.subscription_url),
        ("tags_url", &record.tags_url),
        ("teams_url", &record.teams_url),
        ("trees_url", &record.trees_url),
    ]
    .into_iter()
    .map(|(name, value)| (name, serde_json::to_string(value).unwrap()))
    .chain([
        ("id", serde_json::to_string(&record.id).unwrap()),
        ("private", serde_json::to_string(&record.private).unwrap()),
        ("fork", serde_json::to_string(&record.fork).unwrap()),
        ("owner", serde_json::to_string(&record.owner).unwrap()),
        (
            "description",
            serde_json::to_string(&record.description).unwrap(),
        ),
    ]) {
        let field = format!(r#""{name}":{value}"#);
        assert_eq!(wire.matches(&field).count(), 1, "{name}");
        let missing =
            wire.replacen(&format!("{field},"), "", 1)
                .replacen(&format!(",{field}"), "", 1);
        assert_ne!(missing, wire, "{name}");
        assert!(
            serde_json::from_str::<WorkflowRepository>(&missing).is_err(),
            "{name}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowRepository>(missing.as_bytes(), u64::MAX).is_err(),
            "{name}"
        );
    }
}

#[test]
fn workflow_owner_presence_and_account_kinds_follow_the_declared_contract() {
    for input in [
        r#"{"login":"owner","id":1}"#,
        r#"{"login":"owner","id":1,"email":null,"deleted":false,"user_view_type":"public"}"#,
        r#"{"login":"owner","id":1,"email":"owner@example.com","name":"Owner","type":"User"}"#,
        r#"{"login":"owner","id":9007199254740991,"type":"Bot"}"#,
    ] {
        let owner: WorkflowOwner = serde_json::from_str(input).unwrap();
        let encoded = serde_json::to_vec(&owner).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
            amiss_fixtures::canonical_json(&encoded).unwrap(),
        );
        assert_eq!(
            amiss_wire::read_json::<WorkflowOwner>(input.as_bytes(), u64::MAX).unwrap(),
            owner,
        );
    }
    for input in [
        r#"{"login":"owner"}"#,
        r#"{"id":1}"#,
        r#"{"login":null,"id":1}"#,
        r#"{"login":"owner","id":null}"#,
        r#"{"login":"owner","id":9007199254740992}"#,
        r#"{"login":"owner","id":1.0}"#,
        r#"{"login":"owner","id":1e0}"#,
        r#"{"login":"owner","id":-0}"#,
        r#"{"login":"owner","id":1,"unknown":false}"#,
        r#"{"login":"owner","\u006cogin":"duplicate","id":1}"#,
        r#"{"login":"owner","id":1,"name":null}"#,
        r#"{"login":"owner","id":1,"gravatar_id":null}"#,
        r#"{"login":"owner","id":1,"deleted":null}"#,
        r#"{"login":"owner","id":1,"type":null}"#,
        r#"{"login":"owner","id":1,"type":"unknown"}"#,
        r#"{"login":"owner","id":1,"type":{"User":null}}"#,
        r#"{"login":"owner","id":1,"email":7}"#,
    ] {
        assert!(
            serde_json::from_str::<WorkflowOwner>(input).is_err(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowOwner>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}

#[test]
fn workflow_repository_input_refuses_unknown_ambiguous_and_untyped_data() {
    let record: WorkflowRepository = serde_json::from_slice(REPOSITORY).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    for (old, new) in [
        (r#""id":300029405"#, r#""id":9007199254740992"#),
        (r#""id":300029405"#, r#""id":1e0"#),
        (r#""id":300029405"#, r#""id":-0"#),
        (r#""id":300029405"#, r#""id":null"#),
        (r#""private":false"#, r#""private":0"#),
        (r#""fork":false"#, r#""fork":null"#),
        (r#""node_id":"#, r#""unknown":true,"node_id":"#),
        (r#""owner":{"#, r#""owner":{"login":null,"#),
        (r#""owner":{"#, r#""\u006fwner":null,"owner":{"#),
        (r#""login":"octo-org""#, r#""login":{}"#),
    ] {
        assert!(wire.contains(old), "{old}");
        let invalid = wire.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRepository>(&invalid).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowRepository>(invalid.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let owner = serde_json::to_string(&record.owner).unwrap();
    for invalid in [
        wire.replacen(&owner, r#"["octo-org",41548062]"#, 1),
        format!("[{wire}]"),
        format!("{wire} null"),
        "null".to_owned(),
    ] {
        assert!(
            amiss_wire::read_json::<WorkflowRepository>(invalid.as_bytes(), u64::MAX).is_err(),
            "{invalid}"
        );
    }
}
