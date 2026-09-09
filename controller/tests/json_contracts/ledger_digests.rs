use amiss_controller::{ArtifactReference, CheckBinding};

#[test]
fn check_bindings_keep_the_stored_layout_and_validate_digests() {
    let digest = "a".repeat(64);
    let encoded = format!(
        r#"{{"plan_digest":"sha256:{digest}","required_status_name":"amiss/enforce","execution_constraint_digest":"sha256:{digest}"}}"#
    );
    let check: CheckBinding = serde_json::from_str(&encoded).unwrap();
    assert_eq!(serde_json::to_string(&check).unwrap(), encoded);
    for field in ["plan_digest", "execution_constraint_digest"] {
        for invalid in ["sha256!", "sha256:A", "sha256:"] {
            let mutation = encoded.replace(
                &format!(r#""{field}":"sha256:{digest}""#),
                &format!(r#""{field}":"{invalid}""#),
            );
            assert_ne!(mutation, encoded);
            assert!(serde_json::from_str::<CheckBinding>(&mutation).is_err());
        }
    }
    for mutation in [
        encoded.replacen('{', r#"{"unknown":false,"#, 1),
        encoded.replace(r#""required_status_name""#, r#""other""#),
        encoded.replace(r#""amiss/enforce""#, "null"),
    ] {
        assert!(serde_json::from_str::<CheckBinding>(&mutation).is_err());
    }
}

#[test]
fn artifact_references_keep_the_stored_layout_and_closed_fields() {
    let digest = "a".repeat(64);
    for optional in [false, true] {
        let assessment = if optional {
            format!(
                r#","assessment_digest":"sha256:{digest}","external_tally":{{"refuted":0,"unproven":0,"reachable":0}}"#
            )
        } else {
            String::new()
        };
        let encoded = format!(
            r#"{{"id":"{digest}","locator":"https://amiss.example/artifacts/{digest}/report","expires_at_unix_millis":2000,"report_digest":"sha256:{digest}"{assessment},"external_incomplete":false,"semantic_digest":"sha256:{digest}"}}"#
        );
        let artifact: ArtifactReference = serde_json::from_str(&encoded).unwrap();
        assert_eq!(serde_json::to_string(&artifact).unwrap(), encoded);
        for field in ["report_digest", "semantic_digest"]
            .into_iter()
            .chain(optional.then_some("assessment_digest"))
        {
            let mutation = encoded.replace(
                &format!(r#""{field}":"sha256:{digest}""#),
                &format!(r#""{field}":"sha256!{digest}""#),
            );
            assert_ne!(mutation, encoded);
            assert!(serde_json::from_str::<ArtifactReference>(&mutation).is_err());
        }
        for mutation in [
            encoded.replacen('{', r#"{"unknown":false,"#, 1),
            encoded.replace(r#""report_digest""#, r#""other""#),
            encoded.replace(
                r#""expires_at_unix_millis":2000"#,
                r#""expires_at_unix_millis":null"#,
            ),
        ] {
            assert!(serde_json::from_str::<ArtifactReference>(&mutation).is_err());
        }
        let without_semantic =
            encoded.replace(&format!(r#","semantic_digest":"sha256:{digest}""#), "");
        let omitted = without_semantic.replace(r#","external_incomplete":false"#, "");
        let restored: ArtifactReference = serde_json::from_str(&omitted).unwrap();
        assert_eq!(restored.semantic_digest, None);
        assert!(!restored.external_incomplete);
        assert_eq!(serde_json::to_string(&restored).unwrap(), without_semantic);
    }
}
