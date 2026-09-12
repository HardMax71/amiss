use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::workflow::WorkflowPullRequest;
use amiss_wire::model::ObjectFormat;

#[test]
fn workflow_references_keep_bindings_without_unused_urls() {
    let input = include_bytes!("../fixtures/workflow-webhook-pr.json");
    let (reference, length): (WorkflowPullRequest, _) =
        decode_bounded_json(input.as_slice(), None, input.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(length, input.len());
    assert_eq!(reference.id, 279_147_437);
    assert_eq!(reference.number, 2);
    assert_eq!(reference.head.branch, "changes");
    assert_eq!(reference.base.branch, "master");
    assert_eq!(reference.head.sha.object_format(), ObjectFormat::Sha1);
    assert_eq!(reference.base.sha.object_format(), ObjectFormat::Sha1);
    assert_ne!(reference.head.sha, reference.base.sha);
    let encoded = serde_json::to_string(&reference).unwrap();
    let metadata = encoded
        .replacen('{', r#"{"url":null,"unknown":true,"#, 1)
        .replace(r#""head":{"#, r#""head":{"label":false,"unknown":[],"#)
        .replace(r#""base":{"#, r#""base":{"user":{},"unknown":null,"#)
        .replace(r#""repo":{"#, r#""repo":{"url":false,"unknown":{},"#);
    assert_ne!(metadata, encoded);
    assert_eq!(
        serde_json::from_str::<WorkflowPullRequest>(&metadata).unwrap(),
        reference
    );
    assert_eq!(
        decode_bounded_json::<WorkflowPullRequest, _>(
            input.as_slice(),
            None,
            input.len() - 1,
            |bytes| serde_json::from_slice(bytes)
        ),
        Err(ProviderError::InvalidResponse)
    );
    assert!(serde_json::from_str::<WorkflowPullRequest>(&format!("{encoded} null")).is_err());
}

#[test]
fn workflow_references_require_typed_unique_identity_fields() {
    let reference: WorkflowPullRequest =
        serde_json::from_slice(include_bytes!("../fixtures/workflow-webhook-pr.json")).unwrap();
    let input = serde_json::to_string(&reference).unwrap();
    for field in ["id", "number", "head", "base", "ref", "sha", "repo", "name"] {
        let original = format!(r#""{field}":"#);
        for replacement in [
            format!(r#""missing_{field}":"#),
            format!(r#""{field}":null,"{field}":"#),
        ] {
            amiss_fixtures::assert_json_rejections::<WorkflowPullRequest>(
                &input,
                &[(&original, &replacement)],
            );
        }
    }
    for (field, value) in [
        ("id", reference.id),
        ("number", reference.number),
        ("id", reference.head.repo.id),
        ("id", reference.base.repo.id),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for invalid in [
            "null",
            "false",
            "-1",
            "-0",
            "2e0",
            "2.0",
            "9007199254740992",
        ] {
            amiss_fixtures::assert_json_rejections::<WorkflowPullRequest>(
                &input,
                &[(&original, &format!(r#""{field}":{invalid}"#))],
            );
        }
    }
    for sha in [&reference.head.sha, &reference.base.sha] {
        amiss_fixtures::assert_json_rejections::<WorkflowPullRequest>(
            &input,
            &[(sha.as_str(), "not-an-oid")],
        );
    }
}
