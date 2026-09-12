use amiss_controller::ProviderError;
use amiss_controller_github::webhook::GitHubPayload;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::run::{CheckRunAction, CheckRunEvent};
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

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
            ("{", r#"{"unknown":true,"#),
            ("{", r#"{"installation":null,"#),
            (r#""app":{"#, r#""app":{"unknown":true,"#),
            (r#""permissions":{"#, r#""permissions":{"unknown":"read","#),
            (r#""events":[]"#, r#""events":["future"]"#),
            (r#""id":128620228"#, r#""id":128620228,"\u0069d":128620228"#),
            (r#""id":128620228"#, r#""id":9007199254740992"#),
            (r#""output":{"#, r#""missing_output":{"#),
            (r#""status":"completed""#, r#""status":"future""#),
            (r#""sender":{"#, r#""sender":{"login":null,"#),
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
fn signed_requested_actions_keep_the_closed_action_metadata() {
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
        (r#""requested_action":{"unknown":true},"#, false),
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
