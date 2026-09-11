use amiss_controller_gitea::issue::Label;
use amiss_controller_gitea::webhook::{
    HookIssueAction, PreviousReference, PullRequestChanges, PullRequestPayload, ReviewPayload,
    ReviewType,
};
use amiss_fixtures::GITEA_PULL_WEBHOOK;
use amiss_wire::model::{ObjectFormat, Oid};

#[test]
fn webhook_metadata_retains_both_edit_shapes() -> Result<(), Box<dyn std::error::Error>> {
    let mut payload: PullRequestPayload = serde_json::from_slice(GITEA_PULL_WEBHOOK)?;
    let label = Label {
        id: 1,
        name: "docs".to_owned(),
        exclusive: false,
        is_archived: false,
        color: "abcdef".to_owned(),
        description: "Documentation".to_owned(),
        url: "https://forge.example/acme/widget/labels/1".to_owned(),
    };
    let previous = PreviousReference {
        from: "develop".to_owned(),
    };
    payload.action = HookIssueAction::Edited;
    payload.requested_reviewer = payload.sender.clone();
    payload.label = Some(label.clone());
    payload.before = Oid::new(ObjectFormat::Sha1, "a".repeat(40));
    payload.after = Oid::new(ObjectFormat::Sha1, "b".repeat(40));
    payload.commit_id = payload.after.clone();
    payload.review = Some(ReviewPayload {
        kind: ReviewType::PullRequestReviewApproved,
        content: "Looks good".to_owned(),
    });
    for changes in [
        PullRequestChanges::Gitea {
            title: Some(previous.clone()),
            body: Some(previous.clone()),
            reference: Some(previous.clone()),
            name: Some(previous.clone()),
            added_labels: Some(vec![label.clone()]),
            removed_labels: Some(vec![label]),
        },
        PullRequestChanges::Gitea {
            title: None,
            body: None,
            reference: Some(previous.clone()),
            name: None,
            added_labels: None,
            removed_labels: Some(vec![]),
        },
        PullRequestChanges::Forgejo {
            title: Some(previous.clone()),
            body: Some(previous.clone()),
            reference: Some(previous),
        },
    ] {
        payload.changes = Some(changes);
        let wire = serde_json::to_vec(&payload)?;
        assert_eq!(
            amiss_wire::read_json::<PullRequestPayload>(&wire, u64::MAX)?,
            payload
        );
    }
    Ok(())
}

#[test]
fn webhook_nullable_and_omitted_fields_remain_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let original: PullRequestPayload = serde_json::from_slice(GITEA_PULL_WEBHOOK)?;
    let empty = PullRequestPayload {
        repository: None,
        pull_request: None,
        requested_reviewer: None,
        sender: None,
        review: None,
        ..original
    };
    let wire = serde_json::to_string(&empty)?;
    assert_eq!(
        amiss_wire::read_json::<PullRequestPayload>(wire.as_bytes(), u64::MAX)?,
        empty
    );
    for field in [
        "repository",
        "pull_request",
        "requested_reviewer",
        "sender",
        "review",
    ] {
        let changed = wire.replacen(&format!(",\"{field}\":null"), "", 1);
        assert_ne!(changed, wire);
        assert!(
            serde_json::from_str::<PullRequestPayload>(&changed).is_err(),
            "{field}"
        );
    }
    for field in ["changes", "before", "after", "label"] {
        let changed = wire.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(
            serde_json::from_str::<PullRequestPayload>(&changed).is_err(),
            "{field}"
        );
    }
    let null_commit = wire.replacen(r#""commit_id":"""#, r#""commit_id":null"#, 1);
    assert_ne!(null_commit, wire);
    assert_eq!(
        serde_json::from_str::<PullRequestPayload>(&null_commit)?,
        empty
    );
    for replacement in ["true", r#""bad-id""#] {
        let changed = wire.replacen(
            r#""commit_id":"""#,
            &format!(r#""commit_id":{replacement}"#),
            1,
        );
        assert_ne!(changed, wire);
        assert!(serde_json::from_str::<PullRequestPayload>(&changed).is_err());
    }
    let missing = wire.replacen(",\"commit_id\":\"\"", "", 1);
    assert_ne!(missing, wire);
    assert!(serde_json::from_str::<PullRequestPayload>(&missing).is_err());
    assert!(amiss_wire::read_json::<PullRequestPayload>(GITEA_PULL_WEBHOOK, 1).is_err());
    Ok(())
}

#[test]
fn webhook_objects_reject_undeclared_and_malformed_data() -> Result<(), Box<dyn std::error::Error>>
{
    let original = std::str::from_utf8(GITEA_PULL_WEBHOOK)?;
    for (needle, replacement) in [
        (r#""action":"opened""#, r#""action":"opened","extra":true"#),
        (r#""repository":{"#, r#""repository":{"extra":true,"#),
        (r#""pull_request":{"#, r#""pull_request":{"extra":true,"#),
        (r#""sender":{"#, r#""sender":{"id":false,"#),
        (r#""action":"opened""#, r#""action":{"opened":null}"#),
        (r#""number":42"#, r#""number":9007199254740992"#),
        (r#""number":42"#, r#""number":-1"#),
        (r#""number":42"#, r#""number":42,"\u006eumber":42"#),
        (
            r#""review":null"#,
            r#""review":{"type":"pull_request_review_approved","content":"ok","extra":true}"#,
        ),
        (
            r#""review":null"#,
            r#""review":{"type":"pull_request_review_approved"}"#,
        ),
    ] {
        let changed = original.replacen(needle, replacement, 1);
        assert_ne!(changed, original, "{needle}");
        assert!(
            serde_json::from_str::<PullRequestPayload>(&changed).is_err(),
            "{replacement}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestPayload>(changed.as_bytes(), u64::MAX).is_err(),
            "{replacement}"
        );
    }
    for changes in [
        r#"{"extra":true}"#,
        r#"{"ref":{"from":"main","extra":true}}"#,
        r#"{"ref":null}"#,
        r#"{"ref":{}}"#,
        r#"{"added_labels":null}"#,
        r#"{"added_labels":[],"removed_labels":null,"name":null}"#,
        r#"{"added_labels":[{"id":1}],"removed_labels":null}"#,
    ] {
        assert!(
            serde_json::from_str::<PullRequestChanges>(changes).is_err(),
            "{changes}"
        );
    }
    Ok(())
}

#[test]
fn webhook_tags_use_the_producer_string_vocabulary() -> Result<(), Box<dyn std::error::Error>> {
    for (tag, review) in [
        (
            "pull_request_review_approved",
            ReviewType::PullRequestReviewApproved,
        ),
        (
            "pull_request_review_rejected",
            ReviewType::PullRequestReviewRejected,
        ),
        (
            "pull_request_review_comment",
            ReviewType::PullRequestReviewComment,
        ),
    ] {
        let wire = serde_json::to_string(tag)?;
        assert_eq!(serde_json::from_str::<ReviewType>(&wire)?, review);
        assert_eq!(serde_json::to_string(&review)?, wire);
    }
    for wire in [
        r#""approved""#,
        r#""unknown""#,
        r#"{"pull_request_review_approved":null}"#,
    ] {
        assert!(serde_json::from_str::<ReviewType>(wire).is_err());
    }
    for wire in [r#""unknown""#, r#"{"opened":null}"#, "null", "42"] {
        assert!(serde_json::from_str::<HookIssueAction>(wire).is_err());
    }
    Ok(())
}
