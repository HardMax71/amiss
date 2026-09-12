use amiss_controller::ProviderError;
use amiss_controller_github::GitHubPullRequestSource;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::suite::{CheckSuiteAction, CheckSuiteEvent};
use amiss_wire::model::BranchRef;

use super::{
    BODY, authenticate_target, provider, replaced_once, source, webhook, workflow_artifact,
};

#[test]
fn signed_check_roots_ignore_metadata_but_reject_conflicting_events() {
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for (capture, marker, actions) in [
        (
            amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN,
            "check_run",
            &[
                ("", false),
                (r#""action":"created","#, false),
                (r#""action":"completed","#, false),
                (r#""action":"rerequested","#, false),
                (r#""action":"requested_action","#, true),
            ][..],
        ),
        (
            amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE,
            "check_suite",
            &[
                (r#""action":"completed","#, false),
                (r#""action":"requested","#, false),
                (r#""action":"rerequested","#, false),
            ][..],
        ),
    ] {
        let captured: GitHubEvent = serde_json::from_slice(capture).unwrap();
        let wire = serde_json::to_string(&captured).unwrap();
        let old_action = r#""action":"completed","#;
        assert_eq!(wire.matches(old_action).count(), 1);
        for source in [
            source(),
            GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]),
        ] {
            for &(action, requested) in actions {
                let input = wire.replacen(old_action, action, 1);
                let original: GitHubEvent = serde_json::from_str(&input).unwrap();
                assert_eq!(
                    authenticate_target(&source, input.as_bytes(), &target),
                    Ok(None)
                );
                for metadata in [
                    r#"{"sender":null,"organization":false,"enterprise":[],"future":{"number":1.5},"#,
                    r#"{"sender":{"login":null},"organization":{},"enterprise":null,"future":42,"#,
                ] {
                    let changed = input.replacen('{', metadata, 1);
                    assert_ne!(changed, input);
                    assert!(serde_json::from_str::<GitHubEvent>(&changed).unwrap() == original);
                    assert_eq!(
                        authenticate_target(&source, changed.as_bytes(), &target),
                        Ok(None)
                    );
                }
                for field in [
                    "number",
                    "pull_request",
                    "issue",
                    "review",
                    "comment",
                    "thread",
                    "check_run",
                    "check_suite",
                    "workflow",
                    "workflow_run",
                    "requested_action",
                    "changes",
                ]
                .into_iter()
                .filter(|&field| field != marker && (field != "requested_action" || !requested))
                {
                    for value in ["null", "false", "42", "[]", "{}", r#"{"id":1}"#] {
                        let changed = input.replacen('{', &format!(r#"{{"{field}":{value},"#), 1);
                        assert_eq!(
                            authenticate_target(&source, changed.as_bytes(), &target),
                            Err(ProviderError::Authentication),
                            "{marker}, {action}: {field}={value}"
                        );
                    }
                }
                for (old, new) in [
                    ("{", r#"{"installation":null,"#),
                    (r#""repository":{"#, r#""repository":null,"repository":{"#),
                ] {
                    assert!(input.contains(old), "mutation absent: {old}");
                    let changed = input.replacen(old, new, 1);
                    assert_ne!(changed, input);
                    assert_eq!(
                        authenticate_target(&source, changed.as_bytes(), &target),
                        Err(ProviderError::Authentication),
                        "{marker}, {action}: {new}"
                    );
                }
            }
        }
    }
}

#[test]
fn signed_check_suites_keep_typed_facts_and_remain_no_work() {
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
            (r#""check_suite":{"#, r#""check_suite":{"unknown":true,"#),
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
        let input = replaced_once(
            &serde_json::to_vec(&payload).unwrap(),
            "{",
            &format!("{{{marker},"),
        );
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
