use amiss_wire::controls::TargetKind;
use amiss_wire::report::model::{
    EmptyRepositoryPath, FindingKeyScope, ReportEnvelope, RepositoryIntentKind,
    RepositoryIntentPath, RepositoryTargetIntent,
};

#[test]
fn finding_keys_require_closed_scope_tags() {
    let report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.frozen-1.json"
    ))
    .unwrap();
    let finding = report
        .payload
        .findings
        .iter()
        .find(|finding| matches!(finding.key_input.scope, FindingKeyScope::Reference { .. }))
        .unwrap();
    let key = &finding.key_input;
    let scope = serde_json::to_string(&key.scope).unwrap();
    let wrong_tag = scope.replace("\"kind\":\"reference\"", "\"kind\":\"document\"");
    assert_ne!(wrong_tag, scope);
    assert!(serde_json::from_str::<FindingKeyScope>(&wrong_tag).is_err());
}

#[test]
fn finding_key_intents_preserve_required_nullable_fields() {
    let intent: RepositoryTargetIntent = RepositoryTargetIntent {
        commit_oid: None,
        fragment_digest: None,
        kind: RepositoryIntentKind::RepositoryPath,
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
        for invalid in [
            encoded.replace(&member, &format!("\"{field}\":{{}}")),
            encoded.replace(&format!("{member},"), ""),
        ] {
            assert_ne!(invalid, encoded);
            assert!(serde_json::from_str::<RepositoryTargetIntent>(&invalid).is_err());
        }
    }
    let invalid = encoded.replacen('{', "{\"commit_oid\":null,", 1);
    assert_ne!(invalid, encoded);
    assert!(serde_json::from_str::<RepositoryTargetIntent>(&invalid).is_err());
    for invalid in [
        r#"{"kind":"control","rule_id":"rule"}"#,
        r#"{"control_path":{},"kind":"control","rule_id":"rule"}"#,
    ] {
        assert!(serde_json::from_str::<FindingKeyScope>(invalid).is_err());
    }
}
