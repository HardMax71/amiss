use amiss_controller_github::pull::PullRequestRecord;
use amiss_controller_github::webhook::pull::request::SynchronizePullRequest;

#[test]
fn webhook_pull_profiles_retain_the_same_published_binding_facts() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_PULL;
    let ordinary: PullRequestRecord = serde_json::from_slice(input).unwrap();
    let synchronize: SynchronizePullRequest = serde_json::from_slice(input).unwrap();
    assert_eq!(ordinary.id, synchronize.id);
    assert_eq!(ordinary.number, synchronize.number);
    assert_eq!(ordinary.head.sha, synchronize.head.sha);
    assert_eq!(ordinary.head.branch, synchronize.head.branch);
    assert_eq!(ordinary.base.branch, synchronize.base.branch);
    assert_eq!(
        ordinary.base.repo.as_ref().unwrap().id,
        synchronize.base.repo.id
    );
    let input = serde_json::to_string(&ordinary).unwrap().replacen(
        '{',
        r#"{"allow_auto_merge":null,"allow_update_branch":0,"merge_commit_message":{},"merge_commit_title":"future","unknown":[false],"#,
        1,
    );
    assert!(serde_json::from_str::<PullRequestRecord>(&input).unwrap() == ordinary);
    assert!(serde_json::from_str::<SynchronizePullRequest>(&input).unwrap() == synchronize);
}

#[test]
fn synchronize_refs_preserve_required_nullable_participants() {
    let mut pull: SynchronizePullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    pull.head.repo = None;
    pull.base.repo.owner = None;
    let input = serde_json::to_string(&pull).unwrap();
    assert!(serde_json::from_str::<SynchronizePullRequest>(&input).unwrap() == pull);
    amiss_fixtures::assert_json_rejections::<SynchronizePullRequest>(
        &input,
        &[
            (r#","repo":null"#, ""),
            (r#""owner":null"#, r#""missing_owner":null"#),
            (r#""repo":null"#, r#""repo":false"#),
        ],
    );
}

#[test]
fn pull_profiles_require_only_the_facts_each_flow_consumes() {
    let pull: PullRequestRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    let input = serde_json::to_string(&pull).unwrap();
    for (old, new, ordinary, synchronize) in [
        (r#""mergeable":null,"#, "", false, true),
        (r#""merge_commit_sha":null,"#, "", false, true),
        (r#""state":"open","#, "", false, true),
        (r#""mergeable":null"#, r#""mergeable":{}"#, false, true),
        (r#""id":279147437,"#, "", false, false),
        (
            r#""id":279147437"#,
            r#""id":9007199254740992"#,
            false,
            false,
        ),
        (r#""id":279147437"#, r#""id":null"#, false, false),
        (
            r#""id":279147437"#,
            r#""id":279147437,"\u0069d":1"#,
            false,
            false,
        ),
        ("{", r#"{"unknown":true,"#, true, true),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_ok(),
            ordinary,
            "{new}"
        );
        assert_eq!(
            serde_json::from_str::<SynchronizePullRequest>(&changed).is_ok(),
            synchronize,
            "{new}"
        );
    }
}
