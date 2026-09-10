use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::installation::permissions::{
    AppPermissions, ReadOnly, ReadWrite, ReadWriteAdmin, WriteOnly,
};
use amiss_controller_github::installation::{InstallationToken, RepositorySelection};
use amiss_controller_github::repository::pull::PullRepositoryRecord;

const COMPLETE: &str = include_str!("../fixtures/installation-token.json");
const MINIMAL: &str =
    r#"{"token":"synthetic-installation-token","expires_at":"2026-09-10T01:00:00Z"}"#;

#[test]
fn installation_tokens_retain_complete_metadata_before_strict_reading() {
    for input in [MINIMAL, COMPLETE] {
        let (record, length): (InstallationToken, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&record).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
        );
        assert!(
            amiss_wire::read_json::<InstallationToken>(input.as_bytes(), u64::MAX).unwrap()
                == record
        );
        assert!(matches!(
            decode_bounded_json::<InstallationToken, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| amiss_wire::read_json(bytes, u64::MAX)
            ),
            Err(ProviderError::InvalidResponse)
        ));
        assert!(amiss_wire::read_json::<InstallationToken>(input.as_bytes(), 0).is_err());
        assert!(
            amiss_wire::read_json::<InstallationToken>(
                format!("{input} {{}}").as_bytes(),
                u64::MAX
            )
            .is_err()
        );
    }
    let complete: InstallationToken = serde_json::from_str(COMPLETE).unwrap();
    assert_eq!(
        complete.repository_selection,
        Some(RepositorySelection::Selected)
    );
    assert_eq!(complete.has_multiple_single_files, Some(true));
    let permissions = complete.permissions.unwrap();
    assert_eq!(permissions.contents, Some(ReadWrite::Write));
    assert_eq!(
        permissions.organization_projects,
        Some(ReadWriteAdmin::Admin)
    );
    assert_eq!(permissions.organization_events, Some(ReadOnly::Read));
    assert_eq!(permissions.workflows, Some(WriteOnly::Write));
}

#[test]
fn installation_optionals_are_absent_or_nonnull_not_silently_defaulted() {
    let record: InstallationToken = serde_json::from_str(COMPLETE).unwrap();
    let encoded = serde_json::to_string(&record).unwrap();
    for (name, supplied) in [
        (
            "permissions",
            serde_json::to_string(&record.permissions).unwrap(),
        ),
        ("repositories", "[]".to_owned()),
        ("repository_selection", r#""selected""#.to_owned()),
        ("single_file", r#"".github/settings.yml""#.to_owned()),
        (
            "single_file_paths",
            r#"[".github/settings.yml","config.yml"]"#.to_owned(),
        ),
        ("has_multiple_single_files", "true".to_owned()),
    ] {
        let member = format!(",\"{name}\":{supplied}");
        assert_eq!(encoded.matches(&member).count(), 1, "{name}");
        let absent = encoded.replacen(&member, "", 1);
        let decoded: InstallationToken = serde_json::from_str(&absent).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&decoded).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(absent.as_bytes()).unwrap(),
            "{name}"
        );
        assert!(
            amiss_wire::read_json::<InstallationToken>(absent.as_bytes(), u64::MAX).unwrap()
                == decoded,
            "{name}"
        );
        let null = encoded.replacen(&member, &format!(",\"{name}\":null"), 1);
        assert!(
            serde_json::from_str::<InstallationToken>(&null).is_err(),
            "{name}"
        );
        assert!(
            amiss_wire::read_json::<InstallationToken>(null.as_bytes(), u64::MAX).is_err(),
            "{name}"
        );
    }
    let mut empty: InstallationToken = serde_json::from_str(MINIMAL).unwrap();
    empty.permissions = Some(AppPermissions::default());
    empty.repository_selection = Some(RepositorySelection::All);
    empty.single_file = Some(String::new());
    empty.single_file_paths = Some(Vec::new());
    empty.has_multiple_single_files = Some(false);
    let encoded = serde_json::to_vec(&empty).unwrap();
    assert!(serde_json::from_slice::<InstallationToken>(&encoded).unwrap() == empty);
    assert!(amiss_wire::read_json::<InstallationToken>(&encoded, u64::MAX).unwrap() == empty);
}

#[test]
fn installation_fields_are_required_and_closed_at_json_ingress() {
    for input in [
        "{}",
        r#"{"token":"synthetic"}"#,
        r#"{"expires_at":"2026-09-10T01:00:00Z"}"#,
        r#"{"token":null,"expires_at":"2026-09-10T01:00:00Z"}"#,
        r#"{"token":"synthetic","expires_at":null}"#,
        r#"{"token":3,"expires_at":"2026-09-10T01:00:00Z"}"#,
        r#"{"token":"synthetic","expires_at":3}"#,
    ] {
        assert!(serde_json::from_str::<InstallationToken>(input).is_err());
        assert!(amiss_wire::read_json::<InstallationToken>(input.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        (r#"{"token":"#, r#"{"unexpected":true,"token":"#),
        (r#"{"token":"#, r#"{"\u0074oken":"other","token":"#),
        (
            r#""permissions":{"#,
            r#""permissions":{"unknown_scope":"read","#,
        ),
        (
            r#""actions":"write""#,
            r#""\u0061ctions":"read","actions":"write""#,
        ),
        (
            r#""repository_selection":"selected""#,
            r#""repository_selection":"other""#,
        ),
        (
            r#""repository_selection":"selected""#,
            r#""repository_selection":{"selected":null}"#,
        ),
        (r#""single_file_paths":["#, r#""single_file_paths":[null,"#),
        (
            r#""has_multiple_single_files":true"#,
            r#""has_multiple_single_files":"true""#,
        ),
    ] {
        assert_eq!(COMPLETE.matches(old).count(), 1, "{old}");
        let changed = COMPLETE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<InstallationToken>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<InstallationToken>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let record: InstallationToken = serde_json::from_str(COMPLETE).unwrap();
    let positional = serde_json::to_vec(&(
        &record.token,
        &record.expires_at,
        &record.permissions,
        &record.repositories,
        &record.repository_selection,
        &record.single_file,
        &record.single_file_paths,
        &record.has_multiple_single_files,
    ))
    .unwrap();
    assert!(serde_json::from_slice::<InstallationToken>(&positional).unwrap() == record);
    assert!(amiss_wire::read_json::<InstallationToken>(&positional, u64::MAX).is_err());
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

#[test]
fn installation_repositories_share_the_complete_bounded_identity_contract() {
    let input = include_str!("../fixtures/pull-repository.json");
    let repository: PullRepositoryRecord = serde_json::from_str(input).unwrap();
    let mut record: InstallationToken = serde_json::from_str(COMPLETE).unwrap();
    record.repositories = Some(vec![repository]);
    let encoded = serde_json::to_string(&record).unwrap();
    assert!(serde_json::from_str::<InstallationToken>(&encoded).unwrap() == record);
    assert!(
        amiss_wire::read_json::<InstallationToken>(encoded.as_bytes(), u64::MAX).unwrap() == record
    );
    let repository = record.repositories.as_ref().unwrap().first().unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(repository).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
    );
    let id = format!("\"repositories\":[{{\"id\":{}", repository.id);
    for (old, new) in [
        (
            "\"repositories\":[{",
            "\"repositories\":[{\"unexpected\":true,",
        ),
        (id.as_str(), "\"repositories\":[{\"id\":9007199254740992"),
        (id.as_str(), "\"repositories\":[{\"id\":-1"),
        (id.as_str(), "\"repositories\":[{\"id\":null"),
        ("\"repositories\":[{", "\"repositories\":[null,{"),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<InstallationToken>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<InstallationToken>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
}
