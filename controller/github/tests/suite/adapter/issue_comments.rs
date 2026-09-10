use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, source};

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
