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
use amiss_controller_gitea::user::UserRecord;

#[test]
fn user_profiles_keep_only_the_consumed_identity() {
    for input in [
        include_str!("fixtures/gitea-user.json"),
        include_str!("fixtures/forgejo-user.json"),
    ] {
        let (user, length): (UserRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(user.id, 77);
        assert_eq!(user.login, "amiss-controller");
        for encoded in [
            serde_json::to_vec(&user).unwrap(),
            serde_json::to_vec(&(user.id, &user.login)).unwrap(),
        ] {
            assert_eq!(
                serde_json::from_slice::<UserRecord>(&encoded).unwrap(),
                user
            );
        }
        for (old, new) in [
            (
                r#""id": 77"#,
                r#""id": 77, "extra": {"future": [null,true]}"#,
            ),
            (r#""source_id": 0"#, r#""source_id": false"#),
            (r#""login_name": "","#, ""),
            (r#""visibility": "public""#, r#""visibility": "future""#),
            (r#""username": "amiss-controller""#, r#""username": null"#),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert_eq!(serde_json::from_str::<UserRecord>(&changed).unwrap(), user);
        }
        for (original, replacement) in [
            (r#""id": 77"#, r#""id": -1"#),
            (r#""id": 77"#, r#""id": 77, "\u0069d": 77"#),
            (r#""id": 77"#, r#""id": 9007199254740992"#),
            (r#""id": 77,"#, ""),
            (r#""login": "amiss-controller","#, ""),
            (r#""login": "amiss-controller""#, r#""login": null"#),
            (r#""login":"#, r#""login": "other", "login":"#),
        ] {
            let invalid = input.replace(original, replacement);
            assert_ne!(invalid, input);
            assert_eq!(
                decode_bounded_json::<UserRecord, _>(
                    invalid.as_bytes(),
                    None,
                    invalid.len(),
                    |bytes| serde_json::from_slice(bytes)
                ),
                Err(ProviderError::InvalidResponse)
            );
        }
        assert_eq!(
            decode_bounded_json::<UserRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| { serde_json::from_slice(bytes) }
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    for invalid in ["null", "true", "[]", "{}", "[{}]"] {
        assert!(serde_json::from_str::<UserRecord>(invalid).is_err());
    }
}
