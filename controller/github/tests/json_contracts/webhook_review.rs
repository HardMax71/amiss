use amiss_controller_github::pull::metadata::{StackBase, StackRecord};
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_wire::model::{ObjectFormat, Oid};

#[test]
fn published_review_event_retains_every_member() {
    let event: GitHubEvent =
        amiss_wire::read_json(amiss_fixtures::GITHUB_WEBHOOK_REVIEW, u64::MAX).unwrap();
    assert!(matches!(event, GitHubEvent::Review(_)));
    assert!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&event).unwrap()).unwrap()
            == amiss_fixtures::canonical_json(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap(),
        "the complete published review event must retain every member"
    );
}

#[test]
fn review_events_keep_action_specific_rules() {
    let published: ReviewEvent =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap();
    let wire = serde_json::to_string(&published).unwrap();
    let stack = serde_json::to_string(&StackRecord {
        base: StackBase {
            branch: "master".to_owned(),
            sha: Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap(),
        },
        size: None,
        position: None,
        id: None,
        number: None,
    })
    .unwrap();
    let stacked = format!(r#""id":279147437,"stack":{stack}"#);
    for action in ["submitted", "dismissed", "edited"] {
        let input = wire.replacen(
            r#""action":"submitted""#,
            &format!(r#""action":"{action}""#),
            1,
        );
        let input = if action == "dismissed" {
            input.replacen(r#""state":"commented""#, r#""state":"dismissed""#, 1)
        } else if action == "edited" {
            input.replacen('{', r#"{"changes":{},"#, 1)
        } else {
            input
        };
        let event: GitHubEvent = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
        assert!(matches!(event, GitHubEvent::Review(_)));
        assert!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&event).unwrap()).unwrap()
                == amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
            "the {action} event must retain every member"
        );
        if action == "edited" {
            let missing = input.replacen(r#""changes":{},"#, "", 1);
            assert!(serde_json::from_str::<GitHubEvent>(&missing).is_err());
        }
        let state = if action == "dismissed" {
            "dismissed"
        } else {
            "commented"
        };
        let state_member = format!(r#""state":"{state}""#);
        for (old, new, valid) in [
            ("{", r#"{"unknown":true,"#, false),
            ("{", r#"{"organization":null,"#, false),
            ("{", r#"{"enterprise":null,"#, false),
            (r#""review":{"#, r#""review":{"unknown":true,"#, false),
            (
                r#""id":279147437"#,
                r#""id":279147437,"unknown":true"#,
                false,
            ),
            (
                r#""id":237895671"#,
                r#""id":237895671,"\u0069d":237895671"#,
                false,
            ),
            (r#""id":237895671"#, r#""id":9007199254740992"#, false),
            (r#","draft":false"#, "", false),
            (r#""body":null"#, r#""body":false"#, false),
            (r#""id":279147437"#, stacked.as_str(), action != "edited"),
            (
                state_member.as_str(),
                r#""state":"future_state""#,
                action != "dismissed",
            ),
            (
                r#""submitted_at":"2019-05-15T15:20:38Z""#,
                r#""submitted_at":null"#,
                action != "dismissed",
            ),
            (
                r#""type":"User""#,
                r#""type":"Mannequin""#,
                action == "dismissed",
            ),
            (
                r#""active_lock_reason":null"#,
                r#""active_lock_reason":"custom""#,
                false,
            ),
        ] {
            assert!(input.contains(old), "{old}");
            let candidate = input.replacen(old, new, 1);
            assert_eq!(
                serde_json::from_str::<GitHubEvent>(&candidate).is_ok(),
                valid,
                "{action}: {new}"
            );
            assert_eq!(
                amiss_wire::read_json::<GitHubEvent>(candidate.as_bytes(), u64::MAX).is_ok(),
                valid,
                "{action}: {new}"
            );
        }
    }
}
