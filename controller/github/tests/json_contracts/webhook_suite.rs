use amiss_controller_github::installation::permissions::AppPermissions;
use amiss_controller_github::webhook::app::{WebhookApp, WebhookAppPermissions};
use amiss_controller_github::webhook::suite::{CheckSuiteAction, CheckSuiteEvent};
use amiss_wire::assessment::Nullable;

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
            ("{", r#"{"unknown":true,"#, false),
            ("{", r#"{"installation":null,"#, false),
            ("{", r#"{"organization":null,"#, false),
            ("{", r#"{"enterprise":null,"#, false),
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
                amiss_wire::read_json::<CheckSuiteEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
                valid,
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn webhook_apps_keep_required_nulls_and_optional_metadata() {
    let event: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    let mut app = event.check_suite.app;
    app.id = Nullable::Null;
    app.owner = Nullable::Null;
    app.external_url = Nullable::Null;
    app.created_at = Nullable::Null;
    app.updated_at = Nullable::Null;
    app.permissions = None;
    app.events = None;
    let input = serde_json::to_string(&app).unwrap();
    assert_eq!(
        amiss_wire::read_json::<WebhookApp>(input.as_bytes(), u64::MAX).unwrap(),
        app
    );
    for member in [
        r#""id":null,"#,
        r#""owner":null,"#,
        r#""external_url":null,"#,
        r#""created_at":null,"#,
        r#","updated_at":null"#,
    ] {
        assert!(input.contains(member));
        assert!(
            amiss_wire::read_json::<WebhookApp>(input.replacen(member, "", 1).as_bytes(), u64::MAX)
                .is_err(),
            "{member}"
        );
    }
    for (addition, valid) in [
        (r#""unknown":true,"#, false),
        (r#""permissions":null,"#, false),
        (r#""permissions":{},"#, true),
        (r#""events":null,"#, false),
        (r#""events":[],"#, true),
        (
            r#""events":["workflow_run","projects_v2_item","repository_import"],"#,
            true,
        ),
        (r#""events":["future_event"],"#, false),
        (r#""events":[{"workflow_run":null}],"#, false),
        (r#""client_id":null,"#, true),
        (r#""client_id":"app-client","#, true),
        (r#""slug":null,"#, false),
        (r#""slug":"app","#, true),
    ] {
        let candidate = input.replacen('{', &format!("{{{addition}"), 1);
        assert_eq!(
            amiss_wire::read_json::<WebhookApp>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{addition}"
        );
    }
}

#[test]
fn webhook_permission_profiles_reuse_levels_without_widening_installation_tokens() {
    for input in [
        r#"{"workflows":"read"}"#,
        r#"{"organization_plan":"write"}"#,
        r#"{"team_discussions":"write"}"#,
    ] {
        assert!(
            amiss_wire::read_json::<WebhookAppPermissions>(input.as_bytes(), u64::MAX).is_ok(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<AppPermissions>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
    for (input, valid) in [
        (
            r#"{"organization_projects":"admin","repository_projects":"admin"}"#,
            true,
        ),
        (
            r#"{"copilot_requests":"write","content_references":"read","drives":"write","emails":"read","keys":"write","models":"read","security_scanning_alert":"write"}"#,
            true,
        ),
        (r#"{"copilot_requests":"read"}"#, false),
        (r#"{"team_discussions":"admin"}"#, false),
        (r#"{"checks":"admin"}"#, false),
        (r#"{"checks":null}"#, false),
        (r#"{"unknown":"read"}"#, false),
        (r#"{"checks":"write","\u0063hecks":"write"}"#, false),
        (
            r#"{"team_discussions":"write","\u0074eam_discussions":"write"}"#,
            false,
        ),
    ] {
        assert_eq!(
            amiss_wire::read_json::<WebhookAppPermissions>(input.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{input}"
        );
    }
}
