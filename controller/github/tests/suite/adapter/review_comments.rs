use amiss_controller::ProviderError;
use amiss_controller_github::webhook::comment::issue::IssueCommentEvent;
use amiss_controller_github::webhook::comment::{
    CommentPayload, ReviewCommentEvent, ReviewCommentRecord,
};
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::pull::review::CommentPullRequest;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_controller_github::webhook::{Absent, GitHubPayload};
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn signed_review_comments_retain_the_published_events_and_remain_no_work() {
    let ReviewEvent::Submitted { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("the fixture is a submitted review")
    };
    let mut pull: CommentPullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL).unwrap();
    pull.draft = None;
    pull.auto_merge = None;
    let created = serde_json::to_vec(&ReviewCommentEvent::Created {
        changes: Absent,
        event: CommentPayload {
            comment: serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
            pull_request: pull,
            repository: event.repository,
            installation: event.installation,
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
    })
    .unwrap();
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for action in [
        r#""action":"created""#,
        r#""action":"edited","changes":{"body":{"from":""}}"#,
        r#""action":"deleted""#,
    ] {
        let wire = replaced_once(&created, r#""action":"created""#, action);
        let event: GitHubEvent = serde_json::from_slice(&wire).unwrap();
        assert!(matches!(event, GitHubEvent::ReviewComment(_)));
        assert_eq!(authenticate_target(&source, &wire, &target), Ok(None));
        if action.contains("edited") {
            let missing = replaced_once(&wire, r#""changes":{"body":{"from":""}},"#, "");
            assert_ne!(missing, wire);
            assert_eq!(
                authenticate_target(&source, &missing, &target),
                Err(ProviderError::Authentication)
            );
        }
        for (old, new) in [
            (r#""comment":{"#, r#""comment":{"unknown":true,"#),
            (r#""id":279147437"#, r#""id":279147437,"unknown":true"#),
            (r#""original_line":265,"#, ""),
            (r#""id":284312630"#, r#""id":284312630,"\u0069d":284312630"#),
        ] {
            let candidate = replaced_once(&wire, old, new);
            assert_ne!(candidate, wire);
            assert_eq!(
                authenticate_target(&source, &candidate, &target),
                Err(ProviderError::Authentication),
                "{action}: {new}"
            );
        }
        for (old, new, valid) in [
            (
                r#""original_line":265"#,
                r#""original_line":null"#,
                action.contains("created"),
            ),
            (
                r#""assignees":["#,
                r#""assignees":[{"login":"ghost","id":1,"type":"Mannequin"},"#,
                action.contains("created"),
            ),
            (
                r#""requested_reviewers":["#,
                r#""requested_reviewers":[{"name":"docs","id":1},"#,
                !action.contains("created"),
            ),
        ] {
            let candidate = replaced_once(&wire, old, new);
            assert_ne!(candidate, wire);
            assert_eq!(
                authenticate_target(&source, &candidate, &target),
                valid.then_some(None).ok_or(ProviderError::Authentication),
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn issue_comments_remain_no_work_without_becoming_pr_deliveries() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT;
    let IssueCommentEvent::Created { event, .. } = serde_json::from_slice(input).unwrap() else {
        panic!("the fixture is a created issue comment")
    };
    for action in ["created", "edited", "deleted"] {
        let mut wire = replaced_once(
            input,
            r#""action": "created""#,
            &format!(r#""action":"{action}""#),
        );
        if action == "edited" {
            wire = replaced_once(&wire, "{", r#"{"changes":{},"#);
        }
        assert_eq!(authenticate_target(&source, &wire, &target), Ok(None));
    }
    let mut pull: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let comment = serde_json::to_string(&event.comment).unwrap();
    for action in ["opened", "edited", "synchronize", "closed"] {
        pull.action = Some(action.to_owned());
        let wire = replaced_once(
            &serde_json::to_vec(&pull).unwrap(),
            "{",
            &format!(r#"{{"comment":{comment},"#),
        );
        assert_eq!(
            authenticate_target(&source, &wire, &target),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn review_comments_cannot_fall_back_to_partial_or_active_prs() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let comment: ReviewCommentRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap();
    let comment_member = format!(r#""comment":{}"#, serde_json::to_string(&comment).unwrap());
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    for action in [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "closed",
        "created",
        "deleted",
    ] {
        payload.action = Some(action.to_owned());
        let wire = replaced_once(
            &serde_json::to_vec(&payload).unwrap(),
            "{",
            &format!("{{{comment_member},"),
        );
        let null = replaced_once(&wire, &comment_member, r#""comment":null"#);
        assert_ne!(null, wire);
        for candidate in [&wire, &null] {
            assert_eq!(
                authenticate_target(&source, candidate, &target),
                Err(ProviderError::Authentication),
                "{action}"
            );
        }
    }
    for action in ["created", "deleted"] {
        payload.action = Some(action.to_owned());
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&payload).unwrap(), &target),
            Err(ProviderError::Authentication),
            "missing comment: {action}"
        );
    }
}
