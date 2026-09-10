use amiss_controller_github::pull::PullRequestRecord;
use amiss_controller_github::repository::metadata::{
    MergeCommitMessage, MergeCommitTitle, SquashMergeCommitMessage, SquashMergeCommitTitle,
};
use amiss_controller_github::webhook::pull::LockReason;
use amiss_controller_github::webhook::pull::request::{PullRequestWebhook, SynchronizePullRequest};
use amiss_wire::assessment::Nullable;

#[test]
fn published_pull_request_retains_all_metadata() {
    for input in [
        include_bytes!("../fixtures/pull-request.json").as_slice(),
        include_bytes!("../fixtures/pull-request-closed.json").as_slice(),
    ] {
        let ordinary: PullRequestWebhook = amiss_wire::read_json(input, u64::MAX).unwrap();
        assert!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&ordinary).unwrap()).unwrap()
                == amiss_fixtures::canonical_json(input).unwrap(),
            "the ordinary profile must retain the real open and deleted-head captures",
        );
    }
    let input = amiss_fixtures::GITHUB_WEBHOOK_PULL;
    let ordinary: PullRequestWebhook = amiss_wire::read_json(input, u64::MAX).unwrap();
    let synchronize: SynchronizePullRequest = amiss_wire::read_json(input, u64::MAX).unwrap();
    for encoded in [
        serde_json::to_vec(&ordinary).unwrap(),
        serde_json::to_vec(&synchronize).unwrap(),
    ] {
        assert!(
            amiss_fixtures::canonical_json(&encoded).unwrap()
                == amiss_fixtures::canonical_json(input).unwrap(),
            "the complete published PR must not lose metadata",
        );
    }
    assert_eq!(ordinary.request.id, synchronize.id);
    assert_eq!(ordinary.request.head.sha, synchronize.head.sha);
    assert_eq!(ordinary.request.labels, synchronize.labels);
    assert_eq!(synchronize.active_lock_reason, Nullable::Null);
    assert_eq!(synchronize.merged, Some(Nullable::Value(false)));
}

#[test]
fn ordinary_webhook_extensions_compose_the_existing_closed_rest_record() {
    let mut pull: PullRequestWebhook =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    pull.allow_auto_merge = Some(true);
    pull.allow_update_branch = Some(false);
    pull.delete_branch_on_merge = Some(true);
    pull.merge_commit_message = Some(MergeCommitMessage::PrBody);
    pull.merge_commit_title = Some(MergeCommitTitle::MergeMessage);
    pull.squash_merge_commit_message = Some(SquashMergeCommitMessage::CommitMessages);
    pull.squash_merge_commit_title = Some(SquashMergeCommitTitle::CommitOrPrTitle);
    pull.use_squash_pr_title_as_default = Some(false);
    let input = serde_json::to_string(&pull).unwrap();
    assert!(
        amiss_wire::read_json::<PullRequestWebhook>(input.as_bytes(), u64::MAX).unwrap() == pull,
    );
    assert!(serde_json::from_str::<PullRequestRecord>(&input).is_err());
    assert!(serde_json::from_str::<SynchronizePullRequest>(&input).is_err());
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""allow_auto_merge":true"#, r#""allow_auto_merge":null"#),
        (
            r#""allow_update_branch":false"#,
            r#""allow_update_branch":0"#,
        ),
        (
            r#""merge_commit_message":"PR_BODY""#,
            r#""merge_commit_message":"unknown""#,
        ),
        (
            r#""merge_commit_title":"MERGE_MESSAGE""#,
            r#""merge_commit_title":{"MERGE_MESSAGE":null}"#,
        ),
        (
            r#""allow_auto_merge":true"#,
            r#""allow_auto_merge":true,"allow_auto_merge":false"#,
        ),
        (r#""id":279147437"#, r#""id":279147437,"\u0069d":279147437"#),
        (r#""head":{"#, r#""head":{"unknown":true,"#),
        (r#""merged":false,"#, ""),
    ] {
        assert!(input.contains(old), "{old}");
        let invalid = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRequestWebhook>(&invalid).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestWebhook>(invalid.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
}

#[test]
fn synchronize_keeps_its_nullable_participants_and_merge_states() {
    let mut pull: SynchronizePullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    pull.user = Nullable::Null;
    pull.assignees = vec![None];
    pull.requested_reviewers = vec![None];
    pull.requested_teams = vec![serde_json::from_str(r#"{"name":"docs","id":1}"#).unwrap()];
    pull.head.user = None;
    pull.head.repo = None;
    pull.base.repo.owner = None;
    pull.merged = Some(Nullable::Null);
    pull.mergeable = None;
    pull.rebaseable = Some(Nullable::Value(true));
    let input = serde_json::to_vec(&pull).unwrap();
    assert!(amiss_wire::read_json::<SynchronizePullRequest>(&input, u64::MAX).unwrap() == pull,);
    assert!(serde_json::from_slice::<PullRequestWebhook>(&input).is_err());
    for (reason, spelling) in [
        (LockReason::Resolved, "resolved"),
        (LockReason::OffTopic, "off-topic"),
        (LockReason::TooHeated, "too heated"),
        (LockReason::Spam, "spam"),
    ] {
        assert_eq!(
            serde_json::to_string(&reason).unwrap(),
            format!("\"{spelling}\"")
        );
    }
}

#[test]
fn pull_profiles_keep_their_distinct_presence_and_scalar_rules() {
    let pull: SynchronizePullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    let input = serde_json::to_string(&pull).unwrap();
    for (old, new, ordinary, synchronize) in [
        (r#""mergeable":null,"#, "", false, true),
        (r#""draft":false,"#, "", true, false),
        (r#""requested_teams":[],"#, "", true, false),
        (r#""active_lock_reason":null,"#, "", true, false),
        (
            r#""active_lock_reason":null"#,
            r#""active_lock_reason":"custom""#,
            true,
            false,
        ),
        (r#""merged":false"#, r#""merged":null"#, false, true),
        (
            r#""body":"This is a pretty simple change that we need to pull into master.""#,
            r#""body":{}"#,
            false,
            false,
        ),
        (r#""auto_merge":null,"#, "", false, false),
        (
            r#""id":279147437"#,
            r#""id":9007199254740992"#,
            false,
            false,
        ),
        (r#""user":{"#, r#""user":{"unknown":true,"#, false, false),
        ("{", r#"{"unknown":true,"#, false, false),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<PullRequestWebhook>(&changed).is_ok(),
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
