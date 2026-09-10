use amiss_controller_github::webhook::PullRequestChanges;

const CHANGES: &[u8] = include_bytes!("../fixtures/webhook-pull-changes.json");

#[test]
fn edited_changes_retain_all_fields_and_omissions() {
    for input in [
        CHANGES,
        b"{}",
        br#"{"body":{"from":""}}"#,
        br#"{"title":{"from":"Previous title"}}"#,
        br#"{"base":{"ref":{"from":"previous-main"},"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
    ] {
        let changes: PullRequestChanges = serde_json::from_slice(input).unwrap();
        let encoded = serde_json::to_vec(&changes).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(input).unwrap(),
            amiss_fixtures::canonical_json(&encoded).unwrap(),
            "{input:?}"
        );
        assert_eq!(
            amiss_wire::read_json::<PullRequestChanges>(input, u64::MAX).unwrap(),
            changes
        );
    }
    let changes: PullRequestChanges = serde_json::from_slice(CHANGES).unwrap();
    let base = changes.base.unwrap();
    assert_eq!(base.reference.from, "previous-main");
    assert_eq!(
        base.sha.from.as_str(),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        changes.body.unwrap().from,
        "Previous documentation details."
    );
    assert_eq!(changes.title.unwrap().from, "Previous pull request title");
}

#[test]
fn previous_fields_are_required_and_optional_changes_are_nonnull_when_present() {
    for input in [
        r#"{"base":null}"#,
        r#"{"body":null}"#,
        r#"{"title":null}"#,
        r#"{"base":{}}"#,
        r#"{"base":{"ref":{"from":"main"}}}"#,
        r#"{"base":{"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
        r#"{"base":{"ref":null,"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
        r#"{"base":{"ref":{"from":"main"},"sha":null}}"#,
        r#"{"base":{"ref":{},"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
        r#"{"base":{"ref":{"from":null},"sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
        r#"{"base":{"ref":{"from":"main"},"sha":{}}}"#,
        r#"{"base":{"ref":{"from":"main"},"sha":{"from":null}}}"#,
        r#"{"base":{"ref":{"from":"main"},"sha":{"from":"invalid-oid"}}}"#,
        r#"{"body":{}}"#,
        r#"{"body":{"from":null}}"#,
        r#"{"body":{"from":1}}"#,
        r#"{"title":{}}"#,
        r#"{"title":{"from":null}}"#,
        r#"{"title":{"from":false}}"#,
    ] {
        assert!(
            serde_json::from_str::<PullRequestChanges>(input).is_err(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestChanges>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}

#[test]
fn edited_change_objects_reject_unknown_duplicate_and_positional_members() {
    let changes: PullRequestChanges = serde_json::from_slice(CHANGES).unwrap();
    let wire = serde_json::to_string(&changes).unwrap();
    for (old, new) in [
        (r#""base":{"#, r#""unknown":true,"base":{"#),
        (r#""base":{"#, r#""base":{"unknown":true,"#),
        (r#""ref":{"#, r#""ref":{"unknown":true,"#),
        (r#""sha":{"#, r#""sha":{"unknown":true,"#),
        (r#""body":{"#, r#""body":{"unknown":true,"#),
        (r#""title":{"#, r#""title":{"unknown":true,"#),
        (r#""base":{"#, r#""\u0062ase":{},"base":{"#),
        (r#""ref":{"#, r#""\u0072ef":{},"ref":{"#),
        (r#""sha":{"#, r#""\u0073ha":{},"sha":{"#),
        (r#""body":{"#, r#""\u0062ody":{},"body":{"#),
        (r#""title":{"#, r#""\u0074itle":{},"title":{"#),
        (
            r#""from":"previous-main""#,
            r#""\u0066rom":"duplicate","from":"previous-main""#,
        ),
    ] {
        assert_eq!(wire.matches(old).count(), 1, "{old}");
        let invalid = wire.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRequestChanges>(&invalid).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestChanges>(invalid.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let base = changes.base.as_ref().unwrap();
    let reference = serde_json::to_string(&base.reference).unwrap();
    let sha = serde_json::to_string(&base.sha).unwrap();
    for input in [
        serde_json::to_string(&(&changes.base, &changes.body, &changes.title)).unwrap(),
        wire.replacen(
            &reference,
            &serde_json::to_string(&[&base.reference.from]).unwrap(),
            1,
        ),
        wire.replacen(&sha, &serde_json::to_string(&[&base.sha.from]).unwrap(), 1),
        format!("{wire} null"),
    ] {
        assert!(
            amiss_wire::read_json::<PullRequestChanges>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}
