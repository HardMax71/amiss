use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_controller_github::webhook::run::{CheckRunEvent, RequestedAction};

#[test]
fn check_run_webhooks_keep_the_same_typed_facts_without_metadata() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN;
    let event: CheckRunEvent = serde_json::from_slice(input).unwrap();
    let minimal = serde_json::to_string(&event).unwrap();
    let metadata = minimal.replacen(
        r#""check_run":{"#,
        r#""check_run":{"check_suite":false,"deployment":[],"details_url":{},"completed_at":42,"#,
        1,
    );
    assert_ne!(minimal, metadata);
    assert!(serde_json::from_str::<CheckRunEvent>(&metadata).unwrap() == event);
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
        (r#""output":{"#, r#""output":{"unknown":true,"#, true),
    ] {
        assert!(input.contains(old), "mutation absent: {old}");
        let candidate = input.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<CheckRunEvent>(&candidate).is_ok(),
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
    assert!(serde_json::from_str::<CheckRunEvent<RequestedAction>>(&input).is_ok());
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
            serde_json::from_str::<CheckRunEvent<RequestedAction>>(&candidate).is_ok(),
            valid,
            "{addition}"
        );
        assert!(serde_json::from_str::<CheckRunEvent>(&candidate).is_err());
        let missing = candidate.replacen(r#""action":"requested_action","#, "", 1);
        assert_ne!(missing, candidate);
        assert!(serde_json::from_str::<CheckRunEvent<RequestedAction>>(&missing).is_err());
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
