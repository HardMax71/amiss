#![cfg(test)]

use amiss_controller_github::webhook::GitHubPayload;

use super::prepare_webhook;

#[test]
fn github_mutations_keep_unselected_metadata_and_target_each_bound_fact() {
    type Change = fn(&mut GitHubPayload);
    let original = prepare_webhook(&[]);
    let original: GitHubPayload = serde_json::from_slice(&original.body).unwrap();
    let cases: [(u8, Change); 8] = [
        (1, |p| p.action = Some("synchronize".to_owned())),
        (2, |p| p.action = Some("b".to_owned())),
        (3, |p| p.number = Some(1)),
        (4, |p| p.pull_request.as_mut().unwrap().number = 1),
        (5, |p| p.pull_request.as_mut().unwrap().id = 1),
        (7, |p| {
            p.pull_request.as_mut().unwrap().head.branch = "b".to_owned();
        }),
        (8, |p| {
            p.pull_request.as_mut().unwrap().base.branch = "b".to_owned();
        }),
        (9, |p| p.repository.as_mut().unwrap().name = "b".to_owned()),
    ];
    for (selector, change) in cases {
        let mut expected = original.clone();
        change(&mut expected);
        let data = [selector, 1];
        let result = prepare_webhook(&data);
        let changed: GitHubPayload = serde_json::from_slice(&result.body).unwrap();
        assert_eq!(changed, expected, "selector {selector}");
        assert_eq!(result.target_matches, selector != 8);
    }
    for bytes in [&[6, 1][..], &[6, 34, 92, 0, 255][..]] {
        let malformed = prepare_webhook(bytes);
        assert!(serde_json::from_slice::<GitHubPayload>(&malformed.body).is_err());
        let text = std::str::from_utf8(&malformed.body).unwrap();
        assert!(text.contains(&format!(
            "\"sha\":{}",
            serde_json::to_string(&super::text(&bytes[1..])).unwrap()
        )));
    }
}
