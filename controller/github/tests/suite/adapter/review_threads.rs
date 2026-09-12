use amiss_controller::ProviderError;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_controller_github::webhook::thread::{ReviewThread, ReviewThreadEvent, ThreadPayload};
use amiss_controller_github::webhook::{Absent, GitHubPayload};
use amiss_wire::model::BranchRef;

use super::{BODY, authenticate_target, replaced_once, source};

#[test]
fn signed_review_threads_ignore_additions_but_check_known_fields() {
    let ReviewEvent::Submitted { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("submitted review")
    };
    let resolved = serde_json::to_vec(&ReviewThreadEvent::Resolved {
        event: ThreadPayload {
            thread: ReviewThread {
                node_id: "PRRT_kwDOFd42Pc4rQOUv".to_owned(),
                comments: vec![
                    serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT).unwrap(),
                ],
            },
            pull_request: serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW_PULL)
                .unwrap(),
            repository: event.repository,
            installation: event.installation,
            number: Absent,
            issue: Absent,
            review: Absent,
            comment: Absent,
            check_run: Absent,
            check_suite: Absent,
            workflow: Absent,
            workflow_run: Absent,
            requested_action: Absent,
            changes: Absent,
        },
    })
    .unwrap();
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for action in ["resolved", "unresolved"] {
        let input = replaced_once(
            &resolved,
            r#""action":"resolved""#,
            &format!(r#""action":"{action}""#),
        );
        assert_eq!(authenticate_target(&source, &input, &target), Ok(None));
        for (old, new) in [
            (r#","draft":false"#, ""),
            (r#""auto_merge":null,"#, ""),
            (r#""id":284312630"#, r#""id":284312630,"\u0069d":284312630"#),
        ] {
            let candidate = replaced_once(&input, old, new);
            assert!(candidate != input, "mutation absent: {old}");
            assert_eq!(
                authenticate_target(&source, &candidate, &target),
                Err(ProviderError::Authentication),
                "{action}: {new}"
            );
        }
        for (old, new, valid) in [
            (r#""thread":{"#, r#""thread":{"unknown":true,"#, true),
            (r#""comments":[{"#, r#""comments":[{"unknown":true,"#, true),
            (
                r#""id":279147437"#,
                r#""id":279147437,"unknown":true"#,
                true,
            ),
            (
                r#""original_line":265"#,
                r#""original_line":null"#,
                action == "resolved",
            ),
            (
                r#""type":"User""#,
                r#""type":"Mannequin""#,
                action == "resolved",
            ),
            (r#""head":{"#, r#""head":{"label":null,"user":false,"#, true),
            (
                r#""requested_reviewers":["#,
                r#""requested_reviewers":[{"name":"docs","id":1},"#,
                action == "unresolved",
            ),
        ] {
            let candidate = replaced_once(&input, old, new);
            assert!(candidate != input, "mutation absent: {old}");
            assert_eq!(
                authenticate_target(&source, &candidate, &target),
                valid.then_some(None).ok_or(ProviderError::Authentication),
                "{action}: {new}"
            );
        }
    }
}

#[test]
fn thread_markers_cannot_downgrade_or_become_active_pr_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let thread: ReviewThread = ReviewThread {
        node_id: "PRRT_kwDOFd42Pc4rQOUv".to_owned(),
        comments: Vec::new(),
    };
    let marker = format!(r#""thread":{}"#, serde_json::to_string(&thread).unwrap());
    for action in [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "closed",
        "resolved",
        "unresolved",
    ] {
        payload.action = Some(action.to_owned());
        let input = replaced_once(
            &serde_json::to_vec(&payload).unwrap(),
            "{",
            &format!("{{{marker},"),
        );
        let null = replaced_once(&input, &marker, r#""thread":null"#);
        assert_ne!(null, input);
        for candidate in [&input, &null] {
            assert_eq!(
                authenticate_target(&source, candidate, &target),
                Err(ProviderError::Authentication),
                "{action}"
            );
        }
    }
    for action in ["resolved", "unresolved"] {
        payload.action = Some(action.to_owned());
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&payload).unwrap(), &target),
            Err(ProviderError::Authentication)
        );
    }
}
