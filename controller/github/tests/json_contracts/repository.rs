use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::repository::metadata::{
    BypassMode, BypassOptions, BypassReviewer, CodeOfConduct, CustomProperties, CustomProperty,
    PullRequestCreationPolicy, RepositoryLicense, RepositoryPermissions, ReviewerKind,
    SecurityAnalysis, SecurityFeature, SecurityStatus,
};
use amiss_wire::assessment::Nullable;
use js_int::UInt;

const CAPTURE: &str = include_str!("../fixtures/workflow-repository.json");

#[test]
fn workflow_repository_retains_captured_and_optional_metadata() {
    let captured: WorkflowRepositoryRecord =
        amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    assert_eq!(captured.id, 1_298_463_903);
    assert_eq!(captured.full_name, "HardMax71/amiss");
    assert_eq!(captured.owner.login, "HardMax71");
    assert!(captured.contents_url.ends_with("{+path}"));
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&captured).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(CAPTURE.as_bytes()).unwrap()
    );
    let complete = WorkflowRepositoryRecord {
        allow_forking: Some(true),
        archived: Some(false),
        clone_url: Some("https://github.com/HardMax71/amiss.git".to_owned()),
        code_of_conduct: Some(CodeOfConduct {
            url: "https://api.github.com/codes_of_conduct/contributor_covenant".to_owned(),
            html_url: Nullable::Null,
            key: "contributor_covenant".to_owned(),
            name: "Contributor Covenant".to_owned(),
            body: Some("Be respectful.".to_owned()),
        }),
        created_at: Some(Nullable::Value("2026-07-22T00:00:00Z".to_owned())),
        custom_properties: Some(CustomProperties {
            entries: [
                (
                    "feature".to_owned(),
                    Nullable::Value(CustomProperty::Text("true".to_owned())),
                ),
                (
                    "teams".to_owned(),
                    Nullable::Value(CustomProperty::Choices(vec!["docs".to_owned()])),
                ),
                ("unset".to_owned(), Nullable::Null),
            ]
            .into(),
        }),
        default_branch: Some("main".to_owned()),
        delete_branch_on_merge: Some(true),
        disabled: Some(false),
        forks: Some(UInt::from(1_u8)),
        forks_count: Some(UInt::from(1_u8)),
        git_url: Some("git://github.com/HardMax71/amiss.git".to_owned()),
        has_discussions: Some(false),
        has_downloads: Some(true),
        has_issues: Some(true),
        has_pages: Some(true),
        has_projects: Some(false),
        has_pull_requests: Some(true),
        has_wiki: Some(false),
        homepage: Some(Nullable::Value(
            "https://hardmax71.github.io/amiss/".to_owned(),
        )),
        is_template: Some(false),
        language: Some(Nullable::Value("Rust".to_owned())),
        license: Some(Nullable::Value(RepositoryLicense {
            key: Some("mit".to_owned()),
            name: Some("MIT License".to_owned()),
            node_id: Some("MDc6TGljZW5zZTEz".to_owned()),
            spdx_id: Some("MIT".to_owned()),
            url: Some(Nullable::Null),
        })),
        mirror_url: Some(Nullable::Null),
        network_count: Some(UInt::from(1_u8)),
        open_issues: Some(UInt::from(2_u8)),
        open_issues_count: Some(UInt::from(2_u8)),
        permissions: Some(RepositoryPermissions {
            admin: Some(false),
            maintain: Some(false),
            pull: Some(true),
            push: Some(false),
            triage: Some(false),
        }),
        pull_request_creation_policy: Some(PullRequestCreationPolicy::CollaboratorsOnly),
        pushed_at: Some(Nullable::Value("2026-09-09T21:00:00Z".to_owned())),
        role_name: Some("read".to_owned()),
        security_and_analysis: Some(Nullable::Null),
        size: Some(UInt::from(30_u8)),
        ssh_url: Some("git@github.com:HardMax71/amiss.git".to_owned()),
        stargazers_count: Some(UInt::from(4_u8)),
        subscribers_count: Some(UInt::from(5_u8)),
        svn_url: Some("https://github.com/HardMax71/amiss".to_owned()),
        temp_clone_token: Some("synthetic-test-token".to_owned()),
        topics: Some(vec!["documentation".to_owned()]),
        updated_at: Some(Nullable::Null),
        visibility: Some("public".to_owned()),
        watchers: Some(UInt::from(4_u8)),
        watchers_count: Some(UInt::from(4_u8)),
        web_commit_signoff_required: Some(false),
        ..captured
    };
    let encoded = serde_json::to_vec(&complete).unwrap();
    assert!(serde_json::from_slice::<WorkflowRepositoryRecord>(&encoded).unwrap() == complete);
    assert!(
        amiss_wire::read_json::<WorkflowRepositoryRecord>(&encoded, u64::MAX).unwrap() == complete
    );
}

#[test]
fn workflow_repository_preserves_security_and_bypass_metadata() {
    let enabled = SecurityFeature {
        status: Some(SecurityStatus::Enabled),
    };
    let complete = WorkflowRepositoryRecord {
        security_and_analysis: Some(Nullable::Value(SecurityAnalysis {
            advanced_security: Some(enabled.clone()),
            code_security: Some(enabled.clone()),
            dependabot_security_updates: Some(enabled.clone()),
            secret_scanning: Some(enabled.clone()),
            secret_scanning_ai_detection: Some(enabled.clone()),
            secret_scanning_delegated_alert_dismissal: Some(enabled.clone()),
            secret_scanning_delegated_bypass: Some(enabled.clone()),
            secret_scanning_delegated_bypass_options: Some(BypassOptions {
                reviewers: Some(vec![
                    BypassReviewer {
                        reviewer_id: UInt::from(1_u8),
                        reviewer_type: ReviewerKind::Team,
                        mode: Some(BypassMode::Always),
                    },
                    BypassReviewer {
                        reviewer_id: UInt::from(2_u8),
                        reviewer_type: ReviewerKind::Role,
                        mode: Some(BypassMode::Exempt),
                    },
                    BypassReviewer {
                        reviewer_id: UInt::from(3_u8),
                        reviewer_type: ReviewerKind::Team,
                        mode: None,
                    },
                ]),
            }),
            secret_scanning_non_provider_patterns: Some(enabled),
            secret_scanning_push_protection: Some(SecurityFeature {
                status: Some(SecurityStatus::Disabled),
            }),
            secret_scanning_validity_checks: Some(SecurityFeature {
                status: Some(SecurityStatus::Enabled),
            }),
        })),
        ..amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap()
    };
    let encoded = serde_json::to_vec(&complete).unwrap();
    assert!(serde_json::from_slice::<WorkflowRepositoryRecord>(&encoded).unwrap() == complete);
    assert!(
        amiss_wire::read_json::<WorkflowRepositoryRecord>(&encoded, u64::MAX).unwrap() == complete
    );
}

#[test]
fn repository_properties_keep_real_strings_and_reject_untyped_values() {
    let input = include_str!("../fixtures/repository-custom-properties.json");
    let properties: CustomProperties = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
    assert_eq!(properties.entries.len(), 11);
    assert_eq!(
        properties.entries["CodeQL-Block"],
        Nullable::Value(CustomProperty::Text("true".to_owned()))
    );
    assert_eq!(
        properties.entries["deployable"],
        Nullable::Value(CustomProperty::Text("false".to_owned()))
    );
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&properties).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
    );
    for invalid in [
        r#"{"feature":true}"#,
        r#"{"feature":1}"#,
        r#"{"feature":{}}"#,
        r#"{"feature":[null]}"#,
        r#"{"feature":[["a"]]}"#,
        r#"{"feature":"a","feature":"b"}"#,
        r#"{"feature":"a","\u0066eature":"b"}"#,
    ] {
        assert!(
            serde_json::from_str::<CustomProperties>(invalid).is_err(),
            "{invalid}"
        );
        assert!(
            amiss_wire::read_json::<CustomProperties>(invalid.as_bytes(), u64::MAX).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn repository_metadata_refuses_unknown_null_and_invalid_fields() {
    for injected in [
        r#""extra":true"#,
        r#""custom_properties":null"#,
        r#""permissions":{"extra":true}"#,
        r#""permissions":{"pull":null}"#,
        r#""license":{"extra":true}"#,
        r#""license":{"key":null}"#,
        r#""code_of_conduct":{}"#,
        r#""code_of_conduct":{"url":"u","html_url":null,"key":"k","name":"n","extra":true}"#,
        r#""security_and_analysis":{"extra":true}"#,
        r#""security_and_analysis":{"secret_scanning":null}"#,
        r#""security_and_analysis":{"secret_scanning":{"status":null}}"#,
        r#""security_and_analysis":{"secret_scanning":{"extra":true}}"#,
        r#""security_and_analysis":{"secret_scanning":{"status":{"enabled":null}}}"#,
        r#""security_and_analysis":{"secret_scanning":{"status":"future"}}"#,
        r#""security_and_analysis":{"secret_scanning_validity_checks":null}"#,
        r#""security_and_analysis":{"secret_scanning_validity_checks":{"status":"not_set"}}"#,
        r#""security_and_analysis":{"secret_scanning_delegated_bypass_options":{"extra":true}}"#,
        r#""security_and_analysis":{"secret_scanning_delegated_bypass_options":{"reviewers":[{"reviewer_id":1}]}}"#,
        r#""security_and_analysis":{"secret_scanning_delegated_bypass_options":{"reviewers":[{"reviewer_id":1,"reviewer_type":"TEAM","extra":true}]}}"#,
        r#""security_and_analysis":{"secret_scanning_delegated_bypass_options":{"reviewers":[{"reviewer_id":1,"reviewer_type":"team"}]}}"#,
        r#""security_and_analysis":{"secret_scanning_delegated_bypass_options":{"reviewers":[{"reviewer_id":1,"reviewer_type":"TEAM","mode":null}]}}"#,
        r#""size":9007199254740992"#,
        r#""size":1.5"#,
        r#""size":null"#,
        r#""pull_request_creation_policy":{"all":null}"#,
        r#""pull_request_creation_policy":"future""#,
    ] {
        let changed = CAPTURE.replacen('{', &format!("{{{injected},"), 1);
        assert!(
            serde_json::from_str::<WorkflowRepositoryRecord>(&changed).is_err(),
            "{injected}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowRepositoryRecord>(changed.as_bytes(), u64::MAX)
                .is_err(),
            "{injected}"
        );
    }
    for (old, new) in [
        ("\"id\":1298463903,", ""),
        ("\"id\":1298463903", "\"id\":9007199254740992"),
        ("\"id\":1298463903", "\"id\":-1"),
        ("\"id\":1298463903", "\"id\":1.5"),
        ("\"full_name\":\"HardMax71/amiss\",", ""),
        ("\"fork\":false,", ""),
        ("\"fork\":false", "\"\\u0066ork\":false,\"fork\":false"),
    ] {
        assert_eq!(CAPTURE.matches(old).count(), 1, "{old}");
        let changed = CAPTURE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowRepositoryRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowRepositoryRecord>(changed.as_bytes(), u64::MAX)
                .is_err()
        );
    }
}

#[test]
fn nullable_repository_metadata_preserves_presence_without_defaults() {
    let mut record: WorkflowRepositoryRecord =
        amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    record.description = Nullable::Null;
    record.created_at = Some(Nullable::Null);
    record.updated_at = Some(Nullable::Null);
    record.pushed_at = Some(Nullable::Null);
    record.homepage = Some(Nullable::Null);
    record.language = Some(Nullable::Null);
    record.mirror_url = Some(Nullable::Null);
    record.license = Some(Nullable::Null);
    record.security_and_analysis = Some(Nullable::Null);
    record.pull_request_creation_policy = Some(PullRequestCreationPolicy::All);
    let encoded = serde_json::to_vec(&record).unwrap();
    assert!(serde_json::from_slice::<WorkflowRepositoryRecord>(&encoded).unwrap() == record);
    assert!(
        amiss_wire::read_json::<WorkflowRepositoryRecord>(&encoded, u64::MAX).unwrap() == record
    );
    let missing =
        String::from_utf8(encoded.clone())
            .unwrap()
            .replacen("\"description\":null,", "", 1);
    assert!(serde_json::from_str::<WorkflowRepositoryRecord>(&missing).is_err());
    assert!(amiss_wire::read_json::<WorkflowRepositoryRecord>(&encoded, 0).is_err());
    let mut trailing = encoded;
    trailing.extend_from_slice(b" {}");
    assert!(amiss_wire::read_json::<WorkflowRepositoryRecord>(&trailing, u64::MAX).is_err());
}
