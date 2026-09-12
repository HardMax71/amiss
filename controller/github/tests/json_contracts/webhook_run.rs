use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::webhook::WorkflowRun;

const RUN: &str = include_str!("../fixtures/webhook-workflow-run.json");

#[test]
fn webhook_run_keeps_completion_facts_without_unused_metadata() {
    let (run, consumed): (WorkflowRun, _) =
        decode_bounded_json(RUN.as_bytes(), None, RUN.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(consumed, RUN.len());
    assert_eq!(run.id, 289_782_451);
    assert_eq!(run.workflow_id, 2_823_525);
    assert_eq!(run.run_attempt, 1);
    assert_eq!(run.repository.owner.as_ref().unwrap().login, "octo-org");
    assert_eq!(
        run.head_repository.owner.as_ref().unwrap().login,
        "octo-org"
    );
    let encoded = serde_json::to_string(&run).unwrap();
    for metadata in [
        "",
        r#""head_commit":false,"actor":[],"triggering_actor":null,"display_title":{},"#,
        r#""run_number":-1,"check_suite_id":null,"referenced_workflows":false,"unknown":[],"#,
        r#""name":null,"path":false,"head_branch":[],"artifacts_url":{},"#,
    ] {
        let input = encoded.replacen('{', &format!("{{{metadata}"), 1);
        assert_eq!(serde_json::from_str::<WorkflowRun>(&input).unwrap(), run);
    }
    assert_eq!(
        decode_bounded_json::<WorkflowRun, _>(RUN.as_bytes(), None, RUN.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
    assert!(serde_json::from_str::<WorkflowRun>(&format!("{RUN} {{}}")).is_err());
}

#[test]
fn workflow_conclusion_can_be_null_but_not_missing() {
    let mut run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    run.conclusion = None;
    run.pull_requests = vec![None];
    let input = serde_json::to_string(&run).unwrap();
    assert_eq!(serde_json::from_str::<WorkflowRun>(&input).unwrap(), run);
    amiss_fixtures::assert_json_rejections::<WorkflowRun>(&input, &[(r#""conclusion":null,"#, "")]);
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
fn workflow_run_requires_typed_unique_completion_fields() {
    let run: WorkflowRun = serde_json::from_str(RUN).unwrap();
    let input = serde_json::to_string(&run).unwrap();
    for field in [
        "id",
        "event",
        "status",
        "conclusion",
        "workflow_id",
        "run_attempt",
        "head_sha",
        "repository",
        "head_repository",
        "pull_requests",
    ] {
        let original = format!(r#""{field}":"#);
        for replacement in [
            format!(r#""missing_{field}":"#),
            format!(r#""{field}":null,"{field}":"#),
        ] {
            amiss_fixtures::assert_json_rejections::<WorkflowRun>(
                &input,
                &[(&original, &replacement)],
            );
        }
    }
    for (field, value) in [
        ("id", run.id),
        ("workflow_id", run.workflow_id),
        ("run_attempt", run.run_attempt),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for invalid in ["null", "false", "-1", "1.5", "9007199254740992"] {
            amiss_fixtures::assert_json_rejections::<WorkflowRun>(
                &input,
                &[(&original, &format!(r#""{field}":{invalid}"#))],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<WorkflowRun>(
        &input,
        &[(r#""head_sha":"#, r#""\u0068ead_sha":null,"head_sha":"#)],
    );
}
