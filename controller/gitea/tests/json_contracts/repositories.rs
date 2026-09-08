use std::collections::BTreeMap;

use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::repository::{
    ExternalTracker, ExternalWiki, Organization, PermissionLevel, RepositoryRecord,
    RepositoryTransfer, RepositoryUnit, Team, TrackerStyle,
};
use amiss_controller_gitea::user::UserVisibility;
use amiss_wire::assessment::Nullable;
use amiss_wire::model::ObjectFormat;

#[test]
fn repository_captures_keep_omission_null_and_nested_parents_distinct() {
    for (input, fork, omitted) in [
        (
            include_bytes!("../fixtures/gitea-repository.json").as_slice(),
            false,
            true,
        ),
        (
            include_bytes!("../fixtures/forgejo-repository.json").as_slice(),
            false,
            false,
        ),
        (
            include_bytes!("../fixtures/gitea-fork.json").as_slice(),
            true,
            false,
        ),
    ] {
        let (repository, length): (RepositoryRecord, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(repository.object_format_name, ObjectFormat::Sha1);
        assert_eq!(repository.parent.is_none(), omitted);
        assert_eq!(matches!(&repository.parent, Some(Nullable::Value(_))), fork);
        assert_eq!(repository.fork, fork);
        if let Some(Nullable::Value(parent)) = &repository.parent {
            assert_eq!(parent.full_name, "acme/widget");
            assert_eq!(parent.id, 101);
        }
        let encoded = serde_json::to_vec(&repository).unwrap();
        assert_eq!(
            amiss_wire::read_json::<RepositoryRecord>(&encoded, u64::MAX).unwrap(),
            repository
        );
    }
}

#[test]
fn transfers_and_repository_settings_are_closed_typed_data() {
    let mut repository: RepositoryRecord = amiss_wire::read_json(
        include_bytes!("../fixtures/gitea-repository.json"),
        u64::MAX,
    )
    .unwrap();
    repository.external_tracker = Some(ExternalTracker {
        external_tracker_url: "https://tracker.example".to_owned(),
        external_tracker_format: "https://tracker.example/{user}/{repo}/{index}".to_owned(),
        external_tracker_style: TrackerStyle::Numeric,
        external_tracker_regexp_pattern: String::new(),
    });
    repository.external_wiki = Some(ExternalWiki {
        external_wiki_url: "https://wiki.example".to_owned(),
    });
    repository.licenses = Some(Nullable::Null);
    repository.topics = None;
    repository.repo_transfer = Some(Nullable::Value(RepositoryTransfer {
        doer: Some(repository.owner.clone()),
        recipient: None,
        teams: Some(vec![Team {
            id: 42,
            name: "reviewers".to_owned(),
            description: String::new(),
            organization: Some(Organization {
                id: 12,
                name: "acme".to_owned(),
                full_name: "Fixture organization".to_owned(),
                email: "owner@example.com".to_owned(),
                avatar_url: "https://forge.example/avatars/acme".to_owned(),
                description: String::new(),
                website: "https://example.com".to_owned(),
                location: String::new(),
                visibility: UserVisibility::Public,
                repo_admin_change_team_access: false,
                username: "acme".to_owned(),
                created: Some("2026-01-01T00:00:00Z".to_owned()),
            }),
            includes_all_repositories: false,
            permission: PermissionLevel::Read,
            units: Some(vec![RepositoryUnit::Code]),
            units_map: Some(BTreeMap::from([(
                RepositoryUnit::Code,
                PermissionLevel::Read,
            )])),
            can_create_org_repo: false,
            visibility: Some(UserVisibility::Private),
        }]),
    }));
    let input = serde_json::to_string(&repository).unwrap();
    assert_eq!(
        amiss_wire::read_json::<RepositoryRecord>(input.as_bytes(), u64::MAX).unwrap(),
        repository
    );
    for (original, replacement) in [
        (
            r#""external_tracker":{"#,
            r#""external_tracker":{"unknown":true,"#,
        ),
        (
            r#""external_wiki":{"#,
            r#""external_wiki":{"unknown":true,"#,
        ),
        (
            r#""repo_transfer":{"#,
            r#""repo_transfer":{"unknown":true,"#,
        ),
        (r#""teams":[{"#, r#""teams":[{"unknown":true,"#),
        (r#""organization":{"#, r#""organization":{"unknown":true,"#),
        (r#""repo.code":"read""#, r#""repo.unknown":"read""#),
        (r#""repo.code":"read""#, r#""repo.code":"unknown""#),
        (
            r#""external_tracker_style":"numeric""#,
            r#""external_tracker_style":"unknown""#,
        ),
        (r#""permission":"read""#, r#""permission":"unknown""#),
        (r#""recipient":null,"#, ""),
    ] {
        let invalid = input.replace(original, replacement);
        assert_ne!(invalid, input);
        assert!(amiss_wire::read_json::<RepositoryRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn malformed_repositories_cannot_prove_visibility() {
    let input = include_str!("../fixtures/gitea-repository.json");
    for (original, replacement) in [
        (r#""id":101"#, r#""id":101,"unknown":true"#),
        (r#""id":101"#, r#""id":-1"#),
        (r#""id":101"#, r#""id":9007199254740992"#),
        (r#""id":101"#, r#""id":101,"\u0069d":101"#),
        (
            r#""object_format_name":"sha1""#,
            r#""object_format_name":"unknown""#,
        ),
        (
            r#""default_merge_style":"squash""#,
            r#""default_merge_style":"unknown""#,
        ),
        (
            r#""default_update_style":"merge""#,
            r#""default_update_style":"unknown""#,
        ),
        (r#""projects_mode":"all""#, r#""projects_mode":"unknown""#),
        (
            r#""allow_manual_merge":false"#,
            r#""allow_manual_merge":null"#,
        ),
        (r#""permissions":{"#, r#""permissions":{"unknown":true,"#),
        (
            r#""internal_tracker":{"#,
            r#""internal_tracker":{"unknown":true,"#,
        ),
        (r#""owner":{"#, r#""owner":{"unknown":true,"#),
        (
            r#""permissions":{"admin":false,"push":false,"pull":true}"#,
            r#""permissions":[false,false,true]"#,
        ),
        (r#""size":74757"#, r#""size":-1"#),
        (r#""empty":false,"#, ""),
    ] {
        let invalid = input.replace(original, replacement);
        assert_ne!(invalid, input);
        assert_eq!(
            decode_bounded_json::<RepositoryRecord, _>(
                invalid.as_bytes(),
                None,
                invalid.len(),
                |bytes| amiss_wire::read_json(bytes, u64::MAX)
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    let fork = include_str!("../fixtures/gitea-fork.json");
    let invalid = fork.replace(r#""parent":{"#, r#""parent":{"unknown":true,"#);
    assert_ne!(invalid, fork);
    assert!(amiss_wire::read_json::<RepositoryRecord>(invalid.as_bytes(), u64::MAX).is_err());
    for invalid in ["null", "true", "42", "{}", "[]", "[{}]"] {
        assert!(amiss_wire::read_json::<RepositoryRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
}
