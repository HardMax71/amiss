use amiss_controller_github::repository::metadata::RepositoryOrganization;
use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_controller_github::repository::template::{TemplateOwner, TemplateRepository};
use amiss_controller_github::webhook::GitHubPayload;
use amiss_wire::assessment::Nullable;

#[test]
fn webhook_root_retains_the_complete_published_repository() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY;
    let record: PullRepositoryRecord = serde_json::from_slice(input).unwrap();
    let encoded = serde_json::to_vec(&record).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(input).unwrap(),
        amiss_fixtures::canonical_json(&encoded).unwrap()
    );
    assert_eq!(
        amiss_wire::read_json::<PullRepositoryRecord>(input, u64::MAX).unwrap(),
        record
    );
    assert!(amiss_wire::read_json::<PullRepositoryRecord>(input, 0).is_err());
    assert!(serde_json::from_str::<GitHubPayload>(r#"{"repository":null}"#).is_err());
    let envelope = serde_json::from_str::<GitHubPayload>("{}").unwrap();
    assert_eq!(envelope.repository, None);
}

#[test]
fn webhook_repository_additions_keep_presence_and_known_value_shapes() {
    let record: PullRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    for extra in [
        r#""network_count":0,"subscribers_count":9007199254740991"#,
        r#""organization":null,"template_repository":null"#,
        r#""organization":"octo-org""#,
        r#""template_repository":{}"#,
        r#""template_repository":{"owner":{},"permissions":{"pull":false}}"#,
    ] {
        let input = wire.replacen('{', &format!("{{{extra},"), 1);
        let decoded: PullRepositoryRecord = serde_json::from_str(&input).unwrap();
        assert_eq!(
            amiss_wire::read_json::<PullRepositoryRecord>(input.as_bytes(), u64::MAX).unwrap(),
            decoded
        );
        assert_eq!(
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
            amiss_fixtures::canonical_json(&serde_json::to_vec(&decoded).unwrap()).unwrap()
        );
    }
    for extra in [
        r#""network_count":null"#,
        r#""network_count":-1"#,
        r#""network_count":9007199254740992"#,
        r#""subscribers_count":1.0"#,
        r#""subscribers_count":1e0"#,
        r#""subscribers_count":-0"#,
        r#""organization":{}"#,
        r#""organization":false"#,
        r#""template_repository":false"#,
        r#""template_repository":{"unknown":true}"#,
        r#""template_repository":{"owner":{"unknown":true}}"#,
        r#""template_repository":{"permissions":{"unknown":true}}"#,
        r#""template_repository":{"owner":null}"#,
        r#""template_repository":{"owner":{"login":null}}"#,
        r#""template_repository":{"description":null}"#,
        r#""template_repository":{"permissions":null}"#,
        r#""template_repository":null,"\u0074emplate_repository":{}"#,
    ] {
        let input = wire.replacen('{', &format!("{{{extra},"), 1);
        assert!(
            serde_json::from_str::<PullRepositoryRecord>(&input).is_err(),
            "{extra}"
        );
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(input.as_bytes(), u64::MAX).is_err(),
            "{extra}"
        );
    }
    for properties in [
        r#"{"team":"docs","areas":["api","book"],"unset":null}"#,
        r#"{"empty":[],"name":""}"#,
    ] {
        let input = wire.replacen(
            r#""custom_properties":{}"#,
            &format!(r#""custom_properties":{properties}"#),
            1,
        );
        assert_ne!(input, wire);
        let decoded: PullRepositoryRecord = serde_json::from_str(&input).unwrap();
        assert_eq!(
            amiss_wire::read_json::<PullRepositoryRecord>(input.as_bytes(), u64::MAX).unwrap(),
            decoded
        );
        assert_eq!(
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
            amiss_fixtures::canonical_json(&serde_json::to_vec(&decoded).unwrap()).unwrap()
        );
    }
    for properties in [
        "null",
        r#"{"flag":false}"#,
        r#"{"nested":{}}"#,
        r#"{"team":"docs","\u0074eam":"api"}"#,
    ] {
        let input = wire.replacen(
            r#""custom_properties":{}"#,
            &format!(r#""custom_properties":{properties}"#),
            1,
        );
        assert_ne!(input, wire);
        assert!(
            serde_json::from_str::<PullRepositoryRecord>(&input).is_err(),
            "{properties}"
        );
        assert!(amiss_wire::read_json::<PullRepositoryRecord>(input.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn webhook_template_keeps_its_complete_distinct_optional_contract() {
    let input = include_bytes!("../fixtures/webhook-template-repository.json");
    let template: TemplateRepository = serde_json::from_slice(input).unwrap();
    let encoded = serde_json::to_vec(&template).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(input).unwrap(),
        amiss_fixtures::canonical_json(&encoded).unwrap()
    );
    assert_eq!(
        amiss_wire::read_json::<TemplateRepository>(input, u64::MAX).unwrap(),
        template
    );
    assert_eq!(
        serde_json::to_string(&TemplateRepository::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&TemplateOwner::default()).unwrap(),
        "{}"
    );
    let mut root: PullRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY).unwrap();
    root.organization = Some(Nullable::Value(RepositoryOrganization::Account(Box::new(
        root.owner.clone(),
    ))));
    root.template_repository = Some(Nullable::Value(template));
    let encoded = serde_json::to_vec(&root).unwrap();
    assert_eq!(
        serde_json::from_slice::<PullRepositoryRecord>(&encoded).unwrap(),
        root
    );
    assert_eq!(
        amiss_wire::read_json::<PullRepositoryRecord>(&encoded, u64::MAX).unwrap(),
        root
    );
    let owner = serde_json::to_string(&root.owner).unwrap();
    for invalid in [
        owner.replacen('{', r#"{"unknown":true,"#, 1),
        owner.replacen(r#""id":41548062"#, r#""id":9007199254740992"#, 1),
        owner.replacen(r#""id":41548062"#, r#""id":1,"\u0069d":2"#, 1),
    ] {
        assert_ne!(invalid, owner);
        assert!(serde_json::from_str::<RepositoryOrganization>(&invalid).is_err());
        assert!(
            amiss_wire::read_json::<RepositoryOrganization>(invalid.as_bytes(), u64::MAX).is_err()
        );
    }
    for input in [
        r#"{"id":9007199254740992}"#,
        r#"{"id":1.0}"#,
        r#"{"id":-0}"#,
        r#"{"owner":{"id":9007199254740992}}"#,
        r#"{"owner":{"id":1e0}}"#,
        r#"{"owner":{"site_admin":null}}"#,
        r#"{"owner":{"id":1,"\u0069d":2}}"#,
        r#"{"topics":null}"#,
        r#"{"topics":[null]}"#,
        r#"{"merge_commit_title":{"PR_TITLE":null}}"#,
        r#"{"merge_commit_message":"other"}"#,
        r#"{"squash_merge_commit_title":true}"#,
        r#"{"squash_merge_commit_message":"PR_TITLE"}"#,
    ] {
        assert!(
            serde_json::from_str::<TemplateRepository>(input).is_err(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<TemplateRepository>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
    for input in ["[]", r#"{"owner":[]}"#, r#"{"permissions":[]}"#, "{} null"] {
        assert!(
            amiss_wire::read_json::<TemplateRepository>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
}
