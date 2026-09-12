use amiss_wire::controls::TargetKind;
use amiss_wire::report::model::{
    EmptyRepositoryPath, Finding, FindingFactInput, FindingKeyScope, ReportEnvelope,
    RepositoryIntentKind, RepositoryIntentPath, RepositoryTargetIntent,
};

#[test]
fn finding_keys_require_objects_and_closed_scopes() {
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
    let FindingKeyScope::Reference {
        normalized_target_intent: intent,
        occurrence,
        ..
    } = &key.scope
    else {
        panic!("selected reference key");
    };
    let encoded = serde_json::to_string(&report).unwrap();
    for (object, sequence) in [
        (
            serde_json::to_string(intent).unwrap(),
            serde_json::to_string(&(
                &intent.commit_oid,
                intent.fragment_digest,
                intent.kind,
                &intent.path,
                intent.query_digest,
                intent.target_kind,
            ))
            .unwrap(),
        ),
        (
            serde_json::to_string(occurrence).unwrap(),
            serde_json::to_string(&(occurrence.kind, occurrence.source_projection_digest)).unwrap(),
        ),
    ] {
        let altered = encoded.replace(&object, &sequence);
        assert_ne!(altered, encoded);
        assert!(serde_json::from_str::<ReportEnvelope>(&altered).is_err());
    }
    let mut single_finding = finding.clone();
    single_finding.base_fact = None;
    single_finding.candidate_fact = None;
    let encoded_finding = serde_json::to_string(&single_finding).unwrap();
    let key_object = serde_json::to_string(key).unwrap();
    let key_sequence = serde_json::to_string(&(key.finding_kind, key.schema, &key.scope)).unwrap();
    let invalid = encoded_finding.replace(&key_object, &key_sequence);
    assert_ne!(invalid, encoded_finding);
    assert!(serde_json::from_str::<Finding>(&invalid).is_err());
    assert!(finding.base_fact.is_some() || finding.candidate_fact.is_some());
    for fact in finding.base_fact.iter().chain(&finding.candidate_fact) {
        let encoded_fact = serde_json::to_string(fact).unwrap();
        let fact_key = &fact.key_input;
        let invalid = encoded_fact.replace(
            &serde_json::to_string(fact_key).unwrap(),
            &serde_json::to_string(&(fact_key.finding_kind, fact_key.schema, &fact_key.scope))
                .unwrap(),
        );
        assert_ne!(invalid, encoded_fact);
        assert!(serde_json::from_str::<FindingFactInput>(&invalid).is_err());
    }
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
