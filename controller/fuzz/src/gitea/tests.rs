#![cfg(test)]

use amiss_controller_gitea::webhook::{HookIssueAction, PullRequestPayload};

use super::{FIXTURE, prepare_webhook};

#[test]
fn wire_mutations_reach_only_the_selected_fixture_field() {
    type Mutation = fn(&mut PullRequestPayload);
    let cases: [(u8, Mutation); 7] = [
        (1, |p| p.action = HookIssueAction::Synchronized),
        (3, |p| p.number = 1),
        (4, |p| p.pull_request.number = 1),
        (5, |p| p.pull_request.id = 1),
        (7, |p| p.pull_request.head.branch = "b".to_owned()),
        (8, |p| p.pull_request.base.branch = "b".to_owned()),
        (9, |p| p.repository.name = "b".to_owned()),
    ];
    for (selector, change) in cases {
        let mut expected = FIXTURE.0.clone();
        change(&mut expected);
        let data = [selector, 1];
        let exercise = prepare_webhook(&data);
        let actual: PullRequestPayload = serde_json::from_slice(&exercise.body).unwrap();
        assert_eq!(actual, expected, "selector {selector}");
        assert_eq!(exercise.target_matches, selector != 8);
    }
    for (selector, fragment) in [(2, r#""action":"b""#), (6, r#""sha":"b""#)] {
        let data = [selector, 1];
        let exercise = prepare_webhook(&data);
        assert!(
            std::str::from_utf8(&exercise.body)
                .unwrap()
                .contains(fragment)
        );
        assert!(serde_json::from_slice::<PullRequestPayload>(&exercise.body).is_err());
    }
    let unchanged = prepare_webhook(&[]);
    assert_eq!(unchanged.body, FIXTURE.1.as_bytes());
}
