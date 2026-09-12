use amiss_controller_github::owner::OwnerRecord;
use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::webhook::pull::request::SynchronizePullRequest;

#[test]
fn pull_repository_captures_use_the_shared_repository_facts() {
    let input = include_bytes!("../fixtures/webhook-pull-repository.json");
    let captured: WorkflowRepositoryRecord<Option<OwnerRecord>> =
        serde_json::from_slice(input).unwrap();
    assert_eq!(captured.id, 186_853_002);
    assert_eq!(captured.name, "Hello-World");
    assert_eq!(captured.full_name, "Codertocat/Hello-World");
    assert_eq!(captured.owner.as_ref().unwrap().login, "Codertocat");
    let wire = serde_json::to_string(&captured).unwrap();
    for metadata in [
        r#""license":false,"permissions":[],"description":{},"disabled":null"#,
        r#""visibility":"future","topics":false,"created_at":{},"pushed_at":null"#,
        r#""custom_properties":null,"merge_commit_message":[],"has_issues":1,"unknown":{}"#,
    ] {
        let candidate = wire.replacen('{', &format!("{{{metadata},"), 1);
        assert_eq!(
            serde_json::from_str::<WorkflowRepositoryRecord<Option<OwnerRecord>>>(&candidate)
                .unwrap(),
            captured
        );
    }
}

#[test]
fn synchronize_repository_facts_stay_required_and_bounded() {
    let mut pull: SynchronizePullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    pull.head.repo = None;
    let input = serde_json::to_string(&pull).unwrap();
    let repository = serde_json::to_string(&pull.base.repo).unwrap();
    assert_eq!(input.matches(&repository).count(), 1);
    for (member, value) in [
        ("id", pull.base.repo.id.to_string()),
        ("name", serde_json::to_string(&pull.base.repo.name).unwrap()),
        (
            "full_name",
            serde_json::to_string(&pull.base.repo.full_name).unwrap(),
        ),
        (
            "owner",
            serde_json::to_string(&pull.base.repo.owner).unwrap(),
        ),
    ] {
        let field = format!(r#""{member}":{value}"#);
        for replacement in [
            format!(r#""missing_{member}":{value}"#),
            format!(r#""{member}":false"#),
            format!(r#""{member}":{value},"{member}":{value}"#),
        ] {
            let changed = repository.replacen(&field, &replacement, 1);
            assert_ne!(changed, repository);
            let input = input.replacen(&repository, &changed, 1);
            assert!(
                serde_json::from_str::<SynchronizePullRequest>(&input).is_err(),
                "{member}"
            );
        }
    }
    for invalid in ["9007199254740992", "-1", "1.5", "null"] {
        let changed = repository.replacen(
            &format!(r#""id":{}"#, pull.base.repo.id),
            &format!(r#""id":{invalid}"#),
            1,
        );
        let input = input.replacen(&repository, &changed, 1);
        assert!(
            serde_json::from_str::<SynchronizePullRequest>(&input).is_err(),
            "{invalid}"
        );
    }
}
