use amiss_controller_github::owner::OwnerRecord;
use amiss_controller_github::webhook::comment::issue::{
    IssueCommentEvent, IssueCommentRecord, PinnedIssueCommentMetadata,
};
use amiss_controller_github::webhook::issue::IssueRecord;
use amiss_controller_github::webhook::issue::context::IssueActivityContext;
use amiss_controller_github::webhook::issue::field::IssueFieldValue;
use amiss_wire::assessment::Nullable;

#[test]
fn ordinary_issue_metadata_does_not_inherit_comment_requiredness() {
    let IssueCommentEvent::Created { event, .. } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT_EVENT).unwrap()
    else {
        panic!("the fixture is a created issue comment")
    };
    let mut pinned: IssueCommentRecord<OwnerRecord, PinnedIssueCommentMetadata> =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT).unwrap();
    pinned.metadata = PinnedIssueCommentMetadata::default();
    let mut issue: IssueRecord<IssueActivityContext> =
        serde_json::from_slice(&serde_json::to_vec(&event.issue).unwrap()).unwrap();
    issue.context.user = Nullable::Null;
    issue.context.assignee = None;
    issue.context.labels = None;
    issue.context.locked = None;
    issue.context.state = None;
    issue.context.pinned_comment = Some(Nullable::Value(Box::new(pinned)));
    issue.context.issue_field_values = Some(vec![
        serde_json::from_str(
            r#"{"issue_field_id":1,"node_id":"field","data_type":"number","value":42.5}"#,
        )
        .unwrap(),
    ]);
    let wire = serde_json::to_vec(&issue).unwrap();
    assert_eq!(
        serde_json::from_slice::<IssueRecord<IssueActivityContext>>(&wire).unwrap(),
        issue
    );
    assert!(serde_json::from_slice::<IssueRecord>(&wire).is_err());
    let input = std::str::from_utf8(&wire).unwrap();
    let metadata = input.replacen(
        r#""pinned_comment":{"#,
        r#""pinned_comment":{"unknown":true,"#,
        1,
    );
    assert_ne!(metadata, input);
    assert_eq!(
        serde_json::from_str::<IssueRecord<IssueActivityContext>>(&metadata).unwrap(),
        issue
    );
    for (old, new) in [
        (r#""user":null,"#, ""),
        (r#""value":42.5"#, r#""value":true"#),
        (
            r#""issue_field_id":1"#,
            r#""issue_field_id":9007199254740992"#,
        ),
    ] {
        assert!(input.contains(old), "mutation absent: {old}");
        let invalid = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<IssueRecord<IssueActivityContext>>(&invalid).is_err(),
            "{new}"
        );
    }
}

#[test]
fn issue_field_values_are_known_scalars_not_arbitrary_json() {
    for (value, valid) in [
        ("null", true),
        ("42.5", true),
        ("-3", true),
        (r#""text""#, true),
        ("true", false),
        ("[]", false),
        ("{}", false),
    ] {
        let input = format!(
            r#"{{"issue_field_id":1,"node_id":"field","data_type":"number","value":{value}}}"#
        );
        let parsed = serde_json::from_str::<IssueFieldValue>(&input);
        assert_eq!(parsed.is_ok(), valid, "{value}");
        if let Ok(field) = parsed {
            assert_eq!(
                amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
                amiss_fixtures::canonical_json(&serde_json::to_vec(&field).unwrap()).unwrap()
            );
        }
    }
}
