#[path = "json_contracts/branches.rs"]
mod branches;

#[path = "json_contracts/commits.rs"]
mod commits;

#[path = "json_contracts/numbers.rs"]
mod numbers;

#[path = "json_contracts/protection.rs"]
mod protection;

#[path = "json_contracts/pulls.rs"]
mod pulls;

#[path = "json_contracts/repositories.rs"]
mod repositories;

#[path = "json_contracts/refs.rs"]
mod refs;

#[path = "json_contracts/reviews.rs"]
mod reviews;

#[path = "json_contracts/statuses.rs"]
mod statuses;

#[path = "json_contracts/webhooks.rs"]
mod webhooks;

use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::user::{UserRecord, UserVisibility};

#[test]
fn user_profiles_preserve_both_provider_shapes_and_reject_projection_loss() {
    for (input, pronouns) in [
        (include_str!("fixtures/gitea-user.json"), None),
        (include_str!("fixtures/forgejo-user.json"), Some("")),
    ] {
        let (user, length): (UserRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(user.pronouns.as_deref(), pronouns);
        assert_eq!(user.id, 77);
        assert_eq!(user.username, user.login);
        assert_eq!(user.visibility, UserVisibility::Public);
        assert_eq!(user.last_login, "0001-01-01T00:00:00Z");
        for visibility in [
            UserVisibility::Public,
            UserVisibility::Limited,
            UserVisibility::Private,
        ] {
            let response = UserRecord {
                visibility,
                ..user.clone()
            };
            let encoded = serde_json::to_vec(&response).unwrap();
            assert_eq!(
                amiss_wire::read_json::<UserRecord>(&encoded, u64::MAX).unwrap(),
                response
            );
        }
        for (original, replacement) in [
            (r#""id": 77"#, r#""id": 77, "unknown": true"#),
            (r#""id": 77"#, r#""id": -1"#),
            (r#""id": 77"#, r#""id": 77, "\u0069d": 77"#),
            (r#""id": 77"#, r#""id": 9007199254740992"#),
            (r#""source_id": 0"#, r#""source_id": false"#),
            (r#""login_name": "","#, ""),
            (r#""username": "amiss-controller""#, r#""username": null"#),
            (r#""is_admin": false"#, r#""is_admin": 0"#),
            (r#""visibility": "public""#, r#""visibility": "unknown""#),
            (r#""followers_count": "#, r#""followers_count": -"#),
        ] {
            let invalid = input.replace(original, replacement);
            assert_ne!(invalid, input);
            assert_eq!(
                decode_bounded_json::<UserRecord, _>(
                    invalid.as_bytes(),
                    None,
                    invalid.len(),
                    |bytes| amiss_wire::read_json(bytes, u64::MAX)
                ),
                Err(ProviderError::InvalidResponse)
            );
        }
    }
    let input = include_str!("fixtures/forgejo-user.json");
    let invalid = input.replace(r#""pronouns": """#, r#""pronouns": null"#);
    assert_ne!(invalid, input);
    assert!(amiss_wire::read_json::<UserRecord>(invalid.as_bytes(), u64::MAX).is_err());
    for invalid in ["null", "true", "[]", "{}", "[{}]"] {
        assert!(amiss_wire::read_json::<UserRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
}
