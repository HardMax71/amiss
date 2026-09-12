use amiss_controller::ProviderError;
use amiss_controller_github::GitHubPullRequestSource;
use amiss_controller_github::webhook::app::WebhookApp;
use amiss_controller_github::webhook::comment::issue::IssueCommentEvent;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::run::{CheckRunAction, CheckRunEvent};
use amiss_controller_github::webhook::suite::CheckSuiteEvent;
use amiss_controller_github::webhook::{Absent, GitHubPayload};
use amiss_wire::assessment::Nullable;
use amiss_wire::model::BranchRef;

use super::{
    BODY, authenticate_target, provider, replaced_once, source, webhook, workflow_artifact,
};

#[test]
fn signed_webhook_app_fields_preserve_no_work_in_every_consumer() {
    let mut run: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    run.check_run.app = Some(WebhookApp { id: None });
    let mut suite: CheckSuiteEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE).unwrap();
    suite.check_suite.app.id = None;
    let IssueCommentEvent::Created { mut event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap()
    else {
        panic!("the fixture is a created issue comment")
    };
    event.issue.performed_via_github_app = Some(Nullable::Value(Box::new(WebhookApp { id: None })));
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for source in [
        source(),
        GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]),
    ] {
        for (wire, member, old_action, actions) in [
            (
                serde_json::to_string(&run).unwrap(),
                "app",
                r#""action":"completed""#,
                &[
                    r#""action":"created""#,
                    r#""action":"completed""#,
                    r#""action":"rerequested""#,
                    r#""action":"requested_action""#,
                ][..],
            ),
            (
                serde_json::to_string(&suite).unwrap(),
                "app",
                r#""action":"completed""#,
                &[
                    r#""action":"completed""#,
                    r#""action":"requested""#,
                    r#""action":"rerequested""#,
                ][..],
            ),
            (
                serde_json::to_string(&IssueCommentEvent::Created {
                    changes: Absent,
                    event: event.clone(),
                })
                .unwrap(),
                "performed_via_github_app",
                r#""action":"created""#,
                &[
                    r#""action":"created""#,
                    r#""action":"edited","changes":{}"#,
                    r#""action":"deleted""#,
                ][..],
            ),
        ] {
            assert_eq!(wire.matches(old_action).count(), 1);
            let marker = format!(r#""{member}":{{"id":null}}"#);
            assert_eq!(wire.matches(&marker).count(), 1);
            for action in actions {
                let input = wire.replacen(old_action, action, 1);
                assert_eq!(
                    authenticate_target(&source, input.as_bytes(), &target),
                    Ok(None)
                );
                for (app, valid) in [
                    (
                        r#"{"id":null,"owner":false,"permissions":null,"events":["future"],"unknown":1.5}"#,
                        true,
                    ),
                    (
                        r#"{"id":9007199254740991,"owner":[],"permissions":{"checks":"future"},"events":false}"#,
                        true,
                    ),
                    ("{}", false),
                    (r#"{"id":9007199254740992}"#, false),
                    (r#"{"id":-1}"#, false),
                    (r#"{"id":"1"}"#, false),
                    (r#"{"id":null,"\u0069d":null}"#, false),
                ] {
                    let changed = input.replacen(&marker, &format!(r#""{member}":{app}"#), 1);
                    assert_ne!(changed, input);
                    assert_eq!(
                        authenticate_target(&source, changed.as_bytes(), &target),
                        valid.then_some(None).ok_or(ProviderError::Authentication),
                        "{member}, {action}: {app}"
                    );
                }
            }
        }
    }
}

#[test]
fn signed_check_runs_ignore_metadata_without_creating_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let input = amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN;
    assert_eq!(authenticate_target(&source, input, &target), Ok(None));
    for (member, metadata) in [
        (
            r#""check_run": {"#,
            r#""check_run": {"unknown":true,"check_suite":false,"deployment":[],"#,
        ),
        (r#""sender": {"#, r#""sender": {"unknown":true,"name":{},"#),
    ] {
        let changed = replaced_once(input, member, metadata);
        assert_ne!(changed, input);
        assert_eq!(authenticate_target(&source, &changed, &target), Ok(None));
    }
}

#[test]
fn signed_check_run_actions_reject_malformed_envelopes() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    for action in [
        CheckRunAction::Created,
        CheckRunAction::Completed,
        CheckRunAction::Rerequested,
    ] {
        event.action.action = Some(action);
        let input = serde_json::to_vec(&event).unwrap();
        assert_eq!(authenticate_target(&source, &input, &target), Ok(None));
        for (old, new) in [
            ("{", r#"{"installation":null,"#),
            (r#""id":128620228"#, r#""id":128620228,"\u0069d":128620228"#),
            (r#""id":128620228"#, r#""id":9007199254740992"#),
            (r#""output":{"#, r#""missing_output":{"#),
            (r#""status":"completed""#, r#""status":"future""#),
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
    let fragment = br#"{"action":"completed","check_run":{"id":89721586894},"installation":{"id":7,"node_id":"installation-seven"}}"#;
    assert_eq!(
        authenticate_target(&source, fragment, &target),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn check_run_markers_cannot_downgrade_or_become_pr_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    let marker = format!(
        r#""check_run":{}"#,
        serde_json::to_string(&event.check_run).unwrap()
    );
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    for action in [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "closed",
        "completed",
        "rerequested",
        "requested_action",
    ] {
        payload.action = Some(action.to_owned());
        let input = replaced_once(
            &serde_json::to_vec(&payload).unwrap(),
            "{",
            &format!("{{{marker},"),
        );
        let null = replaced_once(&input, &marker, r#""check_run":null"#);
        assert_ne!(null, input);
        for candidate in [&input, &null] {
            assert_eq!(
                authenticate_target(&source, candidate, &target),
                Err(ProviderError::Authentication),
                "{action}"
            );
        }
    }
}

#[test]
fn signed_requested_actions_keep_typed_identifiers_and_ignore_metadata() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let event: CheckRunEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN).unwrap();
    let input = serde_json::to_string(&event).unwrap().replacen(
        r#""action":"completed""#,
        r#""action":"requested_action""#,
        1,
    );
    for (addition, valid) in [
        (r#""requested_action":{"identifier":"retry"},"#, true),
        (r#""requested_action":{},"#, true),
        (r#""requested_action":null,"#, false),
        (r#""requested_action":{"identifier":null},"#, false),
        (r#""requested_action":{"unknown":true},"#, true),
    ] {
        let candidate = input.replacen('{', &format!("{{{addition}"), 1);
        assert_eq!(
            authenticate_target(&source, candidate.as_bytes(), &target),
            if valid {
                Ok(None)
            } else {
                Err(ProviderError::Authentication)
            },
            "{addition}"
        );
        if valid {
            assert!(matches!(
                serde_json::from_str::<GitHubEvent>(&candidate).unwrap(),
                GitHubEvent::RequestedCheckRun(_)
            ));
        }
    }
}
