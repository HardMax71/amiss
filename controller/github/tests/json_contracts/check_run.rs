use amiss_controller::decode_bounded_json;
use amiss_controller_github::check::{
    CheckRunConclusion, CheckRunDeployment, CheckRunPage, CheckRunRecord, CheckRunStatus,
};
use amiss_controller_github::workflow::WorkflowRunRecord;
use amiss_wire::assessment::Nullable;
use js_int::UInt;

const RUN: &str = include_str!("../fixtures/check-run.json");
const PAGE: &str = include_str!("../fixtures/check-runs.json");

#[test]
fn check_run_captures_retain_the_complete_individual_and_page_responses() {
    let run: CheckRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    let (page, consumed): (CheckRunPage, _) = decode_bounded_json(
        PAGE.as_bytes(),
        Some(u64::try_from(PAGE.len()).unwrap()),
        PAGE.len(),
        |bytes| amiss_wire::read_json(bytes, u64::MAX),
    )
    .unwrap();
    assert_eq!(consumed, PAGE.len());
    assert_eq!(page.total_count, 1);
    assert_eq!(page.check_runs.as_slice(), std::slice::from_ref(&run));
    assert_eq!(run.id, 102_692_011_089);
    assert_eq!(
        run.head_sha.as_str(),
        "07d0a117c2345635f6ffc0ae9fa2491448c1201f"
    );
    assert_eq!(run.status, CheckRunStatus::Completed);
    assert_eq!(run.conclusion, Some(CheckRunConclusion::Success));
    assert_eq!(run.app.as_ref().unwrap().id, 15368);
    for (input, encoded) in [
        (RUN, serde_json::to_vec(&run).unwrap()),
        (PAGE, serde_json::to_vec(&page).unwrap()),
    ] {
        assert_eq!(
            amiss_fixtures::canonical_json(&encoded).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
        );
    }
    let positional = serde_json::to_vec(&(page.total_count, &page.check_runs)).unwrap();
    assert!(serde_json::from_slice::<CheckRunPage>(&positional).is_ok());
    assert!(amiss_wire::read_json::<CheckRunPage>(&positional, u64::MAX).is_err());
    assert!(amiss_wire::read_json::<CheckRunPage>(PAGE.as_bytes(), 0).is_err());
    let trailing = format!("{PAGE} {{}}");
    assert!(amiss_wire::read_json::<CheckRunPage>(trailing.as_bytes(), u64::MAX).is_err());
}

#[test]
fn nullable_check_run_metadata_is_required_not_silently_defaulted() {
    let mut run: CheckRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    run.external_id = None;
    run.conclusion = None;
    run.app = None;
    run.html_url = Nullable::Null;
    run.resource.details_url = Nullable::Null;
    run.started_at = Nullable::Null;
    run.completed_at = Nullable::Null;
    run.check_suite = Nullable::Null;
    let encoded = serde_json::to_string(&run).unwrap();
    assert_eq!(
        serde_json::from_str::<CheckRunRecord>(&encoded).unwrap(),
        run
    );
    assert_eq!(
        amiss_wire::read_json::<CheckRunRecord>(encoded.as_bytes(), u64::MAX).unwrap(),
        run
    );
    for field in [
        "external_id",
        "conclusion",
        "app",
        "html_url",
        "details_url",
        "started_at",
        "completed_at",
        "check_suite",
        "title",
        "summary",
        "text",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        let missing = encoded.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<CheckRunRecord>(&missing).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<CheckRunRecord>(missing.as_bytes(), u64::MAX).is_err());
    }
    assert!(!encoded.contains("\"deployment\":"));
    let null = encoded.replacen('{', "{\"deployment\":null,", 1);
    assert!(serde_json::from_str::<CheckRunRecord>(&null).is_err());
    assert!(amiss_wire::read_json::<CheckRunRecord>(null.as_bytes(), u64::MAX).is_err());
}

#[test]
fn check_run_states_are_exact_strings_including_server_assigned_stale() {
    for (status, text) in [
        (CheckRunStatus::Queued, "queued"),
        (CheckRunStatus::InProgress, "in_progress"),
        (CheckRunStatus::Completed, "completed"),
        (CheckRunStatus::Waiting, "waiting"),
        (CheckRunStatus::Requested, "requested"),
        (CheckRunStatus::Pending, "pending"),
    ] {
        let encoded = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&status).unwrap(), encoded);
        assert_eq!(
            serde_json::from_str::<CheckRunStatus>(&encoded).unwrap(),
            status
        );
        assert_eq!(
            amiss_wire::read_json::<CheckRunStatus>(encoded.as_bytes(), u64::MAX).unwrap(),
            status
        );
    }
    for (conclusion, text) in [
        (CheckRunConclusion::ActionRequired, "action_required"),
        (CheckRunConclusion::Cancelled, "cancelled"),
        (CheckRunConclusion::Failure, "failure"),
        (CheckRunConclusion::Neutral, "neutral"),
        (CheckRunConclusion::Success, "success"),
        (CheckRunConclusion::Skipped, "skipped"),
        (CheckRunConclusion::Stale, "stale"),
        (CheckRunConclusion::TimedOut, "timed_out"),
    ] {
        let encoded = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&conclusion).unwrap(), encoded);
        assert_eq!(
            serde_json::from_str::<CheckRunConclusion>(&encoded).unwrap(),
            conclusion
        );
        assert_eq!(
            amiss_wire::read_json::<CheckRunConclusion>(encoded.as_bytes(), u64::MAX).unwrap(),
            conclusion
        );
    }
    for invalid in [
        "null",
        "1",
        "[]",
        r#"{"completed":null}"#,
        r#""future""#,
        r#""startup_failure""#,
    ] {
        assert!(
            serde_json::from_str::<CheckRunStatus>(invalid).is_err(),
            "{invalid}"
        );
        assert!(
            serde_json::from_str::<CheckRunConclusion>(invalid).is_err(),
            "{invalid}"
        );
    }
    assert!(serde_json::from_str::<CheckRunConclusion>(r#"{"success":null}"#).is_err());
    let mut run: CheckRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    run.conclusion = Some(CheckRunConclusion::Stale);
    let encoded = serde_json::to_vec(&run).unwrap();
    assert_eq!(
        amiss_wire::read_json::<CheckRunRecord>(&encoded, u64::MAX).unwrap(),
        run
    );
}

#[test]
fn deployment_metadata_preserves_presence_and_reuses_existing_records() {
    let mut run: CheckRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    let workflow: WorkflowRunRecord =
        amiss_wire::read_json(include_bytes!("../fixtures/workflow-run.json"), u64::MAX).unwrap();
    run.pull_requests = workflow.pull_requests.unwrap();
    run.head_sha = run.pull_requests[0].head.sha.clone();
    let app = run.app.clone().unwrap();
    let mut deployment = CheckRunDeployment {
        id: UInt::from(42_u8),
        node_id: "deployment-node".to_owned(),
        task: "deploy".to_owned(),
        environment: "staging".to_owned(),
        description: Nullable::Null,
        statuses_url: "https://api.github.com/repos/example/repo/deployments/42/statuses"
            .to_owned(),
        repository_url: "https://api.github.com/repos/example/repo".to_owned(),
        url: "https://api.github.com/repos/example/repo/deployments/42".to_owned(),
        created_at: "2026-09-10T00:00:00Z".to_owned(),
        updated_at: "2026-09-10T00:00:00Z".to_owned(),
        original_environment: Some("development".to_owned()),
        transient_environment: Some(true),
        production_environment: Some(false),
        performed_via_github_app: None,
    };
    for performing_app in [None, Some(Nullable::Null), Some(Nullable::Value(app))] {
        deployment.performed_via_github_app = performing_app;
        run.deployment = Some(deployment.clone());
        let encoded = serde_json::to_vec(&run).unwrap();
        assert_eq!(
            serde_json::from_slice::<CheckRunRecord>(&encoded).unwrap(),
            run
        );
        assert_eq!(
            amiss_wire::read_json::<CheckRunRecord>(&encoded, u64::MAX).unwrap(),
            run
        );
    }
    deployment.original_environment = None;
    deployment.transient_environment = None;
    deployment.production_environment = None;
    run.deployment = Some(deployment);
    let encoded = serde_json::to_string(&run).unwrap();
    assert_eq!(
        serde_json::from_str::<CheckRunRecord>(&encoded).unwrap(),
        run
    );
    assert_eq!(
        amiss_wire::read_json::<CheckRunRecord>(encoded.as_bytes(), u64::MAX).unwrap(),
        run
    );
    for field in [
        "original_environment",
        "transient_environment",
        "production_environment",
    ] {
        assert!(!encoded.contains(&format!("\"{field}\":")));
        let null = encoded.replacen(
            "\"deployment\":{",
            &format!("\"deployment\":{{\"{field}\":null,"),
            1,
        );
        assert!(
            serde_json::from_str::<CheckRunRecord>(&null).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<CheckRunRecord>(null.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        ("\"deployment\":{", "\"deployment\":{\"extra\":true,"),
        ("\"description\":null,", ""),
        ("\"id\":42", "\"id\":9007199254740992"),
        (
            "\"performed_via_github_app\":{",
            "\"performed_via_github_app\":{\"extra\":true,",
        ),
        (
            "\"pull_requests\":[{",
            "\"pull_requests\":[{\"extra\":true,",
        ),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn check_run_models_refuse_unknown_data_and_invalid_identifiers() {
    for (old, new) in [
        ("\"output\":{", "\"output\":{\"extra\":true,"),
        ("\"check_suite\":{", "\"check_suite\":{\"extra\":true,"),
        ("\"head_sha\":", "\"extra\":true,\"head_sha\":"),
        ("\"head_sha\":", "\"\\u0068ead_sha\":null,\"head_sha\":"),
        (
            "\"head_sha\":\"07d0a117c2345635f6ffc0ae9fa2491448c1201f\"",
            "\"head_sha\":\"bad\"",
        ),
        ("\"id\":102692011089", "\"id\":9007199254740992"),
        ("\"id\":102692011089", "\"id\":1.5"),
        ("\"id\":102692011089", "\"id\":-1"),
        ("\"id\":93244620619", "\"id\":9007199254740992"),
        (
            "\"annotations_count\":0",
            "\"annotations_count\":9007199254740992",
        ),
        ("\"annotations_count\":0,", ""),
        ("\"status\":\"completed\"", "\"status\":null"),
        ("\"status\":\"completed\"", "\"status\":\"future\""),
        (
            "\"conclusion\":\"success\"",
            "\"conclusion\":\"startup_failure\"",
        ),
    ] {
        assert_eq!(RUN.matches(old).count(), 1, "{old}");
        let changed = RUN.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        ("\"total_count\":1", "\"extra\":true,\"total_count\":1"),
        ("\"total_count\":1", "\"total_count\":9007199254740992"),
        (",\"total_count\":1", ""),
        (
            "\"total_count\":1",
            "\"\\u0074otal_count\":1,\"total_count\":1",
        ),
    ] {
        assert_eq!(PAGE.matches(old).count(), 1, "{old}");
        let changed = PAGE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunPage>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunPage>(changed.as_bytes(), u64::MAX).is_err());
    }
}
