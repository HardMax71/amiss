use amiss_controller::ProviderError;
use amiss_controller_github::GitHubPullRequestSource;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::workflow::{WorkflowRunAction, WorkflowRunEvent};
use amiss_wire::model::BranchRef;

use super::{
    BODY, authenticate_target, provider, replaced_once, source, webhook, workflow_artifact,
    workflow_payload,
};

#[test]
fn signed_workflow_events_reject_unknown_root_metadata() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let input = amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN;
    assert_eq!(authenticate_target(&source, input, &target), Ok(None));
    let invalid = replaced_once(input, "{", r#"{"unknown":true,"#);
    assert!(invalid != input, "mutation must change the body");
    assert_eq!(
        authenticate_target(&source, &invalid, &target),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn workflow_roots_are_checked_before_a_configured_completion_becomes_work() {
    let source = GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]);
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut payload = workflow_payload();
    payload.workflow = None;
    let input = serde_json::to_vec(&payload).unwrap();
    assert!(matches!(
        authenticate_target(&source, &input, &target),
        Ok(Some(_))
    ));
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        ("{", r#"{"number":42,"#),
        (r#""sender":{"#, r#""sender":{"login":null,"#),
        (r#""workflow":null,"#, ""),
        (r#""action":"completed","#, ""),
        (r#""action":"completed""#, r#""action":null"#),
        (r#""workflow_run":{"#, r#""workflow_run":{"unknown":true,"#),
    ] {
        let candidate = replaced_once(&input, old, new);
        assert!(candidate != input, "mutation absent: {old}");
        assert_eq!(
            authenticate_target(&source, &candidate, &target),
            Err(ProviderError::Authentication),
            "{new}"
        );
    }
    payload.action = WorkflowRunAction::InProgress;
    assert_eq!(
        authenticate_target(&source, &serde_json::to_vec(&payload).unwrap(), &target),
        Ok(None)
    );
}

#[test]
fn workflow_markers_cannot_fall_back_into_pr_delivery_paths() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let event: WorkflowRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN).unwrap();
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    for action in ["opened", "synchronize", "closed"] {
        payload.action = Some(action.to_owned());
        let input = serde_json::to_string(&payload).unwrap();
        for (field, value) in [
            ("workflow", serde_json::to_string(&event.workflow).unwrap()),
            ("workflow", "null".to_owned()),
            (
                "workflow_run",
                serde_json::to_string(&event.workflow_run).unwrap(),
            ),
            ("workflow_run", "null".to_owned()),
        ] {
            let candidate = input.replacen('{', &format!(r#"{{"{field}":{value},"#), 1);
            assert_eq!(
                authenticate_target(&source, candidate.as_bytes(), &target),
                Err(ProviderError::Authentication),
                "{action}: {field}"
            );
        }
    }
}

#[test]
fn requested_workflows_are_typed_no_work_even_with_a_successful_run() {
    let source = GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]);
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut payload = workflow_payload();
    for valid in [true, false] {
        if !valid {
            payload.workflow_run.title.display_title = None;
        }
        let input = serde_json::to_string(&payload).unwrap().replacen(
            r#""action":"completed""#,
            r#""action":"requested""#,
            1,
        );
        assert_eq!(
            authenticate_target(&source, input.as_bytes(), &target),
            if valid {
                Ok(None)
            } else {
                Err(ProviderError::Authentication)
            }
        );
    }
}
