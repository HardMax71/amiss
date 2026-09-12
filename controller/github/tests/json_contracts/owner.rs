use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::owner::OwnerRecord;

#[test]
fn owner_captures_keep_consumed_logins_without_changing_spelling() {
    for (input, login) in [
        (include_str!("../fixtures/owner-user.json"), "HardMax71"),
        (
            include_str!("../fixtures/owner-organization.json"),
            "github",
        ),
    ] {
        let (owner, length): (OwnerRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(owner.login, login);
        let encoded = serde_json::to_vec(&owner).unwrap();
        assert_eq!(
            serde_json::from_slice::<OwnerRecord>(&encoded).unwrap(),
            owner
        );
        assert_eq!(
            decode_bounded_json::<OwnerRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse),
        );
        assert!(serde_json::from_str::<OwnerRecord>(&format!("{input} null")).is_err());
    }
}

#[test]
fn owner_metadata_does_not_become_an_input_requirement() {
    let owner = OwnerRecord {
        login: "HardMax71".to_owned(),
    };
    for input in [
        r#"{"login":"HardMax71"}"#,
        r#"{"login":"HardMax71","id":null,"name":{},"email":false,"site_admin":[]}"#,
        r#"{"login":"HardMax71","type":"future","user_view_type":{},"unknown":[null]}"#,
        r#"{"login":"HardMax71","avatar_url":42,"gravatar_id":true,"node_id":[]}"#,
    ] {
        assert_eq!(serde_json::from_str::<OwnerRecord>(input).unwrap(), owner);
    }
}

#[test]
fn consumed_owner_logins_are_required_typed_and_unique() {
    for input in [
        "{}",
        r#"{"id":1}"#,
        r#"{"login":null}"#,
        r#"{"login":false}"#,
        r#"{"login":1}"#,
        r#"{"login":[]}"#,
        r#"{"login":{}}"#,
        r#"{"login":"owner","login":"other"}"#,
        r#"{"login":"owner","\u006cogin":"other"}"#,
    ] {
        assert!(
            serde_json::from_str::<OwnerRecord>(input).is_err(),
            "{input}"
        );
    }
}
