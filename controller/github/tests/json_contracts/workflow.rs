use std::io::Cursor;

use amiss_controller::decode_bounded_json;
use amiss_controller_github::workflow::{ReferencedWorkflow, WorkflowRunPage, WorkflowRunRecord};
use amiss_wire::assessment::Nullable;
use js_int::UInt;

const RUN: &str = include_str!("../fixtures/workflow-run.json");
const PAGE: &str = include_str!("../fixtures/workflow-runs.json");

#[test]
fn workflow_captures_retain_complete_run_and_query_responses() {
    let run: WorkflowRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    let length = PAGE.len();
    let (page, consumed): (WorkflowRunPage, _) = decode_bounded_json(
        Cursor::new(PAGE.as_bytes()),
        Some(u64::try_from(length).unwrap()),
        length,
        |bytes| amiss_wire::read_json(bytes, u64::MAX),
    )
    .unwrap();
    assert_eq!(consumed, length);
    assert_eq!(page.total_count, 1);
    assert_eq!(page.workflow_runs.len(), 1);
    assert_eq!(page.workflow_runs[0].id, run.id);
    assert_eq!(page.workflow_runs[0].head_sha, run.head_sha);
    assert!(
        page.workflow_runs[0]
            .pull_requests
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(run.pull_requests.as_ref().unwrap()[0].number, 928);
    let commit = run.head_commit.as_ref().unwrap();
    assert_eq!(commit.id, run.head_sha);
    assert_eq!(
        commit.tree_id.as_str(),
        "39ae7e54b6fd6f10aaa829549dc260d7c72742a9"
    );
    assert_eq!(run.actor.as_ref().unwrap().login, "HardMax71");
    for (original, encoded) in [
        (RUN, serde_json::to_vec(&run).unwrap()),
        (PAGE, serde_json::to_vec(&page).unwrap()),
    ] {
        assert_eq!(
            amiss_fixtures::canonical_json(&encoded).unwrap(),
            amiss_fixtures::canonical_json(original.as_bytes()).unwrap(),
        );
    }
    let positional = serde_json::to_vec(&(page.total_count, &page.workflow_runs)).unwrap();
    assert!(serde_json::from_slice::<WorkflowRunPage>(&positional).is_ok());
    assert!(amiss_wire::read_json::<WorkflowRunPage>(&positional, u64::MAX).is_err());
    assert!(amiss_wire::read_json::<WorkflowRunPage>(PAGE.as_bytes(), 0).is_err());
    let trailing = format!("{PAGE} {{}}");
    assert!(amiss_wire::read_json::<WorkflowRunPage>(trailing.as_bytes(), u64::MAX).is_err());
}

#[test]
fn workflow_required_nulls_and_optional_presence_are_distinct() {
    let mut run: WorkflowRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    run.head_branch = None;
    run.status = None;
    run.conclusion = None;
    run.head_commit = None;
    run.pull_requests = None;
    run.actor = None;
    run.triggering_actor = None;
    run.check_suite_id = None;
    run.check_suite_node_id = None;
    run.head_repository_id = None;
    run.run_attempt = None;
    run.run_started_at = None;
    run.previous_attempt_url = Some(Nullable::Null);
    run.referenced_workflows = Some(Nullable::Null);
    for name in [
        None,
        Some(Nullable::Null),
        Some(Nullable::Value("ci".to_owned())),
    ] {
        run.name = name;
        let encoded = serde_json::to_string(&run).unwrap();
        assert!(serde_json::from_str::<WorkflowRunRecord>(&encoded).unwrap() == run);
        assert!(
            amiss_wire::read_json::<WorkflowRunRecord>(encoded.as_bytes(), u64::MAX).unwrap()
                == run
        );
    }
    let encoded = serde_json::to_string(&run).unwrap();
    for field in [
        "head_branch",
        "status",
        "conclusion",
        "head_commit",
        "pull_requests",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        let missing = encoded.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<WorkflowRunRecord>(&missing).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunRecord>(missing.as_bytes(), u64::MAX).is_err());
    }
    for field in [
        "actor",
        "triggering_actor",
        "check_suite_id",
        "check_suite_node_id",
        "head_repository_id",
        "run_attempt",
        "run_started_at",
    ] {
        assert!(!encoded.contains(&format!("\"{field}\":")));
        let null = encoded.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(
            serde_json::from_str::<WorkflowRunRecord>(&null).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunRecord>(null.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn workflow_references_and_nullable_commit_users_keep_their_shapes() {
    let mut run: WorkflowRunRecord = amiss_wire::read_json(RUN.as_bytes(), u64::MAX).unwrap();
    let commit = run.head_commit.as_mut().unwrap();
    commit.author = None;
    commit.committer = None;
    run.head_repository_id = Some(UInt::try_from(run.head_repository.id).unwrap());
    run.referenced_workflows = Some(Nullable::Value(vec![
        ReferencedWorkflow {
            path: "example/workflows/build.yml".to_owned(),
            sha: run.head_sha.clone(),
            reference: None,
        },
        ReferencedWorkflow {
            path: "example/workflows/check.yml".to_owned(),
            sha: run.head_sha.clone(),
            reference: Some("refs/heads/main".to_owned()),
        },
    ]));
    let encoded = serde_json::to_string(&run).unwrap();
    assert!(serde_json::from_str::<WorkflowRunRecord>(&encoded).unwrap() == run);
    assert!(
        amiss_wire::read_json::<WorkflowRunRecord>(encoded.as_bytes(), u64::MAX).unwrap() == run
    );
    for (old, new) in [
        ("\"author\":null,", ""),
        (",\"committer\":null", ""),
        ("\"ref\":\"refs/heads/main\"", "\"ref\":null"),
        (
            "\"path\":\"example/workflows/build.yml\"",
            "\"future\":true,\"path\":\"example/workflows/build.yml\"",
        ),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRunRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn workflow_models_reject_unknown_nested_data_and_invalid_identifiers() {
    for (old, new) in [
        ("\"display_title\":", "\"extra\":true,\"display_title\":"),
        ("\"head_commit\":{", "\"head_commit\":{\"extra\":true,"),
        (
            "\"head_commit\":{\"author\":{",
            "\"head_commit\":{\"author\":{\"extra\":true,",
        ),
        (
            "\"pull_requests\":[{",
            "\"pull_requests\":[{\"extra\":true,",
        ),
        ("\"base\":{\"ref\":", "\"base\":{\"extra\":true,\"ref\":"),
        ("\"repo\":{\"id\":", "\"repo\":{\"extra\":true,\"id\":"),
        ("\"head_sha\":", "\"\\u0068ead_sha\":null,\"head_sha\":"),
        ("\"id\":34409057444", "\"id\":9007199254740992"),
        ("\"id\":34409057444", "\"id\":1.5"),
        ("\"id\":34409057444", "\"id\":-1"),
        ("\"run_attempt\":1", "\"run_attempt\":9007199254740992"),
        ("\"number\":928", "\"number\":9007199254740992"),
        (
            "\"repo\":{\"id\":1298463903",
            "\"repo\":{\"id\":9007199254740992",
        ),
        (
            "\"tree_id\":\"39ae7e54b6fd6f10aaa829549dc260d7c72742a9\"",
            "\"tree_id\":\"bad\"",
        ),
    ] {
        assert!(RUN.contains(old), "{old}");
        let changed = RUN.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRunRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        ("\"total_count\":1", "\"extra\":true,\"total_count\":1"),
        ("\"total_count\":1", "\"total_count\":9007199254740992"),
        ("\"total_count\":1,", ""),
        (
            "\"total_count\":1",
            "\"\\u0074otal_count\":1,\"total_count\":1",
        ),
    ] {
        assert_eq!(PAGE.matches(old).count(), 1);
        let changed = PAGE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRunPage>(&changed).is_err(),
            "{new}"
        );
        assert!(amiss_wire::read_json::<WorkflowRunPage>(changed.as_bytes(), u64::MAX).is_err());
    }
}
