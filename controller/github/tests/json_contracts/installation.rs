use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::installation::InstallationToken;

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
