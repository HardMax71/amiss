use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::suite::{CheckSuiteAction, CheckSuiteEvent};
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn signed_check_suites_keep_metadata_and_remain_no_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut event: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    for action in [
        CheckSuiteAction::Completed,
        CheckSuiteAction::Requested,
        CheckSuiteAction::Rerequested,
    ] {
        event.action = action;
        let input = serde_json::to_vec(&event).unwrap();
        assert!(matches!(
            serde_json::from_slice::<GitHubEvent>(&input).unwrap(),
            GitHubEvent::CheckSuite(_)
        ));
        assert_eq!(authenticate_target(&source, &input, &target), Ok(None));
        let metadata = replaced_once(&input, r#""head":{"#, r#""head":{"unknown":true,"#);
        assert_eq!(authenticate_target(&source, &metadata, &target), Ok(None));
        for (old, new) in [
            ("{", r#"{"unknown":true,"#),
            (r#""check_suite":{"#, r#""check_suite":{"unknown":true,"#),
            (r#""app":{"#, r#""app":{"unknown":true,"#),
            (r#""permissions":{"#, r#""permissions":{"unknown":"read","#),
            (
                r#""team_discussions":"write""#,
                r#""team_discussions":"admin""#,
            ),
            (r#""events":[]"#, r#""events":["unknown_event"]"#),
            (r#""status":"completed""#, r#""status":"unknown""#),
            (r#""id":118578147"#, r#""id":118578147,"\u0069d":118578147"#),
            (
                r#""latest_check_runs_count":1"#,
                r#""latest_check_runs_count":9007199254740992"#,
            ),
            (r#""head_commit":{"#, r#""head_commit":{"unknown":true,"#),
        ] {
            let candidate = replaced_once(&input, old, new);
            assert!(candidate != input, "mutation absent: {old}");
            assert_eq!(
                authenticate_target(&source, &candidate, &target),
                Err(ProviderError::Authentication),
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn check_suite_markers_cannot_downgrade_or_become_pr_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let event: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let marker = format!(
        r#""check_suite":{}"#,
        serde_json::to_string(&event.check_suite).unwrap()
    );
    payload.check_suite = Some(event.check_suite);
    for action in [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "closed",
        "completed",
        "requested",
        "rerequested",
    ] {
        payload.action = Some(action.to_owned());
        let input = serde_json::to_vec(&payload).unwrap();
        let null = replaced_once(&input, &marker, r#""check_suite":null"#);
        assert!(null != input, "suite marker must exist");
        for candidate in [&input, &null] {
            assert_eq!(
                authenticate_target(&source, candidate, &target),
                Err(ProviderError::Authentication),
                "{action}"
            );
        }
    }
}
