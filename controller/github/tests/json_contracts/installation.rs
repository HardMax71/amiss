use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::installation::InstallationToken;
use amiss_controller_github::installation::permissions::AppPermissions;

#[test]
fn installation_token_input_requires_only_the_consumed_token() {
    for input in [
        r#"{"token":"synthetic-installation-token"}"#,
        include_str!("../fixtures/installation-token.json"),
        r#"{"token":"synthetic-installation-token","permissions":{"future_scope":true},"expires_at":null,"repositories":[{"future":1.5}]}"#,
    ] {
        let (record, length): (InstallationToken, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(record.token, "synthetic-installation-token");
        assert_eq!(
            serde_json::to_string(&record).unwrap(),
            r#"{"token":"synthetic-installation-token"}"#
        );
        assert!(matches!(
            decode_bounded_json::<InstallationToken, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        ));
    }
}

#[test]
fn installation_token_input_rejects_invalid_consumed_fields() {
    for input in [
        "{}",
        r#"{"token":null}"#,
        r#"{"token":3}"#,
        r#"{"token":{}}"#,
        r#"{"token":"one","token":"two"}"#,
        r#"{"token":"one","\u0074oken":"two"}"#,
        r#"{"token":"one"} {}"#,
        r#"{"token":"one","ignored":[}"#,
    ] {
        assert!(
            serde_json::from_str::<InstallationToken>(input).is_err(),
            "{input}"
        );
    }
    let native: InstallationToken = serde_json::from_str(r#"["synthetic"]"#).unwrap();
    assert_eq!(native.token, "synthetic");
}

#[test]
fn installation_permissions_only_accept_their_declared_levels() {
    for (field, accepted, rejected) in [
        ("contents", &["read", "write"][..], &["admin"][..]),
        (
            "enterprise_custom_properties_for_organizations",
            &["read", "write", "admin"][..],
            &[][..],
        ),
        (
            "organization_custom_properties",
            &["read", "write", "admin"][..],
            &[][..],
        ),
        (
            "organization_projects",
            &["read", "write", "admin"][..],
            &[][..],
        ),
        (
            "repository_projects",
            &["read", "write", "admin"][..],
            &[][..],
        ),
        (
            "organization_events",
            &["read"][..],
            &["write", "admin"][..],
        ),
        ("organization_plan", &["read"][..], &["write", "admin"][..]),
        ("profile", &["write"][..], &["read", "admin"][..]),
        ("workflows", &["write"][..], &["read", "admin"][..]),
    ] {
        for level in accepted {
            let input = format!("{{\"{field}\":\"{level}\"}}");
            let permissions: AppPermissions = serde_json::from_str(&input).unwrap();
            assert_eq!(serde_json::to_string(&permissions).unwrap(), input);
            assert_eq!(
                amiss_wire::read_json::<AppPermissions>(input.as_bytes(), u64::MAX).unwrap(),
                permissions
            );
        }
        for level in rejected.iter().chain(["unknown", "READ", "WRITE"].iter()) {
            let input = format!("{{\"{field}\":\"{level}\"}}");
            assert!(
                serde_json::from_str::<AppPermissions>(&input).is_err(),
                "{input}"
            );
            assert!(amiss_wire::read_json::<AppPermissions>(input.as_bytes(), u64::MAX).is_err());
        }
        for level in ["null", "false", "1", "[]", r#"{"read":null}"#] {
            let input = format!("{{\"{field}\":{level}}}");
            assert!(
                serde_json::from_str::<AppPermissions>(&input).is_err(),
                "{input}"
            );
            assert!(amiss_wire::read_json::<AppPermissions>(input.as_bytes(), u64::MAX).is_err());
        }
    }
    let empty: AppPermissions = AppPermissions::default();
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
}
