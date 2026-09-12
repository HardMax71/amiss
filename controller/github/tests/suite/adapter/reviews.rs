use amiss_controller::ProviderError;
use amiss_controller_github::GitHubPullRequestSource;
use amiss_controller_github::webhook::comment::issue::IssueCommentEvent;
use amiss_controller_github::webhook::comment::{CommentPayload, ReviewCommentEvent};
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::pull::review::CommentPullRequest;
use amiss_controller_github::webhook::review::{ReviewEvent, ReviewPayload};
use amiss_controller_github::webhook::thread::{ReviewThread, ReviewThreadEvent, ThreadPayload};
use amiss_controller_github::webhook::{Absent, GitHubPayload};
use amiss_wire::model::BranchRef;

use super::{
    BODY, authenticate_target, provider, replaced_once, source, webhook, workflow_artifact,
};

fn review_events() -> [GitHubEvent; 4] {
    let event: ReviewPayload =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap();
    let mut pull: CommentPullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL).unwrap();
    pull.draft = None;
    pull.auto_merge = None;
    let comment = ReviewCommentEvent::Created {
        changes: Absent,
        event: CommentPayload {
            comment: serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
            pull_request: pull,
            repository: event.repository.clone(),
            installation: event.installation.clone(),
            number: Absent,
            issue: Absent,
            review: Absent,
            thread: Absent,
            check_run: Absent,
            check_suite: Absent,
            workflow: Absent,
            workflow_run: Absent,
            requested_action: Absent,
        },
    };
    let thread = ReviewThreadEvent::Resolved {
        event: ThreadPayload {
            thread: ReviewThread {
                node_id: "thread".to_owned(),
                comments: vec![
                    serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
                ],
            },
            pull_request: serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL)
                .unwrap(),
            repository: event.repository.clone(),
            installation: event.installation.clone(),
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
    };
    let issue: IssueCommentEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap();
    [
        GitHubEvent::Review(Box::new(ReviewEvent::Submitted {
            event,
            changes: Absent,
        })),
        GitHubEvent::ReviewComment(Box::new(comment)),
        GitHubEvent::IssueComment(Box::new(issue)),
        GitHubEvent::ReviewThread(Box::new(thread)),
    ]
}

#[test]
fn review_roots_keep_typed_events_without_actor_metadata() {
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for (event, (markers, actions)) in review_events().into_iter().zip([
        (
            ["review", "pull_request"],
            &["submitted", "dismissed", "edited"][..],
        ),
        (
            ["comment", "pull_request"],
            &["created", "edited", "deleted"][..],
        ),
        (["issue", "comment"], &["created", "edited", "deleted"][..]),
        (["thread", "pull_request"], &["resolved", "unresolved"][..]),
    ]) {
        let wire = serde_json::to_string(&event).unwrap();
        let old_action = format!(r#""action":"{}""#, actions[0]);
        assert_eq!(wire.matches(&old_action).count(), 1);
        for source in [
            source(),
            GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]),
        ] {
            for &action in actions {
                let mut input = wire.replacen(&old_action, &format!(r#""action":"{action}""#), 1);
                if action == "edited" {
                    input = input.replacen('{', r#"{"changes":{},"#, 1);
                } else if action == "dismissed" {
                    input = input.replacen(r#""state":"commented""#, r#""state":"dismissed""#, 1);
                }
                let original: GitHubEvent = serde_json::from_str(&input).unwrap();
                assert_eq!(
                    std::mem::discriminant(&original),
                    std::mem::discriminant(&event)
                );
                assert_eq!(
                    authenticate_target(&source, input.as_bytes(), &target),
                    Ok(None)
                );
                for metadata in [
                    r#"{"sender":null,"organization":false,"enterprise":[],"updated_at":false,"future":1.5,"#,
                    r#"{"sender":{"login":null},"organization":{},"enterprise":null,"future":{"review":null},"#,
                ] {
                    let changed = input.replacen('{', metadata, 1);
                    assert!(serde_json::from_str::<GitHubEvent>(&changed).unwrap() == original);
                    assert_eq!(
                        authenticate_target(&source, changed.as_bytes(), &target),
                        Ok(None)
                    );
                }
                for field in [
                    "number",
                    "pull_request",
                    "issue",
                    "review",
                    "comment",
                    "thread",
                    "check_run",
                    "check_suite",
                    "workflow",
                    "workflow_run",
                    "requested_action",
                    "changes",
                ]
                .into_iter()
                .filter(|field| {
                    !markers.contains(field) && (*field != "changes" || action != "edited")
                }) {
                    for value in ["null", "false", "42", "[]", "{}", r#"{"id":1}"#] {
                        let changed = input.replacen('{', &format!(r#"{{"{field}":{value},"#), 1);
                        assert_eq!(
                            authenticate_target(&source, changed.as_bytes(), &target),
                            Err(ProviderError::Authentication),
                            "{markers:?}, {action}: {field}={value}"
                        );
                    }
                }
                if action == "edited" {
                    for replacement in ["", r#""changes":null,"#, r#""changes":{},"changes":{},"#] {
                        let changed = input.replacen(r#""changes":{},"#, replacement, 1);
                        assert_ne!(changed, input);
                        assert_eq!(
                            authenticate_target(&source, changed.as_bytes(), &target),
                            Err(ProviderError::Authentication),
                            "{markers:?}: {replacement}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn nested_review_metadata_cannot_change_typed_events_or_delivery() {
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for original in review_events() {
        let input = serde_json::to_string(&original).unwrap();
        for source in [
            source(),
            GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]),
        ] {
            for prefix in [
                r#""review":{"#,
                r#""comment":{"#,
                r#""issue":{"#,
                r#""thread":{"#,
                r#""pull_request":{"#,
                r#""user":{"#,
                r#""comments":[{"#,
                r#""labels":[{"#,
                r#""reactions":{"#,
                r#""_links":{"#,
                r#""html":{"#,
                r#""self":{"#,
            ]
            .into_iter()
            .filter(|prefix| input.contains(prefix))
            {
                let changed = input.replace(
                    prefix,
                    &format!(
                        r#"{prefix}"future_metadata":{{"action":"opened","nested":[null,1.5]}},"#
                    ),
                );
                assert!(serde_json::from_str::<GitHubEvent>(&changed).unwrap() == original);
                assert_eq!(
                    authenticate_target(&source, changed.as_bytes(), &target),
                    Ok(None),
                    "{prefix}"
                );
            }
        }
    }
}

#[test]
fn signed_review_edits_are_no_work_without_losing_their_contract() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let edited = replaced_once(
        amiss_fixtures::GITHUB_WEBHOOK_REVIEW,
        r#""action": "submitted""#,
        r#""action": "edited", "changes": {}"#,
    );
    assert_eq!(authenticate_target(&source, &edited, &target), Ok(None));
    let unknown = replaced_once(&edited, "{", r#"{"unknown":true,"#);
    assert_eq!(authenticate_target(&source, &unknown, &target), Ok(None));
}

#[test]
fn malformed_reviews_cannot_downgrade_to_partial_or_active_prs() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let ReviewEvent::Submitted { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("the fixture is a submitted review")
    };
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let review_member = format!(
        r#""review":{}"#,
        serde_json::to_string(&event.review).unwrap()
    );
    for action in [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "closed",
        "submitted",
        "dismissed",
    ] {
        payload.action = Some(action.to_owned());
        let wire = replaced_once(
            &serde_json::to_vec(&payload).unwrap(),
            "{",
            &format!("{{{review_member},"),
        );
        assert_eq!(
            authenticate_target(&source, &wire, &target),
            Err(ProviderError::Authentication),
            "{action}"
        );
        let null = replaced_once(&wire, &review_member, r#""review":null"#);
        assert_ne!(null, wire);
        assert_eq!(
            authenticate_target(&source, &null, &target),
            Err(ProviderError::Authentication),
            "{action}: null review"
        );
    }
}
