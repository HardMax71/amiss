use amiss_controller_github::webhook::workflow::{RequestedWorkflowRunAction, WorkflowRunEvent};

#[test]
fn workflow_event_captures_keep_their_completed_action() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN;
    let event: WorkflowRunEvent = serde_json::from_slice(input).unwrap();
    assert_eq!(
        event.action,
        amiss_controller_github::webhook::workflow::WorkflowRunAction::Completed
    );
    assert!(
        serde_json::from_slice::<WorkflowRunEvent>(&serde_json::to_vec(&event).unwrap()).unwrap()
            == event
    );
}

#[test]
fn workflow_event_nullability_does_not_hide_missing_required_members() {
    let mut event: WorkflowRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN).unwrap();
    event.workflow = None;
    let input = serde_json::to_string(&event).unwrap();
    assert!(
        amiss_wire::read_json::<WorkflowRunEvent>(input.as_bytes(), u64::MAX).unwrap() == event
    );
    for (old, new) in [
        (r#""workflow":null,"#, ""),
        (r#""action":"completed","#, ""),
        (r#""action":"completed""#, r#""action":null"#),
        (r#""action":"completed""#, r#""action":{"completed":null}"#),
        (r#""action":"completed""#, r#""action":"unknown""#),
        (r#""sender":{"#, r#""sender":{"login":null,"#),
        ("{", r#"{"installation":null,"#),
        ("{", r#"{"enterprise":null,"#),
        ("{", r#"{"unknown":true,"#),
    ] {
        assert!(input.contains(old), "mutation absent: {old}");
        let candidate = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRunEvent>(&candidate).is_err(),
            "{new}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunEvent>(candidate.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn requested_workflow_runs_ignore_unused_display_titles() {
    let event: WorkflowRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN).unwrap();
    let requested = serde_json::to_string(&event).unwrap().replacen(
        r#""action":"completed""#,
        r#""action":"requested""#,
        1,
    );
    for addition in [
        "",
        r#""display_title":null,"#,
        r#""display_title":false,"#,
        r#""display_title":"Workflow title","#,
    ] {
        let input = requested.replacen(
            r#""workflow_run":{"#,
            &format!(r#""workflow_run":{{{addition}"#),
            1,
        );
        let decoded: WorkflowRunEvent<RequestedWorkflowRunAction> =
            serde_json::from_str(&input).unwrap();
        assert_eq!(decoded.workflow_run, event.workflow_run);
        assert!(serde_json::from_str::<WorkflowRunEvent>(&input).is_err());
    }
}
