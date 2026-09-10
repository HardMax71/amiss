use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

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
    assert_eq!(
        authenticate_target(&source, &unknown, &target),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn malformed_reviews_cannot_downgrade_to_partial_or_active_prs() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let ReviewEvent::Submitted { event } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("the fixture is a submitted review")
    };
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    payload.review = Some(event.review);
    let review_member = format!(
        r#""review":{}"#,
        serde_json::to_string(&payload.review).unwrap()
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
        let wire = serde_json::to_vec(&payload).unwrap();
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
