use amiss_controller_gitlab::claims::Claims;

#[test]
fn policy_claims_retain_verified_facts_and_ignore_metadata() {
    let claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    assert_eq!(claims.job_project_id, 101);
    assert_eq!(claims.job_project_path, "acme/widget");
    assert_eq!(
        (claims.pipeline_id, claims.job_id, claims.runner_id),
        (202, 303, 77)
    );
    assert_eq!(claims.aud, ["amiss-controller"]);
    assert_eq!(claims.job_source, "pipeline_execution_policy");
    let encoded = serde_json::to_string(&claims).unwrap();
    let metadata = encoded
        .replacen('{', r#"{"project_id":false,"namespace_path":42,"user_login":[],"environment_protected":{},"user_identities":null,"extra":{},"#, 1)
        .replacen(r#""job_config":{"#, r#""job_config":{"future":[null,true],"#, 1);
    assert_eq!(serde_json::from_str::<Claims>(&metadata).unwrap(), claims);
}

#[test]
fn policy_identity_time_and_configuration_fields_remain_required() {
    let claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    let encoded = serde_json::to_string(&claims).unwrap();
    for (field, value) in [
        ("iss", serde_json::to_string(&claims.iss).unwrap()),
        ("sub", serde_json::to_string(&claims.sub).unwrap()),
        ("aud", serde_json::to_string(&claims.aud[0]).unwrap()),
        ("exp", claims.exp.to_string()),
        ("nbf", claims.nbf.to_string()),
        ("iat", claims.iat.to_string()),
        ("jti", serde_json::to_string(&claims.jti).unwrap()),
        ("job_project_id", claims.job_project_id.to_string()),
        (
            "job_project_path",
            serde_json::to_string(&claims.job_project_path).unwrap(),
        ),
        ("pipeline_id", claims.pipeline_id.to_string()),
        (
            "pipeline_source",
            serde_json::to_string(&claims.pipeline_source).unwrap(),
        ),
        ("job_id", claims.job_id.to_string()),
        ("runner_id", claims.runner_id.to_string()),
        (
            "runner_environment",
            serde_json::to_string(&claims.runner_environment).unwrap(),
        ),
        ("sha", serde_json::to_string(&claims.sha).unwrap()),
        (
            "job_source",
            serde_json::to_string(&claims.job_source).unwrap(),
        ),
        (
            "job_config",
            serde_json::to_string(&claims.job_config).unwrap(),
        ),
        (
            "url",
            serde_json::to_string(&claims.job_config.url).unwrap(),
        ),
        (
            "sha",
            serde_json::to_string(&claims.job_config.sha).unwrap(),
        ),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":null"#),
            format!(r#""{field}":false"#),
            format!(r#""{field}":-1"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            amiss_fixtures::assert_json_rejections::<Claims>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
}

#[test]
fn policy_claims_refuse_duplicates_and_malformed_consumed_fields() {
    let input = std::str::from_utf8(amiss_fixtures::GITLAB_POLICY_CLAIMS).unwrap();
    amiss_fixtures::assert_json_rejections::<Claims>(
        input,
        &[
            (r#""iss":"#, r#""i\u0073s":"duplicate","iss":"#),
            (r#""url":"#, r#""url":"duplicate","url":"#),
            (r#""aud": "amiss-controller""#, r#""aud": [42]"#),
        ],
    );
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
