use amiss_controller_gitea::webhook::{
    HookIssueAction, PreviousReference, PullRequestChanges, PullRequestPayload,
};
use amiss_fixtures::GITEA_PULL_WEBHOOK;

#[test]
fn webhook_edits_consume_the_same_reference_from_both_forges()
-> Result<(), Box<dyn std::error::Error>> {
    let expected = PullRequestChanges {
        reference: Some(PreviousReference {
            from: "develop".to_owned(),
        }),
    };
    for wire in [
        r#"{"ref":{"from":"develop"}}"#,
        r#"{"ref":{"from":"develop"},"added_labels":null,"removed_labels":null}"#,
        r#"{"ref":{"from":"develop","extra":true},"title":false,"body":42}"#,
    ] {
        assert_eq!(serde_json::from_str::<PullRequestChanges>(wire)?, expected);
    }
    for wire in ["{}", r#"{"ref":null}"#, r#"{"added_labels":[{"id":1}]}"#] {
        assert_eq!(
            serde_json::from_str::<PullRequestChanges>(wire)?.reference,
            None
        );
    }
    for wire in [
        r#"{"ref":{}}"#,
        r#"{"ref":{"from":null}}"#,
        r#"{"ref":{"from":"main","from":"topic"}}"#,
        r#"{"ref":true}"#,
        r#"{"ref":null,"ref":null}"#,
    ] {
        assert!(
            serde_json::from_str::<PullRequestChanges>(wire).is_err(),
            "{wire}"
        );
    }
    Ok(())
}

#[test]
fn webhook_inputs_use_native_required_fields_and_ignore_unconsumed_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let payload: PullRequestPayload = serde_json::from_slice(GITEA_PULL_WEBHOOK)?;
    let minimal = serde_json::to_string(&payload)?;
    let metadata = minimal.replacen(
        '{',
        r#"{"extra":[null,true],"sender":false,"review":42,"commit_id":{},"before":true,"after":[],"label":null,"changes":null,"#,
        1,
    );
    assert_eq!(
        serde_json::from_str::<PullRequestPayload>(&metadata)?,
        payload
    );
    let positional = serde_json::to_vec(&(
        payload.action,
        &payload.repository,
        payload.number,
        &payload.pull_request,
        &payload.changes,
    ))?;
    assert_eq!(
        serde_json::from_slice::<PullRequestPayload>(&positional)?,
        payload
    );

    for (field, value) in [
        ("action", serde_json::to_string(&payload.action)?),
        ("number", payload.number.to_string()),
        ("repository", serde_json::to_string(&payload.repository)?),
        (
            "pull_request",
            serde_json::to_string(&payload.pull_request)?,
        ),
    ] {
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":null"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            let invalid = minimal.replacen(&format!(r#""{field}":{value}"#), &replacement, 1);
            assert_ne!(invalid, minimal, "{field}");
            assert!(
                serde_json::from_str::<PullRequestPayload>(&invalid).is_err(),
                "{field}"
            );
        }
    }
    for replacement in ["-1", "9007199254740992", "true", r#""42""#] {
        let invalid = minimal.replacen(r#""number":42"#, &format!(r#""number":{replacement}"#), 1);
        assert_ne!(invalid, minimal);
        assert!(
            serde_json::from_str::<PullRequestPayload>(&invalid).is_err(),
            "{replacement}"
        );
    }
    let duplicate = minimal.replacen(r#""number":42"#, r#""number":42,"\u006eumber":42"#, 1);
    assert!(serde_json::from_str::<PullRequestPayload>(&duplicate).is_err());
    let trailing = format!("{minimal} true");
    assert!(serde_json::from_str::<PullRequestPayload>(&trailing).is_err());
    Ok(())
}

#[test]
fn webhook_actions_use_the_producer_string_vocabulary() -> Result<(), Box<dyn std::error::Error>> {
    for (tag, action) in [
        ("opened", HookIssueAction::Opened),
        ("reopened", HookIssueAction::Reopened),
        ("synchronized", HookIssueAction::Synchronized),
        ("edited", HookIssueAction::Edited),
        ("review_requested", HookIssueAction::ReviewRequested),
    ] {
        let wire = serde_json::to_string(tag)?;
        assert_eq!(serde_json::from_str::<HookIssueAction>(&wire)?, action);
        assert_eq!(serde_json::to_string(&action)?, wire);
    }
    for wire in [r#""unknown""#, r#"{"opened":null}"#, "null", "42"] {
        assert!(serde_json::from_str::<HookIssueAction>(wire).is_err());
    }
    Ok(())
}
