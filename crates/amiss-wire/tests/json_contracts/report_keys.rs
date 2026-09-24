use amiss_wire::controls::TargetIntentKind;
use amiss_wire::controls::TargetKind;
use amiss_wire::report::model::{
    EmptyRepositoryPath, FindingKeyScope, RepositoryIntentPath, RepositoryTargetIntent,
};

#[test]
fn finding_keys_require_closed_scope_tags() {
    let scope = r#"{"document":"docs/guide.md","kind":"reference","normalized_target_intent":{"fragment_digest":null,"kind":"repository-path","path":"docs/missing.md","query_digest":null,"target_kind":"either"},"occurrence":{"kind":"source-projection","source_projection_digest":"sha256:f30e7675558037c19d770e8ed46d748dd180aa4ebb636eba2f939a1b1e501c62"},"source_construct":"markdown-inline-link"}"#.to_owned();
    assert!(matches!(
        serde_json::from_str::<FindingKeyScope>(&scope).unwrap(),
        FindingKeyScope::Reference { .. }
    ));
    let wrong_tag = scope.replace("\"kind\":\"reference\"", "\"kind\":\"document\"");
    assert_ne!(wrong_tag, scope);
    assert!(serde_json::from_str::<FindingKeyScope>(&wrong_tag).is_err());
}

#[test]
fn finding_key_intents_reject_wrong_field_types() {
    let intent: RepositoryTargetIntent = RepositoryTargetIntent {
        commit_oid: None,
        fragment_digest: None,
        kind: TargetIntentKind::RepositoryPath,
        path: RepositoryIntentPath::Empty(EmptyRepositoryPath::Empty),
        query_digest: None,
        target_kind: TargetKind::Either,
    };
    let encoded = serde_json::to_string(&intent).unwrap();
    assert!(serde_json::from_str::<amiss_wire::controls::TargetIntent>(&encoded).is_err());
    assert_eq!(
        serde_json::from_str::<RepositoryTargetIntent>(&encoded).unwrap(),
        intent
    );
    for field in ["fragment_digest", "query_digest"] {
        let member = format!("\"{field}\":null");
        let invalid = encoded.replace(&member, &format!("\"{field}\":{{}}"));
        assert_ne!(invalid, encoded);
        assert!(serde_json::from_str::<RepositoryTargetIntent>(&invalid).is_err());
    }
    let invalid = encoded.replacen('{', "{\"commit_oid\":null,", 1);
    assert_ne!(invalid, encoded);
    assert_eq!(
        serde_json::from_str::<RepositoryTargetIntent>(&invalid).unwrap(),
        intent
    );
    assert!(
        serde_json::from_str::<FindingKeyScope>(
            r#"{"control_path":{},"kind":"control","rule_id":"rule"}"#
        )
        .is_err()
    );
}
