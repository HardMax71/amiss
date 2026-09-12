use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn ordinary_issues_keep_their_nullable_author_contract() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut payload: GitHubPayload =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap();
    payload.action = Some("opened".to_owned());
    payload.comment = None;
    payload.issue.as_mut().unwrap().context.issue_field_values = Some(vec![
        serde_json::from_str(
            r#"{"issue_field_id":1,"node_id":"field","data_type":"number","value":42.5}"#,
        )
        .unwrap(),
    ]);
    let user = serde_json::to_string(&payload.issue.as_ref().unwrap().context.user).unwrap();
    let wire = serde_json::to_string(&payload).unwrap();
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
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""issue": {"#, r#""issue": {"unknown":true,"#),
        (r#""sender": {"#, r#""sender": {"login":null,"#),
    ] {
        let invalid = replaced_once(input, old, new);
        assert!(invalid != input, "mutation absent: {old}");
        assert_eq!(
            authenticate_target(&source, &invalid, &target),
            Err(ProviderError::Authentication),
            "{new}"
        );
    }
}

#[test]
fn issue_members_cannot_turn_into_pull_request_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let event: GitHubPayload =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap();
    let mut pull: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    pull.issue = event.issue;
    assert!(pull.issue.is_some());
    for action in ["opened", "synchronize", "closed"] {
        pull.action = Some(action.to_owned());
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&pull).unwrap(), &target),
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
            ("{", r#"{"unknown":true,"#),
            (r#""issue": {"#, r#""issue": {"unknown":true,"#),
            (r#""sender": {"#, r#""sender": {"login":null,"#),
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
