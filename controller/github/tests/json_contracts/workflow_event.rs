use amiss_controller_github::webhook::WorkflowRun;
use amiss_controller_github::webhook::workflow::{
    RequestedWorkflowRunAction, RequestedWorkflowRunTitle, WorkflowRunEvent,
};

#[test]
fn workflow_events_preserve_every_published_root_and_nested_member() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN;
    let event: WorkflowRunEvent = amiss_wire::read_json(input, u64::MAX).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&event).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input).unwrap()
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
        (r#""sender":{"#, r#""sender":{"unknown":true,"#),
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
fn requested_workflow_runs_require_their_own_display_title() {
    let mut event: WorkflowRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_WORKFLOW_RUN).unwrap();
    event.workflow_run.title.display_title = None;
    let completed = serde_json::to_string(&event).unwrap();
    assert!(amiss_wire::read_json::<WorkflowRunEvent>(completed.as_bytes(), u64::MAX).is_ok());
    let requested = completed.replacen(r#""action":"completed""#, r#""action":"requested""#, 1);
    for (addition, valid) in [
        ("", false),
        (r#""display_title":null,"#, false),
        (r#""display_title":false,"#, false),
        (r#""display_title":"Workflow title","#, true),
        (r#""display_title":"x","\u0064isplay_title":"x","#, false),
    ] {
        let input = requested.replacen(
            r#""workflow_run":{"#,
            &format!(r#""workflow_run":{{{addition}"#),
            1,
        );
        assert_eq!(
            serde_json::from_str::<
                WorkflowRunEvent<
                    WorkflowRun<RequestedWorkflowRunTitle>,
                    RequestedWorkflowRunAction,
                >,
            >(&input)
            .is_ok(),
            valid,
            "{addition}"
        );
        assert_eq!(
            amiss_wire::read_json::<
                WorkflowRunEvent<
                    WorkflowRun<RequestedWorkflowRunTitle>,
                    RequestedWorkflowRunAction,
                >,
            >(input.as_bytes(), u64::MAX)
            .is_ok(),
            valid
        );
    }
}
