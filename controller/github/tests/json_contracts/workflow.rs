use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::workflow::{WorkflowRunPage, WorkflowRunRecord};

const RUN: &str = include_str!("../fixtures/workflow-run.json");
const PAGE: &str = include_str!("../fixtures/workflow-runs.json");

#[test]
fn workflow_captures_retain_artifact_selection_facts() {
    let run: WorkflowRunRecord = serde_json::from_str(RUN).unwrap();
    let (page, consumed): (WorkflowRunPage, _) = decode_bounded_json(
        PAGE.as_bytes(),
        Some(u64::try_from(PAGE.len()).unwrap()),
        PAGE.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, PAGE.len());
    assert_eq!(page.total_count, 1);
    assert_eq!(page.workflow_runs.len(), 1);
    assert_eq!(page.workflow_runs[0].id, run.id);
    assert_eq!(page.workflow_runs[0].head_sha, run.head_sha);
    assert_eq!(run.id, 34_409_057_444);
    assert_eq!(run.workflow_id, 313_127_792);
    assert_eq!(run.run_attempt.map(u64::from), Some(1));
    assert_eq!(run.event, "pull_request");
    assert_eq!(run.status.as_deref(), Some("completed"));
    assert_eq!(run.conclusion.as_deref(), Some("success"));
    assert_eq!(run.repository.id, 1_298_463_903);
    assert_eq!(run.head_repository.id, run.repository.id);
    assert_eq!(run.repository.full_name, "HardMax71/amiss");

    let encoded = serde_json::to_string(&page).unwrap();
    let metadata = encoded
        .replacen('{', r#"{"extra":true,"#, 1)
        .replacen(r#""workflow_runs":[{"#, r#""workflow_runs":[{"head_commit":false,"pull_requests":{},"actor":[],"display_title":42,"referenced_workflows":null,"#, 1)
        .replacen(r#""repository":{"#, r#""repository":{"private":null,"permissions":false,"#, 1)
        .replacen(r#""head_repository":{"#, r#""head_repository":{"security_and_analysis":[],"#, 1);
    assert!(serde_json::from_str::<WorkflowRunPage>(&metadata).unwrap() == page);
    let positional = serde_json::to_vec(&(page.total_count, &page.workflow_runs)).unwrap();
    assert!(serde_json::from_slice::<WorkflowRunPage>(&positional).unwrap() == page);
    assert!(matches!(
        decode_bounded_json::<WorkflowRunPage, _>(PAGE.as_bytes(), None, PAGE.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    ));
    assert!(serde_json::from_str::<WorkflowRunPage>(&format!("{PAGE} {{}}")).is_err());
}

#[test]
fn workflow_identity_and_nullable_outcome_remain_required() {
    let mut run: WorkflowRunRecord = serde_json::from_str(RUN).unwrap();
    run.status = None;
    run.conclusion = None;
    run.run_attempt = None;
    let encoded = serde_json::to_string(&run).unwrap();
    assert!(serde_json::from_str::<WorkflowRunRecord>(&encoded).unwrap() == run);
    assert!(!encoded.contains(r#""run_attempt":"#));
    for (field, value) in [
        ("id", run.id.to_string()),
        ("workflow_id", run.workflow_id.to_string()),
        ("head_sha", serde_json::to_string(&run.head_sha).unwrap()),
        ("event", serde_json::to_string(&run.event).unwrap()),
        (
            "repository",
            serde_json::to_string(&run.repository).unwrap(),
        ),
        (
            "head_repository",
            serde_json::to_string(&run.head_repository).unwrap(),
        ),
        ("status", "null".to_owned()),
        ("conclusion", "null".to_owned()),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":false"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            amiss_fixtures::assert_json_rejections::<WorkflowRunRecord>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<WorkflowRunRecord>(
        &encoded,
        &[("{", r#"{"run_attempt":null,"#)],
    );
}

#[test]
fn workflow_consumed_identifiers_and_page_counts_are_exact() {
    let run: WorkflowRunRecord = serde_json::from_str(RUN).unwrap();
    let encoded = serde_json::to_string(&run).unwrap();
    for (field, value) in [
        ("id", run.id),
        ("workflow_id", run.workflow_id),
        ("run_attempt", u64::from(run.run_attempt.unwrap())),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for invalid in ["9007199254740992", "-1", "1.5", "null", "true", r#""1""#] {
            amiss_fixtures::assert_json_rejections::<WorkflowRunRecord>(
                &encoded,
                &[(&original, &format!(r#""{field}":{invalid}"#))],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<WorkflowRunRecord>(
        &encoded,
        &[
            (r#""head_sha":"#, r#""\u0068ead_sha":null,"head_sha":"#),
            (
                r#""head_sha":"316badb35996a3ff460b1e2d4b8460f92571c438""#,
                r#""head_sha":"bad""#,
            ),
        ],
    );
    amiss_fixtures::assert_json_rejections::<WorkflowRunPage>(
        PAGE,
        &[
            (r#""total_count":1"#, r#""total_count":9007199254740992"#),
            (r#""total_count":1"#, r#""missing_total_count":1"#),
            (
                r#""total_count":1"#,
                r#""\u0074otal_count":1,"total_count":1"#,
            ),
            (r#""workflow_runs":"#, r#""missing_workflow_runs":"#),
            (
                r#""workflow_runs":"#,
                r#""workflow_runs":null,"workflow_runs":"#,
            ),
        ],
    );
}
