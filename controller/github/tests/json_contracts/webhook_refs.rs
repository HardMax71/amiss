use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::workflow::WorkflowPullRequest;
use amiss_wire::model::ObjectFormat;

#[test]
fn workflow_webhook_references_retain_their_bound_pull_request() {
    let input = include_bytes!("../fixtures/workflow-webhook-pr.json");
    let reference: WorkflowPullRequest = serde_json::from_slice(input).unwrap();
    assert_eq!(reference.id, 279_147_437);
    assert_eq!(reference.number, 2);
    assert_eq!(reference.head.branch, "changes");
    assert_eq!(reference.base.branch, "master");
    assert_eq!(reference.head.sha.object_format(), ObjectFormat::Sha1);
    assert_eq!(reference.base.sha.object_format(), ObjectFormat::Sha1);
    assert_ne!(reference.head.sha, reference.base.sha);
    let encoded = serde_json::to_vec(&reference).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&encoded).unwrap(),
        amiss_fixtures::canonical_json(input).unwrap()
    );
    let (strict, length): (WorkflowPullRequest, _) =
        decode_bounded_json(input.as_slice(), None, input.len(), |bytes| {
            amiss_wire::read_json(bytes, u64::MAX)
        })
        .unwrap();
    assert_eq!(strict, reference);
    assert_eq!(length, input.len());
    assert_eq!(
        decode_bounded_json::<WorkflowPullRequest, _>(
            encoded.as_slice(),
            None,
            encoded.len() - 1,
            |bytes| amiss_wire::read_json(bytes, u64::MAX)
        ),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn workflow_pr_references_require_complete_closed_objects_and_checked_scalars() {
    let input = include_str!("../fixtures/workflow-webhook-pr.json");
    for (old, new) in [
        (
            r#""url": "https://api.github.com/repos/Codertocat/Hello-World/pulls/2","#,
            "",
        ),
        (
            r#""url": "https://api.github.com/repos/Codertocat/Hello-World/pulls/2""#,
            r#""url": "https://api.github.com/repos/Codertocat/Hello-World/pulls/2", "unknown": 0"#,
        ),
        (r#""id": 279147437"#, r#""id": 9007199254740992"#),
        (r#""id": 279147437"#, r#""id": null"#),
        (r#""number": 2"#, r#""number": 2.5"#),
        (r#""number": 2"#, r#""number": "2""#),
        (r#""head": {"#, r#""head": {"unknown": false,"#),
        (
            r#""sha": "ec26c3e57ca3a959ca5aad62de7213c562f8c821""#,
            r#""sha": "not-an-oid""#,
        ),
        (
            r#""base": {"#,
            r#""base": {"ref": "shadow", "\u0072ef": "shadow","#,
        ),
        (r#""ref": "master","#, ""),
        (r#""repo": {"#, r#""repo": {"unknown": true,"#),
        (
            r#""url": "https://api.github.com/repos/Codertocat/Hello-World","#,
            "",
        ),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowPullRequest>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowPullRequest>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
    let reference: WorkflowPullRequest = serde_json::from_str(input).unwrap();
    let positional = serde_json::to_vec(&(
        reference.id,
        reference.number,
        &reference.url,
        &reference.head,
        &reference.base,
    ))
    .unwrap();
    assert!(amiss_wire::read_json::<WorkflowPullRequest>(&positional, u64::MAX).is_err());
    for value in ["-0", "2e0", "2.0"] {
        let changed = input.replacen(r#""number": 2"#, &format!(r#""number": {value}"#), 1);
        assert!(
            amiss_wire::read_json::<WorkflowPullRequest>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
    for changed in [
        format!("{input} null"),
        format!("[{input}]"),
        "null".to_owned(),
    ] {
        assert!(
            amiss_wire::read_json::<WorkflowPullRequest>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
}
