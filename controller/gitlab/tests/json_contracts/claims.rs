use amiss_controller_gitlab::claims::{Claims, Protection, UserIdentity};

#[test]
fn policy_claims_retain_the_complete_provider_payload() {
    let claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    assert_eq!(claims.project_id, 101);
    assert_eq!(claims.project_path, "acme/widget");
    assert_eq!(claims.namespace_id, 72);
    assert_eq!(claims.namespace_path, "acme");
    assert_eq!(claims.job_namespace_id, 72);
    assert_eq!(claims.job_namespace_path, "acme");
    assert_eq!(claims.user_id, "1");
    assert_eq!(claims.user_login.as_deref(), Some("reviewer"));
    assert_eq!(claims.user_email.as_deref(), Some("reviewer@example.com"));
    assert_eq!(claims.user_access_level.as_deref(), Some("maintainer"));
    assert_eq!(claims.branch, "topic");
    assert_eq!(claims.ref_type, "branch");
    assert_eq!(claims.ref_path, "refs/heads/topic");
    assert_eq!(claims.ref_protected, Protection::Unprotected);
    assert_eq!(claims.ci_config_ref_uri, None);
    assert_eq!(claims.ci_config_sha, None);
    assert_eq!(claims.project_visibility, "private");
    assert_eq!(
        claims.target_audience.as_deref(),
        Some("https://service.example")
    );
    assert_eq!(
        claims.user_identities,
        Some(vec![UserIdentity {
            provider: "github".to_owned(),
            extern_uid: "42".to_owned(),
        }])
    );
    assert_eq!(claims.groups_direct, Some(vec!["acme".to_owned()]));
    assert_eq!(claims.environment.as_deref(), Some("production"));
    assert_eq!(claims.environment_protected, Some(Protection::Protected));
    assert_eq!(claims.deployment_tier.as_deref(), Some("production"));
    assert_eq!(claims.environment_action.as_deref(), Some("start"));

    let encoded = serde_json::to_vec(&claims).unwrap();
    assert_eq!(serde_json::from_slice::<Claims>(&encoded).unwrap(), claims);
    let encoded = std::str::from_utf8(&encoded).unwrap();
    for field in [
        r#""ref_protected":"false""#,
        r#""environment_protected":"true""#,
        r#""ci_config_ref_uri":null"#,
        r#""ci_config_sha":null"#,
    ] {
        assert!(encoded.contains(field), "{field}");
    }
}

#[test]
fn policy_claims_distinguish_nullable_fields_from_optional_fields() {
    let mut claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    claims.user_id.clear();
    claims.user_login = None;
    claims.user_email = None;
    claims.user_access_level = None;
    claims.target_audience = None;
    claims.user_identities = None;
    claims.groups_direct = None;
    claims.environment = None;
    claims.environment_protected = None;
    claims.deployment_tier = None;
    claims.environment_action = None;
    let encoded = serde_json::to_string(&claims).unwrap();
    assert_eq!(serde_json::from_str::<Claims>(&encoded).unwrap(), claims);

    for field in [
        "user_login",
        "user_email",
        "user_access_level",
        "ci_config_ref_uri",
        "ci_config_sha",
    ] {
        let present = format!("\"{field}\":null,");
        let missing = encoded.replace(&present, "");
        assert_ne!(missing, encoded, "{field}");
        assert!(serde_json::from_str::<Claims>(&missing).is_err(), "{field}");
    }
    for field in [
        "target_audience",
        "user_identities",
        "groups_direct",
        "environment",
        "environment_protected",
        "deployment_tier",
        "environment_action",
    ] {
        assert!(!encoded.contains(&format!("\"{field}\":")), "{field}");
        let invalid = encoded.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(serde_json::from_str::<Claims>(&invalid).is_err(), "{field}");
    }
}

#[test]
fn policy_claims_refuse_unknown_duplicate_and_malformed_fields() {
    let input = std::str::from_utf8(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    for (original, replacement) in [
        ("{\n", "{\n\"future\":true,"),
        ("\"job_config\": {", "\"job_config\": {\"future\":true,"),
        (
            "\"provider\": \"github\"",
            "\"future\":true,\"provider\":\"github\"",
        ),
        ("\"iss\":", "\"iss\":\"duplicate\",\"iss\":"),
        ("\"iss\":", "\"i\\u0073s\":\"duplicate\",\"iss\":"),
        ("\"url\":", "\"url\":\"duplicate\",\"url\":"),
        (
            "\"extern_uid\":",
            "\"extern_uid\":\"duplicate\",\"extern_uid\":",
        ),
        ("\"ref_protected\": \"false\"", "\"ref_protected\": false"),
        (
            "\"ref_protected\": \"false\"",
            "\"ref_protected\": {\"false\":null}",
        ),
        (
            "\"ref_protected\": \"false\"",
            "\"ref_protected\": \"unknown\"",
        ),
        (
            "\"environment_protected\": \"true\"",
            "\"environment_protected\": true",
        ),
        (
            "\"environment_protected\": \"true\"",
            "\"environment_protected\": {\"true\":null}",
        ),
        (
            "\"environment_protected\": \"true\"",
            "\"environment_protected\": \"unknown\"",
        ),
        ("\"user_id\": \"1\",", ""),
        ("\"project_id\": \"101\",", ""),
        ("\"namespace_id\": \"72\",", ""),
        ("\"job_namespace_id\": \"72\",", ""),
    ] {
        let invalid = input.replacen(original, replacement, 1);
        assert_ne!(invalid, input, "{original}");
        assert!(
            serde_json::from_str::<Claims>(&invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn policy_identifiers_and_audiences_use_the_library_representations() {
    let input = std::str::from_utf8(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    let expected: Claims = serde_json::from_str(input).unwrap();
    let numeric = serde_json::to_string(&expected).unwrap();
    for (field, value) in [
        ("job_project_id", 101),
        ("pipeline_id", 202),
        ("job_id", 303),
        ("runner_id", 77),
        ("project_id", 101),
        ("namespace_id", 72),
        ("job_namespace_id", 72),
    ] {
        assert!(numeric.contains(&format!("\"{field}\":{value}")));
        let original = format!("\"{field}\": \"{value}\"");
        let native = input.replace(&original, &format!("\"{field}\":{value}"));
        assert_ne!(native, input);
        assert_eq!(serde_json::from_str::<Claims>(&native).unwrap(), expected);
        for value in [
            "null",
            "true",
            "[]",
            "{}",
            "-1",
            "1.5",
            "18446744073709551616",
            "\"-1\"",
            "\"1.5\"",
            "\"invalid\"",
        ] {
            let invalid = input.replace(&original, &format!("\"{field}\":{value}"));
            assert!(
                serde_json::from_str::<Claims>(&invalid).is_err(),
                "{field}: {value}"
            );
        }
    }
    let list = input.replace(
        "\"aud\": \"amiss-controller\"",
        "\"aud\": [\"amiss-controller\",\"another-service\"]",
    );
    let mut expected = expected;
    expected.aud.push("another-service".to_owned());
    assert_eq!(serde_json::from_str::<Claims>(&list).unwrap(), expected);
}
