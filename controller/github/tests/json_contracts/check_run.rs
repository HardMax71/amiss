use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::check::{
    CheckRunConclusion, CheckRunPage, CheckRunRecord, CheckRunStatus,
};

const RUN: &str = include_str!("../fixtures/check-run.json");
const PAGE: &str = include_str!("../fixtures/check-runs.json");

#[test]
fn check_run_captures_retain_publication_facts_and_ignore_metadata() {
    let run: CheckRunRecord = serde_json::from_str(RUN).unwrap();
    let (page, consumed): (CheckRunPage, _) = decode_bounded_json(
        PAGE.as_bytes(),
        Some(u64::try_from(PAGE.len()).unwrap()),
        PAGE.len(),
        |bytes| serde_json::from_slice(bytes),
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
    let encoded = serde_json::to_string(&run).unwrap();
    let metadata = encoded
        .replacen('{', r#"{"check_suite":false,"deployment":[],"pull_requests":{},"node_id":42,"details_url":null,"extra":{},"#, 1)
        .replacen(r#""output":{"#, r#""output":{"text":false,"annotations_count":-1,"annotations_url":[],"#, 1);
    assert_eq!(
        serde_json::from_str::<CheckRunRecord>(&metadata).unwrap(),
        run
    );
    let positional = serde_json::to_vec(&(page.total_count, &page.check_runs)).unwrap();
    assert_eq!(
        serde_json::from_slice::<CheckRunPage>(&positional).unwrap(),
        page
    );
    assert_eq!(
        decode_bounded_json::<CheckRunPage, _>(PAGE.as_bytes(), None, PAGE.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        },),
        Err(ProviderError::InvalidResponse)
    );
    assert!(serde_json::from_str::<CheckRunPage>(&format!("{PAGE} {{}}")).is_err());
}

#[test]
fn check_run_identity_and_nullable_feedback_remain_required() {
    let mut run: CheckRunRecord = serde_json::from_str(RUN).unwrap();
    run.external_id = None;
    run.conclusion = None;
    run.app = None;
    run.output.title = None;
    run.output.summary = None;
    let encoded = serde_json::to_string(&run).unwrap();
    assert_eq!(
        serde_json::from_str::<CheckRunRecord>(&encoded).unwrap(),
        run
    );
    for (field, value) in [
        ("id", run.id.to_string()),
        ("name", serde_json::to_string(&run.name).unwrap()),
        ("head_sha", serde_json::to_string(&run.head_sha).unwrap()),
        ("status", serde_json::to_string(&run.status).unwrap()),
        ("output", serde_json::to_string(&run.output).unwrap()),
        ("external_id", "null".to_owned()),
        ("conclusion", "null".to_owned()),
        ("app", "null".to_owned()),
        ("title", "null".to_owned()),
        ("summary", "null".to_owned()),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            amiss_fixtures::assert_json_rejections::<CheckRunRecord>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
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
    let mut run: CheckRunRecord = serde_json::from_str(RUN).unwrap();
    run.conclusion = Some(CheckRunConclusion::Stale);
    let encoded = serde_json::to_vec(&run).unwrap();
    assert_eq!(
        serde_json::from_slice::<CheckRunRecord>(&encoded).unwrap(),
        run
    );
}

#[test]
fn check_run_models_reject_invalid_consumed_fields() {
    amiss_fixtures::assert_json_rejections::<CheckRunRecord>(
        RUN,
        &[
            (r#""head_sha":"#, r#""\u0068ead_sha":null,"head_sha":"#),
            (
                r#""head_sha":"07d0a117c2345635f6ffc0ae9fa2491448c1201f""#,
                r#""head_sha":"bad""#,
            ),
            (r#""id":102692011089"#, r#""id":9007199254740992"#),
            (r#""id":102692011089"#, r#""id":1.5"#),
            (r#""id":102692011089"#, r#""id":-1"#),
            (r#""status":"completed""#, r#""status":null"#),
            (r#""status":"completed""#, r#""status":"future""#),
            (
                r#""conclusion":"success""#,
                r#""conclusion":"startup_failure""#,
            ),
            (r#""title":null"#, r#""title":false"#),
            (r#""summary":null"#, r#""summary":42"#),
        ],
    );
    amiss_fixtures::assert_json_rejections::<CheckRunPage>(
        PAGE,
        &[
            (r#""total_count":1"#, r#""total_count":9007199254740992"#),
            (r#""total_count":1"#, r#""missing_total_count":1"#),
            (r#""check_runs":"#, r#""missing_check_runs":"#),
            (
                r#""total_count":1"#,
                r#""\u0074otal_count":1,"total_count":1"#,
            ),
        ],
    );
    let metadata = PAGE.replacen('{', r#"{"extra":false,"#, 1);
    assert_eq!(
        serde_json::from_str::<CheckRunPage>(&metadata).unwrap(),
        serde_json::from_str::<CheckRunPage>(PAGE).unwrap()
    );
}
