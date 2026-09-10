use amiss_controller_github::check::{AppOwner, CheckRunApp, EnterpriseRecord};
use amiss_wire::assessment::Nullable;
use js_int::UInt;

const APP: &str = include_str!("../fixtures/github-app.json");

#[test]
fn captured_check_app_retains_its_complete_declared_metadata() {
    let mut app: CheckRunApp = amiss_wire::read_json(APP.as_bytes(), u64::MAX).unwrap();
    assert_eq!(serde_json::from_str::<CheckRunApp>(APP).unwrap(), app);
    assert_eq!(app.id, 15368);
    assert_eq!(app.name, "GitHub Actions");
    assert!(matches!(&app.owner, AppOwner::Account(owner) if owner.login == "github"));
    assert_eq!(app.permissions.len(), 23);
    assert_eq!(app.permissions["artifact_metadata"], "write");
    assert_eq!(app.permissions["models"], "read");
    assert!(app.events.iter().any(|event| event == "workflow_run"));
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&app).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(APP.as_bytes()).unwrap(),
    );
    app.id = UInt::MAX.into();
    app.installations_count = Some(UInt::MAX);
    let encoded = serde_json::to_vec(&app).unwrap();
    assert_eq!(
        serde_json::from_slice::<CheckRunApp>(&encoded).unwrap(),
        app
    );
    assert_eq!(
        amiss_wire::read_json::<CheckRunApp>(&encoded, u64::MAX).unwrap(),
        app
    );
    let positional = serde_json::to_vec(&(
        app.id,
        &app.node_id,
        &app.owner,
        &app.name,
        &app.description,
        &app.external_url,
        &app.html_url,
        &app.created_at,
        &app.updated_at,
        &app.permissions,
        &app.events,
        &app.slug,
        &app.client_id,
        &app.installations_count,
    ))
    .unwrap();
    assert!(serde_json::from_slice::<CheckRunApp>(&positional).is_ok());
    assert!(amiss_wire::read_json::<CheckRunApp>(&positional, u64::MAX).is_err());
    assert!(amiss_wire::read_json::<CheckRunApp>(APP.as_bytes(), 0).is_err());
    let trailing = format!("{APP} {{}}");
    assert!(amiss_wire::read_json::<CheckRunApp>(trailing.as_bytes(), u64::MAX).is_err());
}

#[test]
fn app_optional_fields_do_not_change_required_nullability() {
    let mut app: CheckRunApp = amiss_wire::read_json(APP.as_bytes(), u64::MAX).unwrap();
    app.description = Nullable::Null;
    app.slug = None;
    app.client_id = None;
    app.installations_count = None;
    let encoded = serde_json::to_string(&app).unwrap();
    assert_eq!(serde_json::from_str::<CheckRunApp>(&encoded).unwrap(), app);
    assert_eq!(
        amiss_wire::read_json::<CheckRunApp>(encoded.as_bytes(), u64::MAX).unwrap(),
        app
    );
    for field in ["slug", "client_id", "installations_count"] {
        assert!(!encoded.contains(&format!("\"{field}\":")));
        let null = encoded.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&null).is_err(),
            "{field}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(null.as_bytes(), u64::MAX).is_err());
    }
    for member in [
        "\"description\":null,".to_owned(),
        format!(
            "\"node_id\":{},",
            serde_json::to_string(&app.node_id).unwrap()
        ),
        format!(
            "\"permissions\":{},",
            serde_json::to_string(&app.permissions).unwrap()
        ),
        format!(
            ",\"events\":{}",
            serde_json::to_string(&app.events).unwrap()
        ),
    ] {
        assert_eq!(encoded.matches(&member).count(), 1);
        let missing = encoded.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&missing).is_err(),
            "{member}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(missing.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn enterprise_app_owners_preserve_required_and_optional_nulls() {
    let mut app: CheckRunApp = amiss_wire::read_json(APP.as_bytes(), u64::MAX).unwrap();
    let mut enterprise = EnterpriseRecord {
        id: UInt::from(42_u8),
        node_id: "enterprise-node".to_owned(),
        name: "Example enterprise".to_owned(),
        slug: "example".to_owned(),
        html_url: "https://github.com/enterprises/example".to_owned(),
        created_at: Nullable::Null,
        updated_at: Nullable::Null,
        avatar_url: "https://example.test/avatar.png".to_owned(),
        description: None,
        website_url: None,
    };
    for optional in [
        None,
        Some(Nullable::Null),
        Some(Nullable::Value("https://example.test".to_owned())),
    ] {
        enterprise.description = optional.clone();
        enterprise.website_url = optional;
        app.owner = AppOwner::Enterprise(Box::new(enterprise.clone()));
        let encoded = serde_json::to_vec(&app).unwrap();
        assert_eq!(
            serde_json::from_slice::<CheckRunApp>(&encoded).unwrap(),
            app
        );
        assert_eq!(
            amiss_wire::read_json::<CheckRunApp>(&encoded, u64::MAX).unwrap(),
            app
        );
    }
    let encoded = serde_json::to_string(&app).unwrap();
    for (old, new) in [
        ("\"created_at\":null,", ""),
        ("\"updated_at\":null,", ""),
        ("\"slug\":\"example\",", ""),
        ("\"id\":42", "\"id\":9007199254740992"),
        (
            "\"node_id\":\"enterprise-node\"",
            "\"extra\":true,\"node_id\":\"enterprise-node\"",
        ),
        ("\"id\":42", "\"\\u0069d\":42,\"id\":42"),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(changed.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn app_owners_are_complete_closed_objects_not_fallback_payloads() {
    let app: CheckRunApp = amiss_wire::read_json(APP.as_bytes(), u64::MAX).unwrap();
    let encoded = serde_json::to_string(&app).unwrap();
    let owner = format!("\"owner\":{}", serde_json::to_string(&app.owner).unwrap());
    assert_eq!(encoded.matches(&owner).count(), 1);
    for invalid in ["null", "true", "1", "[]", "{}", r#"{"login":"github"}"#] {
        let changed = encoded.replacen(&owner, &format!("\"owner\":{invalid}"), 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&changed).is_err(),
            "{invalid}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(changed.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        (
            "\"name\":\"GitHub Actions\"",
            "\"extra\":true,\"name\":\"GitHub Actions\"",
        ),
        ("\"owner\":{", "\"owner\":{\"extra\":true,"),
        ("\"login\":\"github\",", ""),
        ("\"id\":15368", "\"\\u0069d\":15368,\"id\":15368"),
    ] {
        assert_eq!(APP.matches(old).count(), 1, "{old}");
        let changed = APP.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(changed.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn app_permissions_events_and_counts_reject_wrong_scalar_shapes() {
    let app: CheckRunApp = amiss_wire::read_json(APP.as_bytes(), u64::MAX).unwrap();
    let encoded = serde_json::to_string(&app).unwrap();
    let events = format!("\"events\":{}", serde_json::to_string(&app.events).unwrap());
    assert_eq!(encoded.matches(&events).count(), 1);
    for invalid in ["null", "true", "1", "[]", "{}"] {
        let old = "\"metadata\":\"read\"";
        assert_eq!(encoded.matches(old).count(), 1);
        let changed = encoded.replacen(old, &format!("\"metadata\":{invalid}"), 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&changed).is_err(),
            "{invalid}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(changed.as_bytes(), u64::MAX).is_err());
    }
    for (old, new) in [
        (
            "\"metadata\":\"read\"",
            "\"metadata\":\"read\",\"\\u006detadata\":\"write\"",
        ),
        ("\"id\":15368", "\"id\":9007199254740992"),
        ("\"id\":15368", "\"id\":-1"),
        ("\"id\":15368", "\"id\":1.5"),
        (
            "\"id\":15368",
            "\"installations_count\":9007199254740992,\"id\":15368",
        ),
        (events.as_str(), "\"events\":[1]"),
        (events.as_str(), "\"events\":{}"),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<CheckRunApp>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<CheckRunApp>(changed.as_bytes(), u64::MAX).is_err());
    }
}
