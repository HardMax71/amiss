use amiss_controller_github::webhook::PullRequestChanges;

const CHANGES: &[u8] = include_bytes!("../fixtures/webhook-pull-changes.json");

#[test]
fn edited_changes_keep_only_the_previous_base_reference() {
    let expected: PullRequestChanges =
        serde_json::from_str(r#"{"base":{"ref":{"from":"previous-main"}}}"#).unwrap();
    for input in [
        CHANGES,
        br#"{"base":{"ref":{"from":"previous-main","extra":null},"sha":false,"extra":[]},"title":null,"body":{},"extra":true}"#,
    ] {
        assert_eq!(
            serde_json::from_slice::<PullRequestChanges>(input).unwrap(),
            expected
        );
    }
    assert_eq!(expected.base.unwrap().reference.from, "previous-main");
    for input in [
        "{}",
        r#"{"body":{"from":""}}"#,
        r#"{"title":{"from":"Previous title"}}"#,
        r#"{"title":false,"body":null,"unknown":[]}"#,
    ] {
        assert_eq!(
            serde_json::from_str::<PullRequestChanges>(input)
                .unwrap()
                .base,
            None
        );
    }
}

#[test]
fn base_changes_require_a_nonnull_unique_previous_reference() {
    for input in [
        r#"{"base":null}"#,
        r#"{"base":false}"#,
        r#"{"base":{}}"#,
        r#"{"base":{"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
        r#"{"base":{"ref":null}}"#,
        r#"{"base":{"ref":false}}"#,
        r#"{"base":{"ref":{}}}"#,
        r#"{"base":{"ref":{"from":null}}}"#,
        r#"{"base":{"ref":{"from":1}}}"#,
        r#"{"base":{"ref":{"from":false}}}"#,
        r#"{"base":{"ref":{"from":"main"}},"\u0062ase":{"ref":{"from":"main"}}}"#,
        r#"{"base":{"ref":{"from":"main"},"\u0072ef":{"from":"main"}}}"#,
        r#"{"base":{"ref":{"from":"main","\u0066rom":"main"}}}"#,
        r#"{"base":{"ref":{"from":"main"}}} null"#,
    ] {
        assert!(
            serde_json::from_str::<PullRequestChanges>(input).is_err(),
            "{input}"
        );
    }
}
