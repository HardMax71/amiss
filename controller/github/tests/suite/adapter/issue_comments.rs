use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::comment::issue::IssueCommentEvent;
use amiss_controller_github::webhook::issue::IssueRecord;
use amiss_controller_github::webhook::issue::context::IssueActivityContext;
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn ordinary_issues_keep_their_nullable_author_contract() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let IssueCommentEvent::Created { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap()
    else {
        panic!("the fixture is a created issue comment")
    };
    let mut issue: IssueRecord<IssueActivityContext> =
        serde_json::from_slice(&serde_json::to_vec(&event.issue).unwrap()).unwrap();
    issue.context.issue_field_values = Some(vec![
        serde_json::from_str(
            r#"{"issue_field_id":1,"node_id":"field","data_type":"number","value":42.5}"#,
        )
        .unwrap(),
    ]);
    let user = serde_json::to_string(&issue.context.user).unwrap();
    let wire = format!(
        r#"{{"action":"opened","issue":{}}}"#,
        serde_json::to_string(&issue).unwrap()
    );
    let member = format!(r#""user":{user}"#);
    assert_eq!(wire.matches(&member).count(), 1);
    let candidate = wire.replacen(&member, r#""user":null"#, 1);
    assert_eq!(
        authenticate_target(&source, candidate.as_bytes(), &target),
        Ok(None)
    );
}

#[test]
fn signed_issue_comments_check_the_complete_root_and_issue() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT;
    assert_eq!(authenticate_target(&source, input, &target), Ok(None));
    let metadata = replaced_once(
        input,
        r#""sender": {"#,
        r#""sender": {"unknown":true,"type":"future","#,
    );
    assert_ne!(metadata, input);
    assert_eq!(authenticate_target(&source, &metadata, &target), Ok(None));
    let invalid = replaced_once(input, r#""issue": {"#, r#""issue": {"unknown":true,"#);
    assert_ne!(invalid, input);
    assert_eq!(
        authenticate_target(&source, &invalid, &target),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn issue_members_cannot_turn_into_pull_request_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let IssueCommentEvent::Created { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap()
    else {
        panic!("the fixture is a created issue comment")
    };
    let mut pull: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let issue = serde_json::to_string(&event.issue).unwrap();
    for action in ["opened", "synchronize", "closed"] {
        pull.action = Some(action.to_owned());
        let wire = replaced_once(
            &serde_json::to_vec(&pull).unwrap(),
            "{",
            &format!(r#"{{"issue":{issue},"#),
        );
        assert_eq!(
            authenticate_target(&source, &wire, &target),
            Err(ProviderError::Authentication),
            "{action}"
        );
    }
}

#[test]
fn incomplete_comment_events_cannot_fall_back_to_partial_deliveries() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT;
    for action in ["created", "edited", "deleted"] {
        let mut wire = replaced_once(
            input,
            r#""action": "created""#,
            &format!(r#""action":"{action}""#),
        );
        if action == "edited" {
            wire = replaced_once(&wire, "{", r#"{"changes":{},"#);
        }
        for (old, new) in [
            (r#""issue": {"#, r#""issue": {"unknown":true,"#),
            (r#""number": 1,"#, ""),
            (r#""changes":{},"#, ""),
        ] {
            if old == r#""changes":{},"# && action != "edited" {
                continue;
            }
            let invalid = replaced_once(&wire, old, new);
            assert!(invalid != wire, "mutation absent: {old}");
            assert_eq!(
                authenticate_target(&source, &invalid, &target),
                Err(ProviderError::Authentication),
                "{action}: {old}"
            );
        }
    }
}
