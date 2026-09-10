use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::comment::{Comment, CommentPayload, ReviewCommentEvent};
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::pull::review::CommentPullRequest;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_wire::model::BranchRef;
use sha2::{Digest as _, Sha256};

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn signed_review_comments_retain_the_published_events_and_remain_no_work() {
    let ReviewEvent::Submitted { event } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("the fixture is a submitted review")
    };
    let mut pull: CommentPullRequest =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL).unwrap();
    pull.draft = None;
    pull.auto_merge = None;
    let created = serde_json::to_vec(&ReviewCommentEvent::Created {
        event: CommentPayload {
            comment: serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
            pull_request: pull,
            repository: event.repository,
            sender: event.sender,
            installation: event.installation,
            organization: event.organization,
            enterprise: event.enterprise,
        },
    })
    .unwrap();
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for (action, digest) in [
        (
            r#""action":"created""#,
            "73b7ac2417f47264813ee91a4dff5789bd260317d7a00fe13b4f6b8f73f159c9",
        ),
        (
            r#""action":"edited","changes":{"body":{"from":""}}"#,
            "cadd015a43725b2e526e996d16e8b60f74e4f14ecdd2be027a565059ac224c3f",
        ),
        (
            r#""action":"deleted""#,
            "5949039e952e5fdc8b396d9dbe65ce07bdd20097f660d83766a234bcce8643a4",
        ),
    ] {
        let wire = replaced_once(&created, r#""action":"created""#, action);
        let canonical = amiss_fixtures::canonical_json(&wire).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&canonical)),
            digest,
            "complete published event"
        );
        let event: GitHubEvent = amiss_wire::read_json(&wire, u64::MAX).unwrap();
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
            ("{", r#"{"unknown":true,"#),
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
    let payload: GitHubPayload = serde_json::from_slice(input).unwrap();
    assert!(matches!(payload.comment, Some(Comment::Issue(_))));
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
    pull.comment = payload.comment;
    for action in ["opened", "edited", "synchronize", "closed"] {
        pull.action = Some(action.to_owned());
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&pull).unwrap(), &target),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn review_comments_cannot_fall_back_to_partial_or_active_prs() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let comment = Comment::Review(Box::new(
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
    ));
    let comment_member = format!(r#""comment":{}"#, serde_json::to_string(&comment).unwrap());
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    payload.comment = Some(comment);
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
        let wire = serde_json::to_vec(&payload).unwrap();
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
    payload.comment = None;
    for action in ["created", "deleted"] {
        payload.action = Some(action.to_owned());
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&payload).unwrap(), &target),
            Err(ProviderError::Authentication),
            "missing comment: {action}"
        );
    }
}
