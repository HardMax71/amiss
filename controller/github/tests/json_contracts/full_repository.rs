use amiss_controller::decode_bounded_json;
use amiss_controller_github::repository::RepositoryRecord;
use amiss_controller_github::repository::metadata::{CodeOfConduct, CodeOfConductSummary};
use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_wire::assessment::Nullable;

const CAPTURE: &str = include_str!("../fixtures/repository.json");
const FORK: &str = include_str!("../fixtures/repository-fork.json");

#[test]
fn full_repository_captures_retain_roots_parents_and_sources() {
    for input in [CAPTURE, FORK] {
        let (record, consumed): (RepositoryRecord, _) = decode_bounded_json(
            input.as_bytes(),
            Some(u64::try_from(input.len()).unwrap()),
            input.len(),
            |bytes| amiss_wire::read_json(bytes, u64::MAX),
        )
        .unwrap();
        assert_eq!(consumed, input.len());
        assert!(serde_json::from_str::<RepositoryRecord>(input).unwrap() == record);
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&record).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
        );
        assert!(amiss_wire::read_json::<RepositoryRecord>(input.as_bytes(), 0).is_err());
        let trailing = format!("{input} {{}}");
        assert!(amiss_wire::read_json::<RepositoryRecord>(trailing.as_bytes(), u64::MAX).is_err());
    }
    let fork: RepositoryRecord = amiss_wire::read_json(FORK.as_bytes(), u64::MAX).unwrap();
    assert!(fork.fork);
    assert_eq!(fork.full_name, "rizalgowandy/rest-api-description");
    assert_eq!(
        fork.parent.as_ref().unwrap().full_name,
        "github/rest-api-description"
    );
    assert!(fork.parent == fork.source);
}

#[test]
fn full_repository_optional_fields_reuse_existing_models() {
    let fork: RepositoryRecord = amiss_wire::read_json(FORK.as_bytes(), u64::MAX).unwrap();
    let complete = RepositoryRecord {
        anonymous_access_enabled: Some(false),
        code_of_conduct: Some(CodeOfConductSummary {
            url: "https://api.github.com/codes_of_conduct/contributor_covenant".to_owned(),
            html_url: Nullable::Null,
            key: "contributor_covenant".to_owned(),
            name: "Contributor Covenant".to_owned(),
        }),
        custom_properties: Some(
            amiss_wire::read_json(
                include_bytes!("../fixtures/repository-custom-properties.json"),
                u64::MAX,
            )
            .unwrap(),
        ),
        master_branch: Some("main".to_owned()),
        organization: Some(Nullable::Value(
            amiss_wire::read_json(
                include_bytes!("../fixtures/owner-organization.json"),
                u64::MAX,
            )
            .unwrap(),
        )),
        template_repository: Some(Nullable::Value(fork.parent.clone().unwrap())),
        parent: fork.parent,
        source: fork.source,
        ..amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap()
    };
    let encoded = serde_json::to_vec(&complete).unwrap();
    assert!(serde_json::from_slice::<RepositoryRecord>(&encoded).unwrap() == complete);
    assert!(amiss_wire::read_json::<RepositoryRecord>(&encoded, u64::MAX).unwrap() == complete);
}

#[test]
fn full_repository_presence_distinguishes_required_and_optional_nulls() {
    let mut record = RepositoryRecord {
        description: Nullable::Null,
        homepage: Nullable::Null,
        language: Nullable::Null,
        mirror_url: Nullable::Null,
        license: Nullable::Null,
        ..amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap()
    };
    let encoded = serde_json::to_string(&record).unwrap();
    assert!(serde_json::from_str::<RepositoryRecord>(&encoded).unwrap() == record);
    assert!(
        amiss_wire::read_json::<RepositoryRecord>(encoded.as_bytes(), u64::MAX).unwrap() == record
    );
    for field in [
        "description",
        "homepage",
        "language",
        "mirror_url",
        "license",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        let missing = encoded.replacen(&member, "", 1);
        assert!(serde_json::from_str::<RepositoryRecord>(&missing).is_err());
        assert!(amiss_wire::read_json::<RepositoryRecord>(missing.as_bytes(), u64::MAX).is_err());
    }
    let owner = amiss_wire::read_json(
        include_bytes!("../fixtures/owner-organization.json"),
        u64::MAX,
    )
    .unwrap();
    let template: PullRepositoryRecord =
        amiss_wire::read_json(include_bytes!("../fixtures/pull-repository.json"), u64::MAX)
            .unwrap();
    let security = record.security_and_analysis.clone().unwrap();
    for (organization, template, security, token) in [
        (None, None, None, None),
        (
            Some(Nullable::Null),
            Some(Nullable::Null),
            Some(Nullable::Null),
            Some(Nullable::Null),
        ),
        (
            Some(Nullable::Value(owner)),
            Some(Nullable::Value(Box::new(template))),
            Some(security),
            Some(Nullable::Value("synthetic-token".to_owned())),
        ),
    ] {
        record.organization = organization;
        record.template_repository = template;
        record.security_and_analysis = security;
        record.temp_clone_token = token;
        let encoded = serde_json::to_vec(&record).unwrap();
        assert!(serde_json::from_slice::<RepositoryRecord>(&encoded).unwrap() == record);
        assert!(amiss_wire::read_json::<RepositoryRecord>(&encoded, u64::MAX).unwrap() == record);
    }
}

#[test]
fn full_repository_required_fields_and_numbers_are_exact() {
    let record: RepositoryRecord = amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    for (field, value) in [
        ("id", serde_json::to_string(&record.id).unwrap()),
        (
            "default_branch",
            serde_json::to_string(&record.default_branch).unwrap(),
        ),
        (
            "created_at",
            serde_json::to_string(&record.created_at).unwrap(),
        ),
        (
            "pushed_at",
            serde_json::to_string(&record.pushed_at).unwrap(),
        ),
        (
            "updated_at",
            serde_json::to_string(&record.updated_at).unwrap(),
        ),
        (
            "has_discussions",
            serde_json::to_string(&record.has_discussions).unwrap(),
        ),
        (
            "network_count",
            serde_json::to_string(&record.network_count).unwrap(),
        ),
        (
            "subscribers_count",
            serde_json::to_string(&record.subscribers_count).unwrap(),
        ),
    ] {
        let member = format!("\"{field}\":{value},");
        assert_eq!(CAPTURE.matches(&member).count(), 1, "{field}");
        for replacement in [String::new(), format!("\"{field}\":null,")] {
            let changed = CAPTURE.replacen(&member, &replacement, 1);
            assert!(
                serde_json::from_str::<RepositoryRecord>(&changed).is_err(),
                "{field}"
            );
            assert!(
                amiss_wire::read_json::<RepositoryRecord>(changed.as_bytes(), u64::MAX).is_err()
            );
        }
    }
    for (field, value) in [
        ("id", record.id.to_string()),
        ("size", record.size.to_string()),
        ("network_count", record.network_count.to_string()),
        ("subscribers_count", record.subscribers_count.to_string()),
    ] {
        let member = format!("\"{field}\":{value},");
        assert_eq!(CAPTURE.matches(&member).count(), 1, "{field}");
        for invalid in ["9007199254740992", "-1", "1.5"] {
            let changed = CAPTURE.replacen(&member, &format!("\"{field}\":{invalid},"), 1);
            assert!(
                serde_json::from_str::<RepositoryRecord>(&changed).is_err(),
                "{field}"
            );
            assert!(
                amiss_wire::read_json::<RepositoryRecord>(changed.as_bytes(), u64::MAX).is_err()
            );
        }
    }
}

#[test]
fn full_repository_nested_data_is_closed() {
    for injected in [
        r#""extra":true"#,
        r#""parent":null"#,
        r#""source":null"#,
        r#""code_of_conduct":null"#,
        r#""custom_properties":null"#,
        r#""custom_properties":{"flag":true}"#,
        r#""organization":{"extra":true}"#,
        r#""template_repository":{"extra":true}"#,
        r#""anonymous_access_enabled":null"#,
        r#""master_branch":null"#,
    ] {
        let changed = CAPTURE.replacen('{', &format!("{{{injected},"), 1);
        assert!(
            serde_json::from_str::<RepositoryRecord>(&changed).is_err(),
            "{injected}"
        );
        assert!(amiss_wire::read_json::<RepositoryRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    for (input, old, new) in [
        (FORK, "\"parent\":{", "\"parent\":{\"extra\":true,"),
        (FORK, "\"source\":{", "\"source\":{\"extra\":true,"),
        (
            CAPTURE,
            "\"security_and_analysis\":{",
            "\"security_and_analysis\":{\"extra\":true,",
        ),
        (
            CAPTURE,
            "\"secret_scanning_validity_checks\":{",
            "\"secret_scanning_validity_checks\":{\"extra\":true,",
        ),
        (
            CAPTURE,
            "\"secret_scanning_validity_checks\":{\"status\":\"disabled\"}",
            "\"secret_scanning_validity_checks\":null",
        ),
        (
            CAPTURE,
            "\"full_name\":",
            "\"\\u0066ull_name\":\"HardMax71/amiss\",\"full_name\":",
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<RepositoryRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<RepositoryRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    let mut optional: RepositoryRecord =
        amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    optional.has_downloads = None;
    let encoded = serde_json::to_string(&optional).unwrap();
    assert!(!encoded.contains("\"has_downloads\":"));
    assert!(
        amiss_wire::read_json::<RepositoryRecord>(encoded.as_bytes(), u64::MAX).unwrap()
            == optional
    );
    let null = encoded.replacen('{', "{\"has_downloads\":null,", 1);
    assert!(serde_json::from_str::<RepositoryRecord>(&null).is_err());
}

#[test]
fn conduct_summaries_match_the_four_field_contract() {
    let summary = CodeOfConductSummary {
        url: "https://example.com/code".to_owned(),
        html_url: Nullable::Null,
        key: "contributor_covenant".to_owned(),
        name: "Contributor Covenant".to_owned(),
    };
    let encoded = serde_json::to_string(&summary).unwrap();
    assert_eq!(
        amiss_wire::read_json::<CodeOfConductSummary>(encoded.as_bytes(), u64::MAX).unwrap(),
        summary
    );
    let missing = encoded.replacen("\"html_url\":null,", "", 1);
    assert_ne!(missing, encoded);
    assert!(serde_json::from_str::<CodeOfConductSummary>(&missing).is_err());
    let with_body = encoded.replacen('{', "{\"body\":\"Be respectful.\",", 1);
    assert!(serde_json::from_str::<CodeOfConduct>(&with_body).is_ok());
    assert!(serde_json::from_str::<CodeOfConductSummary>(&with_body).is_err());
    assert!(amiss_wire::read_json::<CodeOfConductSummary>(with_body.as_bytes(), u64::MAX).is_err());
    let positional =
        serde_json::to_vec(&(&summary.url, &summary.html_url, &summary.key, &summary.name))
            .unwrap();
    assert!(serde_json::from_slice::<CodeOfConductSummary>(&positional).is_ok());
    assert!(amiss_wire::read_json::<CodeOfConductSummary>(&positional, u64::MAX).is_err());
}
