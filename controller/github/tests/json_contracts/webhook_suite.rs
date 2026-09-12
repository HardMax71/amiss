use amiss_controller_github::webhook::app::WebhookApp;
use amiss_controller_github::webhook::suite::{CheckSuiteAction, CheckSuiteEvent};

#[test]
fn check_suite_captures_keep_their_completed_action() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE;
    let event: CheckSuiteEvent = serde_json::from_slice(input).unwrap();
    assert_eq!(event.action, CheckSuiteAction::Completed);
    assert!(
        serde_json::from_slice::<CheckSuiteEvent>(&serde_json::to_vec(&event).unwrap()).unwrap()
            == event
    );
}

#[test]
fn check_suite_actions_share_the_closed_suite_lifecycle() {
    let mut event: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    for action in [
        CheckSuiteAction::Completed,
        CheckSuiteAction::Requested,
        CheckSuiteAction::Rerequested,
    ] {
        event.action = action;
        let input = serde_json::to_string(&event).unwrap();
        for (old, new, valid) in [
            ("{", r#"{"unknown":true,"#, true),
            ("{", r#"{"installation":null,"#, false),
            ("{", r#"{"organization":null,"#, true),
            ("{", r#"{"enterprise":null,"#, true),
            (
                r#""check_suite":{"#,
                r#""check_suite":{"unknown":true,"#,
                false,
            ),
            (r#""status":"completed""#, r#""status":"waiting""#, true),
            (r#""status":"completed""#, r#""status":null"#, true),
            (r#""status":"completed""#, r#""status":"future""#, false),
            (r#""status":"completed","#, "", false),
            (
                r#""conclusion":"success""#,
                r#""conclusion":"startup_failure""#,
                true,
            ),
            (r#""conclusion":"success""#, r#""conclusion":null"#, true),
            (
                r#""conclusion":"success""#,
                r#""conclusion":"future""#,
                false,
            ),
            (r#""conclusion":"success","#, "", false),
            (r#""head_branch":"changes""#, r#""head_branch":null"#, true),
            (r#""head_branch":"changes","#, "", false),
            (
                r#""before":"6113728f27ae82c7b1a177c8d03f9e96e0adf246""#,
                r#""before":null"#,
                true,
            ),
            (
                r#""before":"6113728f27ae82c7b1a177c8d03f9e96e0adf246""#,
                r#""before":"not-a-sha""#,
                false,
            ),
            (
                r#""latest_check_runs_count":1"#,
                r#""latest_check_runs_count":9007199254740992"#,
                false,
            ),
            (r#""pull_requests":["#, r#""pull_requests":[null,"#, false),
            (
                r#""head_commit":{"#,
                r#""head_commit":{"unknown":true,"#,
                false,
            ),
            (
                r#""check_suite":{"#,
                r#""check_suite":{"rerequestable":true,"runs_rerequestable":false,"#,
                true,
            ),
            (
                r#""check_suite":{"#,
                r#""check_suite":{"rerequestable":null,"#,
                false,
            ),
        ] {
            assert!(input.contains(old), "mutation absent: {old}");
            let candidate = input.replacen(old, new, 1);
            assert_eq!(
                serde_json::from_str::<CheckSuiteEvent>(&candidate).is_ok(),
                valid,
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn webhook_app_ids_are_required_nullable_and_bounded() {
    for (input, expected) in [
        (r#"{"id":null}"#, None),
        (r#"{"id":0}"#, Some(0)),
        (r#"{"id":9007199254740991}"#, Some(9_007_199_254_740_991)),
    ] {
        let app: WebhookApp = serde_json::from_str(input).unwrap();
        assert_eq!(app.id.map(u64::from), expected);
        assert_eq!(serde_json::to_string(&app).unwrap(), input);
        let metadata = input.replacen(
            '{',
            r#"{"owner":false,"events":["future"],"permissions":{"checks":"admin"},"node_id":null,"name":[],"description":0,"external_url":{},"html_url":null,"created_at":false,"updated_at":1.5,"slug":null,"client_id":false,"installations_count":-1,"future":{},"#,
            1,
        );
        assert_ne!(metadata, input);
        assert_eq!(serde_json::from_str::<WebhookApp>(&metadata).unwrap(), app);
    }
    for input in [
        "{}",
        r#"{"id":false}"#,
        r#"{"id":"1"}"#,
        r#"{"id":{}}"#,
        r#"{"id":[]}"#,
        r#"{"id":-1}"#,
        r#"{"id":9007199254740992}"#,
        r#"{"id":1.5}"#,
        r#"{"id":0,"id":0}"#,
        r#"{"id":null,"\u0069d":null}"#,
        r#"{"id":null} {}"#,
    ] {
        assert!(
            serde_json::from_str::<WebhookApp>(input).is_err(),
            "{input}"
        );
    }
}
