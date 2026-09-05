use amiss_scan::evaluate::FINDING_KEY_DOMAIN;
use amiss_scan::policy::ControlSeed;
use amiss_wire::json;

use super::*;

#[test]
fn reference_keys_preserve_normalization_and_optional_identity_fields() {
    for (path, path_json) in [
        (None, r#""""#),
        (Some(repo_path("dir/é.md")), r#""dir/é.md""#),
        (
            RepoPath::from_bytes(b"dir/\xff.md".to_vec()),
            r#"{"bytes_hex":"6469722fff2e6d64"}"#,
        ),
    ] {
        for (oid, commit_json) in [
            (None, String::new()),
            (
                Oid::new(ObjectFormat::Sha1, "a".repeat(40)),
                format!(r#""commit_oid":"{}","#, "a".repeat(40)),
            ),
            (
                Oid::new(ObjectFormat::Sha256, "b".repeat(64)),
                format!(r#""commit_oid":"{}","#, "b".repeat(64)),
            ),
        ] {
            for (target_kind, expected_kind) in [
                (None, "either"),
                (Some(TargetKind::Either), "either"),
                (Some(TargetKind::Blob), "blob"),
                (Some(TargetKind::Tree), "tree"),
            ] {
                let mut candidate = observation(&missing_spec("d.md", "absent.md"));
                candidate.intent.repository_path = path.clone();
                candidate.intent.commit_oid = oid.clone();
                candidate.intent.target_kind = target_kind;
                candidate.intent.query = Some("mode=raw".to_owned());
                candidate.intent.fragment = Some("heading".to_owned());
                let expected = format!(
                    r#"{{"finding_kind":"explicit-target-missing","schema":"amiss/scanner-finding-key-input","scope":{{"document":"d.md","kind":"reference","normalized_target_intent":{{{commit_json}"fragment_digest":"{}","kind":"repository-path","path":{path_json},"query_digest":"{}","target_kind":"{expected_kind}"}},"occurrence":{{"kind":"source-projection","source_projection_digest":"{}"}},"source_construct":"markdown-inline-link"}}}}"#,
                    hb("amiss/scanner-link-fragment", b"heading"),
                    hb("amiss/scanner-link-query", b"mode=raw"),
                    candidate.projection_digest,
                );
                let findings = evaluate(
                    &[],
                    &comparisons(Vec::new(), vec![candidate]),
                    Profile::Observe,
                );
                let finding = only(findings, FindingKind::ExplicitTargetMissing);
                let expected = json::parse(expected.as_bytes()).expect("key fixture");
                assert_eq!(finding.key().digest(), hj(FINDING_KEY_DOMAIN, &expected));
            }
        }
    }
}

#[test]
fn nonreference_keys_preserve_document_observation_and_control_scopes() {
    let candidate = observation(&spec(
        "d.md",
        "t.md",
        Resolution::Invalid(InvalidReference::PathTraversal),
    ));
    let observation_id = candidate.id;
    let policy = Effects {
        controls: vec![
            ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: "rule".to_owned(),
                control_path: None,
            },
            ControlSeed {
                kind: FindingKind::PolicyWeakened,
                rule_id: "rule".to_owned(),
                control_path: Some(repo_path("config.json")),
            },
        ],
        ..Effects::default()
    };
    let (findings, errors) = evaluate_with_policy(
        &[DocumentInput {
            path: repo_path("gone.md"),
            base: Some(DocumentSide::Unsupported),
            candidate: None,
        }],
        &comparisons(Vec::new(), vec![candidate]),
        Profile::Observe,
        &policy,
        &[],
        &[],
    );
    assert!(errors.is_empty());
    let mut expected = [
        ("document-removed", r#"{"document":"gone.md","kind":"document"}"#.to_owned()),
        ("invalid-reference", format!(r#"{{"kind":"observation","observation_id":"{observation_id}"}}"#)),
        ("policy-weakened", r#"{"control_path":null,"kind":"control","rule_id":"rule"}"#.to_owned()),
        ("policy-weakened", r#"{"control_path":"config.json","kind":"control","rule_id":"rule"}"#.to_owned()),
    ].map(|(kind, scope)| {
        let input = format!(r#"{{"finding_kind":"{kind}","schema":"amiss/scanner-finding-key-input","scope":{scope}}}"#);
        hj(FINDING_KEY_DOMAIN, &json::parse(input.as_bytes()).expect("key fixture"))
    });
    expected.sort_unstable();
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.key().digest())
            .collect::<Vec<_>>(),
        expected
    );
}
