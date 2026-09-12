use amiss_controller_github::webhook::app::AppEvent;
use amiss_controller_github::webhook::comment::issue::IssueCommentEvent;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::issue::IssueRecord;
use amiss_controller_github::webhook::suite::CheckSuiteEvent;
use amiss_wire::assessment::Nullable;

#[test]
fn issue_comment_captures_keep_their_event_identity() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT;
    let event: GitHubEvent = serde_json::from_slice(input).unwrap();
    assert!(matches!(event, GitHubEvent::IssueComment(_)));
    assert!(
        serde_json::from_slice::<GitHubEvent>(&serde_json::to_vec(&event).unwrap()).unwrap()
            == event
    );
}

#[test]
fn issue_comment_actions_and_required_changes_are_closed() {
    let event: IssueCommentEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap();
    let wire = serde_json::to_string(&event).unwrap();
    for (replacement, valid) in [
        (r#""action":"created""#, true),
        (r#""action":"deleted""#, true),
        (r#""action":"edited","changes":{}"#, true),
        (
            r#""action":"edited","changes":{"body":{"from":"old"}}"#,
            true,
        ),
        (r#""action":"edited""#, false),
        (r#""action":"edited","changes":null"#, false),
        (
            r#""action":"edited","changes":{"title":{"from":"old"}}"#,
            false,
        ),
        (r#""action":"created","changes":{}"#, false),
        (r#""action":"unknown""#, false),
        (r#""action":null"#, false),
        (r#""action":{"created":null}"#, false),
        (r#""action":"created","\u0061ction":"deleted""#, false),
    ] {
        let candidate = wire.replacen(r#""action":"created""#, replacement, 1);
        assert_eq!(
            serde_json::from_str::<IssueCommentEvent>(&candidate).is_ok(),
            valid,
            "{replacement}"
        );
        assert_eq!(
            amiss_wire::read_json::<IssueCommentEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid
        );
    }
}

#[test]
fn issue_metadata_retains_declared_shapes_without_unknown_or_missing_data() {
    let event: IssueCommentEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap();
    let wire = serde_json::to_string(&event).unwrap();
    for (addition, valid) in [
        (r#""pull_request":{}"#, true),
        (
            r#""pull_request":{"url":"https://api.github.com/repos/a/b/pulls/1","merged_at":null}"#,
            true,
        ),
        (
            r#""type":null,"state_reason":null,"performed_via_github_app":null"#,
            true,
        ),
        (
            r#""type":{"id":1,"node_id":"type","name":"Bug","description":null,"color":"red","is_enabled":true}"#,
            true,
        ),
        (
            r#""sub_issues_summary":{"total":2,"completed":1,"percent_completed":50}"#,
            true,
        ),
        (
            r#""issue_dependencies_summary":{"blocked_by":0,"blocking":1,"total_blocked_by":2,"total_blocking":3}"#,
            true,
        ),
        (r#""unknown":true"#, false),
        (r#""pull_request":{"unknown":true}"#, false),
        (r#""pull_request":{"url":null}"#, false),
        (
            r#""type":{"id":1,"node_id":"type","name":"Bug","description":null,"color":"unknown"}"#,
            false,
        ),
        (r#""sub_issues_summary":{"total":1,"completed":0}"#, false),
        (
            r#""sub_issues_summary":{"total":9007199254740992,"completed":0,"percent_completed":0}"#,
            false,
        ),
        (r#""issue_dependencies_summary":null"#, false),
        (r#""state_reason":false"#, false),
        (r#""type":null,"\u0074ype":null"#, false),
    ] {
        let candidate = wire.replacen(r#""issue":{"#, &format!(r#""issue":{{{addition},"#), 1);
        assert_eq!(
            serde_json::from_str::<IssueCommentEvent>(&candidate).is_ok(),
            valid,
            "{addition}"
        );
        let decoded = amiss_wire::read_json::<IssueCommentEvent>(candidate.as_bytes(), u64::MAX);
        assert_eq!(decoded.is_ok(), valid, "{addition}");
        if let Ok(event) = decoded {
            assert_eq!(
                amiss_fixtures::canonical_json(candidate.as_bytes()).unwrap(),
                amiss_fixtures::canonical_json(&serde_json::to_vec(&event).unwrap()).unwrap()
            );
        }
    }
    for (old, new) in [
        (r#""active_lock_reason":null,"#, ""),
        (r#""closed_at":null,"#, ""),
        (r#""id":444500041"#, r#""id":444500041,"\u0069d":444500041"#),
        (r#""user":{"#, r#""user":{"unknown":true,"#),
        (r#""installation":{"id":1}"#, r#""installation":null"#),
    ] {
        assert!(wire.contains(old), "mutation absent: {old}");
        let candidate = wire.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<IssueCommentEvent>(&candidate).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<IssueCommentEvent>(candidate.as_bytes(), u64::MAX).is_err()
        );
    }
}

#[test]
fn issue_accounts_and_apps_reuse_their_declared_contracts() -> Result<(), Box<dyn std::error::Error>>
{
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT;
    let IssueCommentEvent::Created { event: payload } = serde_json::from_slice(input)? else {
        return Err("expected a created issue-comment event".into());
    };
    let mut issue = payload.issue;
    let wire = serde_json::to_string(&issue).unwrap();
    let user = format!(
        r#""user":{}"#,
        serde_json::to_string(&issue.context.user).unwrap()
    );
    assert_eq!(wire.matches(&user).count(), 1);
    let invalid = wire.replacen(&user, r#""user":null"#, 1);
    assert!(serde_json::from_str::<IssueRecord>(&invalid).is_err());
    assert!(amiss_wire::read_json::<IssueRecord>(invalid.as_bytes(), u64::MAX).is_err());

    let suite: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    let mut app = suite.check_suite.app;
    app.events = Some(vec![AppEvent::Reminder]);
    issue.performed_via_github_app = Some(Nullable::Value(Box::new(app)));
    issue.context.assignee = Nullable::Null;
    issue.assignees = vec![None];
    let encoded = serde_json::to_vec(&issue).unwrap();
    assert_eq!(
        amiss_wire::read_json::<IssueRecord>(&encoded, u64::MAX).unwrap(),
        issue
    );

    let comment = serde_json::to_string(&payload.comment).unwrap();
    let mannequin = comment.replacen(r#""type":"User""#, r#""type":"Mannequin""#, 1);
    assert_ne!(comment, mannequin);
    let event: IssueCommentEvent = serde_json::from_slice(input).unwrap();
    let original = serde_json::to_string(&event).unwrap();
    assert_eq!(original.matches(&comment).count(), 1);
    let wire = original.replacen(&comment, &mannequin, 1);
    for (action, valid) in [
        (r#""created""#, false),
        (r#""edited","changes":{}"#, true),
        (r#""deleted""#, true),
    ] {
        let candidate = wire.replacen(r#""action":"created""#, &format!(r#""action":{action}"#), 1);
        assert_eq!(
            serde_json::from_str::<GitHubEvent>(&candidate).is_ok(),
            valid,
            "{action}"
        );
        assert_eq!(
            amiss_wire::read_json::<GitHubEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid
        );
    }
    Ok(())
}
