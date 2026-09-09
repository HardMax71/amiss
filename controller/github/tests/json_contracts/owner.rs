use amiss_controller_github::owner::OwnerRecord;
use amiss_wire::assessment::Nullable;

const USER: &str = include_str!("../fixtures/owner-user.json");
const ORGANIZATION: &str = include_str!("../fixtures/owner-organization.json");

#[test]
fn owner_captures_keep_all_metadata_and_original_spellings() {
    for (input, login, id, kind) in [
        (USER, "HardMax71", 66_359_507, "User"),
        (ORGANIZATION, "github", 9_919, "Organization"),
    ] {
        let owner: OwnerRecord = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
        assert_eq!(owner.login, login);
        assert_eq!(u64::from(owner.id), id);
        assert_eq!(owner.kind, kind);
        assert_eq!(owner.user_view_type.as_deref(), Some("public"));
        assert!(owner.following_url.ends_with("{/other_user}"));
        assert!(owner.starred_url.ends_with("{/owner}{/repo}"));
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&owner).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
        );
        for trailing in [format!("{input} {{}}"), format!("{input} trailing")] {
            assert!(amiss_wire::read_json::<OwnerRecord>(trailing.as_bytes(), u64::MAX).is_err());
        }
        assert!(amiss_wire::read_json::<OwnerRecord>(input.as_bytes(), 0).is_err());
    }
}

#[test]
fn owner_presence_preserves_the_declared_nullability() {
    let mut owner: OwnerRecord = amiss_wire::read_json(USER.as_bytes(), u64::MAX).unwrap();
    for name in [
        None,
        Some(Nullable::Null),
        Some(Nullable::Value("A name".to_owned())),
    ] {
        for email in [
            None,
            Some(Nullable::Null),
            Some(Nullable::Value("docs@example.com".to_owned())),
        ] {
            owner.name.clone_from(&name);
            owner.email = email;
            owner.gravatar_id = Nullable::Null;
            owner.user_view_type = None;
            owner.starred_at = Some("2026-09-09T21:58:03Z".to_owned());
            let encoded = serde_json::to_vec(&owner).unwrap();
            assert_eq!(
                serde_json::from_slice::<OwnerRecord>(&encoded).unwrap(),
                owner
            );
            assert_eq!(
                amiss_wire::read_json::<OwnerRecord>(&encoded, u64::MAX).unwrap(),
                owner
            );
        }
    }
    owner.starred_at = None;
    let encoded = serde_json::to_string(&owner).unwrap();
    for field in ["starred_at", "user_view_type"] {
        let null = encoded.replacen('{', &format!(r#"{{"{field}":null,"#), 1);
        assert!(
            serde_json::from_str::<OwnerRecord>(&null).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<OwnerRecord>(null.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn owner_fields_are_required_and_closed_at_native_deserialization() {
    for field in [
        "\"login\":\"HardMax71\",",
        "\"id\":66359507,",
        "\"node_id\":\"MDQ6VXNlcjY2MzU5NTA3\",",
        "\"avatar_url\":\"https://avatars.githubusercontent.com/u/66359507?v=4\",",
        "\"gravatar_id\":\"\",",
        "\"url\":\"https://api.github.com/users/HardMax71\",",
        "\"html_url\":\"https://github.com/HardMax71\",",
        "\"followers_url\":\"https://api.github.com/users/HardMax71/followers\",",
        "\"following_url\":\"https://api.github.com/users/HardMax71/following{/other_user}\",",
        "\"gists_url\":\"https://api.github.com/users/HardMax71/gists{/gist_id}\",",
        "\"starred_url\":\"https://api.github.com/users/HardMax71/starred{/owner}{/repo}\",",
        "\"subscriptions_url\":\"https://api.github.com/users/HardMax71/subscriptions\",",
        "\"organizations_url\":\"https://api.github.com/users/HardMax71/orgs\",",
        "\"repos_url\":\"https://api.github.com/users/HardMax71/repos\",",
        "\"events_url\":\"https://api.github.com/users/HardMax71/events{/privacy}\",",
        "\"received_events_url\":\"https://api.github.com/users/HardMax71/received_events\",",
        "\"type\":\"User\",",
        "\"site_admin\":false,",
    ] {
        assert_eq!(USER.matches(field).count(), 1, "{field}");
        let missing = USER.replacen(field, "", 1);
        assert!(
            serde_json::from_str::<OwnerRecord>(&missing).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<OwnerRecord>(missing.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        ("\"login\":", "\"extra\":true,\"login\":"),
        ("\"login\":", "\"\\u006cogin\":null,\"login\":"),
        ("\"login\":\"HardMax71\"", "\"login\":null"),
        ("\"gravatar_id\":\"\"", "\"gravatar_id\":{}"),
        ("\"site_admin\":false", "\"site_admin\":\"false\""),
        ("\"id\":66359507", "\"id\":-1"),
        ("\"id\":66359507", "\"id\":1.5"),
        ("\"id\":66359507", "\"id\":9007199254740992"),
        ("\"user_view_type\":\"public\"", "\"user_view_type\":[]"),
    ] {
        assert_eq!(USER.matches(old).count(), 1, "{old}");
        let changed = USER.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<OwnerRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(amiss_wire::read_json::<OwnerRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    let largest = USER.replacen("\"id\":66359507", "\"id\":9007199254740991", 1);
    let largest: OwnerRecord = amiss_wire::read_json(largest.as_bytes(), u64::MAX).unwrap();
    assert_eq!(u64::from(largest.id), 9_007_199_254_740_991);
}
