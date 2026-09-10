use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_controller_github::webhook::run::{CheckRunEvent, RequestedAction, WebhookCheckSuite};
use amiss_wire::assessment::Nullable;

#[test]
fn complete_check_runs_retain_every_published_field() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN;
    let event: CheckRunEvent = amiss_wire::read_json(input, u64::MAX).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&event).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input).unwrap()
    );
}

#[test]
fn check_run_profiles_preserve_legacy_absence_and_closed_lifecycle() {
    let event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    let input = serde_json::to_string(&event).unwrap();
    for (old, new, valid) in [
        (r#""action":"completed","#, "", true),
        (r#""action":"completed""#, r#""action":null"#, false),
        (r#""action":"completed""#, r#""action":"created""#, true),
        (r#""action":"completed""#, r#""action":"rerequested""#, true),
        (r#""action":"completed""#, r#""action":"future""#, false),
        (r#""node_id":"MDg6Q2hlY2tSdW4xMjg2MjAyMjg=","#, "", true),
        (
            r#""node_id":"MDg6Q2hlY2tSdW4xMjg2MjAyMjg=""#,
            r#""node_id":null"#,
            false,
        ),
        (r#""status":"completed""#, r#""status":"pending""#, true),
        (r#""status":"completed""#, r#""status":"future""#, false),
        (r#""status":"completed","#, "", false),
        (
            r#""conclusion":"success""#,
            r#""conclusion":"startup_failure""#,
            true,
        ),
        (
            r#""conclusion":"success""#,
            r#""conclusion":"waiting""#,
            true,
        ),
        (
            r#""conclusion":"success""#,
            r#""conclusion":"pending""#,
            true,
        ),
        (
            r#""conclusion":"success""#,
            r#""conclusion":{"success":null}"#,
            false,
        ),
        (r#""conclusion":"success","#, "", false),
        (r#""completed_at":"2019-05-15T15:21:12Z","#, "", false),
        (
            r#""check_suite":{"#,
            r#""check_suite":{"deployment":null,"#,
            false,
        ),
        (r#""output":{"#, r#""output":{"unknown":true,"#, false),
    ] {
        assert!(input.contains(old), "mutation absent: {old}");
        let candidate = input.replacen(old, new, 1);
        assert_eq!(
            amiss_wire::read_json::<CheckRunEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{new}"
        );
    }
}

#[test]
fn check_run_requests_require_the_action_but_not_optional_identifier_metadata() {
    let event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    let input = serde_json::to_string(&event).unwrap().replacen(
        r#""action":"completed""#,
        r#""action":"requested_action""#,
        1,
    );
    assert!(
        amiss_wire::read_json::<CheckRunEvent<RequestedAction>>(input.as_bytes(), u64::MAX).is_ok()
    );
    for (addition, valid) in [
        (r#""requested_action":{},"#, true),
        (r#""requested_action":{"identifier":"lgtm|26764"},"#, true),
        (r#""requested_action":null,"#, false),
        (r#""requested_action":{"identifier":null},"#, false),
        (r#""requested_action":{"unknown":true},"#, false),
        (
            r#""requested_action":{"identifier":"x","\u0069dentifier":"x"},"#,
            false,
        ),
    ] {
        let candidate = input.replacen('{', &format!("{{{addition}"), 1);
        assert_eq!(
            amiss_wire::read_json::<CheckRunEvent<RequestedAction>>(candidate.as_bytes(), u64::MAX)
                .is_ok(),
            valid,
            "{addition}"
        );
        assert!(amiss_wire::read_json::<CheckRunEvent>(candidate.as_bytes(), u64::MAX).is_err());
        let missing = candidate.replacen(r#""action":"requested_action","#, "", 1);
        assert_ne!(missing, candidate);
        assert!(
            amiss_wire::read_json::<CheckRunEvent<RequestedAction>>(missing.as_bytes(), u64::MAX)
                .is_err()
        );
    }
}

#[test]
fn nested_suites_keep_sparse_fields_and_the_existing_minimal_repository() {
    let mut event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    event.check_run.resource.node_id = None;
    event.check_run.resource.details_url = None;
    let mut suite: WebhookCheckSuite = serde_json::from_str("{}").unwrap();
    suite.repository =
        Some(serde_json::from_str(include_str!("../fixtures/workflow-repository.json")).unwrap());
    suite.app = Some(Nullable::Null);
    suite.head_branch = Some(Nullable::Null);
    event.check_run.check_suite = Nullable::Value(suite);
    let input = serde_json::to_vec(&event).unwrap();
    assert!(amiss_wire::read_json::<CheckRunEvent>(&input, u64::MAX).unwrap() == event);
    for input in [
        r#"{"unknown":true}"#,
        r#"{"status":null}"#,
        r#"{"repository":null}"#,
        r#"{"repository":{}}"#,
        r#"{"head_sha":"bad"}"#,
    ] {
        assert!(
            amiss_wire::read_json::<WebhookCheckSuite>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}

#[test]
fn legacy_check_run_repositories_allow_absence_but_not_invalid_availability() {
    let event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    let input = serde_json::to_string(&event).unwrap();
    for (replacement, valid) in [
        ("", true),
        (r#""disabled":null,"#, false),
        (r#""disabled":"false","#, false),
        (r#""disabled":false,"\u0064isabled":false,"#, false),
    ] {
        let candidate = input.replacen(r#""disabled":false,"#, replacement, 1);
        assert_ne!(candidate, input);
        assert_eq!(
            serde_json::from_str::<CheckRunEvent>(&candidate).is_ok(),
            valid,
            "{replacement}"
        );
        assert_eq!(
            amiss_wire::read_json::<CheckRunEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{replacement}"
        );
        let repository = serde_json::to_string(&event.repository).unwrap().replacen(
            r#""disabled":false,"#,
            replacement,
            1,
        );
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(repository.as_bytes(), u64::MAX).is_err()
        );
    }
}
