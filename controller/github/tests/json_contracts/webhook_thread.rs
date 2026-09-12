use amiss_controller_github::owner::OwnerRecord;
use amiss_controller_github::pull::PullRefRecord;
use amiss_controller_github::pull::metadata::{AutoMergeRecord, MergeMethod};
use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::webhook::Absent;
use amiss_controller_github::webhook::pull::Team;
use amiss_controller_github::webhook::pull::thread::ThreadPullRequest;
use amiss_controller_github::webhook::repository::WorkflowOwner;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_controller_github::webhook::thread::{ReviewThread, ReviewThreadEvent, ThreadPayload};
use amiss_wire::assessment::Nullable;

#[test]
fn thread_envelopes_keep_optional_metadata_and_action_specific_merge_titles() {
    let ReviewEvent::Submitted { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("submitted review")
    };
    let mut pull: ThreadPullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL).unwrap();
    pull.auto_merge = Nullable::Value(AutoMergeRecord {
        enabled_by: None,
        merge_method: MergeMethod::Merge,
        commit_title: Some("docs".to_owned()),
        commit_message: None,
    });
    let input = serde_json::to_string(&ReviewThreadEvent::Resolved {
        event: ThreadPayload {
            thread: ReviewThread {
                node_id: "thread".to_owned(),
                comments: Vec::new(),
            },
            pull_request: pull,
            repository: event.repository,
            installation: None,
            number: Absent,
            issue: Absent,
            review: Absent,
            comment: Absent,
            check_run: Absent,
            check_suite: Absent,
            workflow: Absent,
            workflow_run: Absent,
            requested_action: Absent,
            changes: Absent,
        },
    })
    .unwrap();
    for action in ["resolved", "unresolved"] {
        let input = input.replacen(
            r#""action":"resolved""#,
            &format!(r#""action":"{action}""#),
            1,
        );
        assert!(serde_json::from_str::<ReviewThreadEvent>(&input).is_ok());
        for (old, new, valid) in [
            ("{", r#"{"updated_at":null,"#, true),
            ("{", r#"{"updated_at":"2026-09-10T00:00:00Z","#, true),
            ("{", r#"{"updated_at":false,"#, true),
            ("{", r#"{"sender":null,"#, true),
            ("{", r#"{"installation":null,"#, false),
            ("{", r#"{"organization":null,"#, true),
            ("{", r#"{"enterprise":null,"#, true),
            (
                r#""commit_title":"docs""#,
                r#""commit_title":null"#,
                action == "resolved",
            ),
        ] {
            assert!(input.contains(old));
            let candidate = input.replacen(old, new, 1);
            assert_eq!(
                serde_json::from_str::<ReviewThreadEvent>(&candidate).is_ok(),
                valid,
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn thread_pr_profiles_keep_head_and_merge_title_rules() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL;
    let mut pull: ThreadPullRequest = serde_json::from_slice(input).unwrap();
    let encoded = serde_json::to_vec(&pull).unwrap();
    assert!(serde_json::from_slice::<ThreadPullRequest>(&encoded).unwrap() == pull);
    let head = serde_json::to_string(&pull.context.head.repo).unwrap();
    pull.auto_merge = Nullable::Value(AutoMergeRecord {
        enabled_by: None,
        merge_method: MergeMethod::Merge,
        commit_title: Some("docs".to_owned()),
        commit_message: None,
    });
    let input = serde_json::to_string(&pull).unwrap();
    for (old, new, resolved, unresolved) in [
        ("{", r#"{"unknown":true,"#, true, true),
        ("{", r#"{"stack":null,"#, false, false),
        (head.as_str(), "null", true, false),
        (
            r#""head":{"#,
            r#""head":{"label":null,"user":false,"#,
            true,
            true,
        ),
        (
            r#""commit_title":"docs""#,
            r#""commit_title":null"#,
            true,
            false,
        ),
        (r#""commit_title":"docs","#, "", false, false),
        (r#","commit_message":null"#, "", false, false),
        (
            r#""commit_message":null"#,
            r#""commit_message":"""#,
            true,
            true,
        ),
    ] {
        assert!(input.contains(old));
        let candidate = input.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<ThreadPullRequest>(&candidate).is_ok(),
            resolved,
            "{new}"
        );
        assert_eq!(
            serde_json::from_str::<
                ThreadPullRequest<
                    WorkflowOwner,
                    PullRefRecord<WorkflowRepositoryRecord<Option<OwnerRecord>>>,
                    Team,
                    String,
                >,
            >(&candidate)
            .is_ok(),
            unresolved,
            "{new}"
        );
    }
}

#[test]
fn thread_comments_are_complete_objects_and_never_optional() {
    let thread = ReviewThread {
        node_id: "PRRT_kwDOFd42Pc4rQOUv".to_owned(),
        comments: vec![
            serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
        ],
    };
    let input = serde_json::to_string(&thread).unwrap();
    assert_eq!(
        amiss_wire::read_json::<ReviewThread>(input.as_bytes(), u64::MAX).unwrap(),
        thread
    );
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""node_id":"PRRT_kwDOFd42Pc4rQOUv","#, ""),
        (r#""comments":["#, r#""comments":[null,"#),
        (r#""comments":[{"#, r#""comments":[{"unknown":true,"#),
        (r#""original_line":265,"#, ""),
    ] {
        assert!(input.contains(old));
        assert!(
            amiss_wire::read_json::<ReviewThread>(input.replacen(old, new, 1).as_bytes(), u64::MAX)
                .is_err(),
            "{new}"
        );
    }
    for input in [
        r#"{"node_id":"thread"}"#,
        r#"{"node_id":"thread","comments":null}"#,
    ] {
        assert!(serde_json::from_str::<ReviewThread>(input).is_err());
    }
    assert!(serde_json::from_str::<ReviewThread>(r#"{"node_id":"thread","comments":[]}"#).is_ok());
}
