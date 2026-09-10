use amiss_controller_github::webhook::WorkflowRun;
use amiss_controller_github::workflow::ReferencedWorkflow;
use amiss_wire::assessment::Nullable;

const RUN: &str = include_str!("../fixtures/webhook-workflow-run.json");

#[test]
fn webhook_run_preserves_the_complete_published_record() {
    let run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    let encoded = serde_json::to_vec(&run).unwrap();
    assert_eq!(
        amiss_wire::read_json::<WorkflowRun>(RUN.as_bytes(), u64::MAX).unwrap(),
        run
    );
    assert!(
        amiss_fixtures::canonical_json(RUN.as_bytes()).unwrap()
            == amiss_fixtures::canonical_json(&encoded).unwrap(),
        "the typed workflow run must retain every published member"
    );
}

#[test]
fn workflow_run_nulls_keep_required_and_optional_presence_distinct() {
    let mut run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    run.actor = Nullable::Null;
    run.triggering_actor = Nullable::Null;
    run.head_branch = Nullable::Null;
    run.name = Nullable::Null;
    run.previous_attempt_url = Nullable::Null;
    run.conclusion = None;
    run.title.display_title = None;
    run.pull_requests = vec![None];
    for workflows in [
        None,
        Some(Nullable::Null),
        Some(Nullable::Value(vec![ReferencedWorkflow {
            path: "example/workflows/docs.yml".to_owned(),
            sha: run.head_sha.clone(),
            reference: Some("refs/heads/main".to_owned()),
        }])),
    ] {
        run.referenced_workflows = workflows;
        let encoded = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            serde_json::from_slice::<WorkflowRun>(&encoded).unwrap(),
            run
        );
        assert_eq!(
            amiss_wire::read_json::<WorkflowRun>(&encoded, u64::MAX).unwrap(),
            run
        );
    }
    let input = serde_json::to_string(&run).unwrap();
    for field in [
        "actor",
        "triggering_actor",
        "head_branch",
        "name",
        "previous_attempt_url",
        "conclusion",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(input.matches(&member).count(), 1, "{field}");
        let missing = input.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<WorkflowRun>(&missing).is_err(),
            "{field}"
        );
    }
    assert!(!input.contains("\"display_title\":"));
    for (old, new) in [
        ("{", "{\"display_title\":null,"),
        (
            "\"referenced_workflows\":[{",
            "\"referenced_workflows\":[{\"unknown\":true,",
        ),
        ("\"ref\":\"refs/heads/main\"", "\"ref\":null"),
        ("\"head_branch\":null", "\"head_branch\":false"),
        ("\"actor\":null", "\"actor\":false"),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRun>(&changed).is_err(),
            "{new}"
        );
    }
}

#[test]
fn workflow_run_state_tags_keep_their_distinct_provider_contract() {
    let run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    let input = serde_json::to_string(&run).unwrap();
    for conclusion in [
        "action_required",
        "cancelled",
        "failure",
        "neutral",
        "skipped",
        "stale",
        "success",
        "timed_out",
        "startup_failure",
    ] {
        let tag = serde_json::to_string(conclusion).unwrap();
        let changed = input.replacen(
            "\"conclusion\":\"success\"",
            &format!("\"conclusion\":{tag}"),
            1,
        );
        let run: WorkflowRun = serde_json::from_str(&changed).unwrap();
        assert_eq!(serde_json::to_string(&run.conclusion).unwrap(), tag);
    }
    for status in [
        "requested",
        "in_progress",
        "completed",
        "queued",
        "pending",
        "waiting",
    ] {
        let tag = serde_json::to_string(status).unwrap();
        let changed = input.replacen("\"status\":\"completed\"", &format!("\"status\":{tag}"), 1);
        let run: WorkflowRun = serde_json::from_str(&changed).unwrap();
        assert_eq!(serde_json::to_string(&run.status).unwrap(), tag);
    }
    for (field, original, invalids) in [
        (
            "status",
            "completed",
            ["\"future\"", "null", "0", "{\"completed\":null}"],
        ),
        (
            "conclusion",
            "success",
            ["\"future\"", "false", "0", "{\"success\":null}"],
        ),
    ] {
        let old = format!("\"{field}\":\"{original}\"");
        assert_eq!(input.matches(&old).count(), 1);
        for invalid in invalids {
            let changed = input.replacen(&old, &format!("\"{field}\":{invalid}"), 1);
            assert!(
                serde_json::from_str::<WorkflowRun>(&changed).is_err(),
                "{field}: {invalid}"
            );
        }
    }
}

#[test]
fn workflow_run_refuses_unbounded_incomplete_and_ambiguous_inputs() {
    let run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    let input = serde_json::to_string(&run).unwrap();
    let node_member = format!(
        "\"node_id\":{},",
        serde_json::to_string(&run.node_id).unwrap()
    );
    for (old, new) in [
        ("{", "{\"unknown\":true,"),
        ("\"actor\":{", "\"actor\":{\"unknown\":true,"),
        (
            "\"triggering_actor\":{",
            "\"triggering_actor\":{\"unknown\":true,",
        ),
        ("\"id\":289782451", "\"id\":9007199254740992"),
        (
            "\"workflow_id\":2823525",
            "\"workflow_id\":9007199254740992",
        ),
        ("\"run_attempt\":1", "\"run_attempt\":9007199254740992"),
        ("\"run_number\":163", "\"run_number\":-1"),
        ("\"run_number\":163", "\"run_number\":9007199254740992"),
        ("\"check_suite_id\":1291536064", "\"check_suite_id\":1.5"),
        (
            "\"check_suite_id\":1291536064",
            "\"check_suite_id\":9007199254740992",
        ),
        ("\"check_suite_id\":1291536064,", ""),
        ("\"run_number\":163,", ""),
        ("\"status\":\"completed\",", ""),
        ("\"event\":\"repository_dispatch\",", ""),
        (node_member.as_str(), ""),
        ("\"head_sha\":", "\"\\u0068ead_sha\":null,\"head_sha\":"),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRun>(&changed).is_err(),
            "{old}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowRun>(changed.as_bytes(), u64::MAX).is_err(),
            "{old}"
        );
    }
    assert!(amiss_wire::read_json::<WorkflowRun>(RUN.as_bytes(), 0).is_err());
    let trailing = format!("{RUN} {{}}");
    assert!(amiss_wire::read_json::<WorkflowRun>(trailing.as_bytes(), u64::MAX).is_err());
}
