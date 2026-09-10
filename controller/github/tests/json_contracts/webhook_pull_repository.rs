use amiss_controller_github::repository::metadata::{
    MergeCommitMessage, MergeCommitTitle, PullRequestCreationPolicy, RepositoryAccess,
    SquashMergeCommitMessage, SquashMergeCommitTitle,
};
use amiss_controller_github::webhook::repository::pull::{
    License, PullRepository, Timestamp, Visibility,
};
use amiss_wire::assessment::Nullable;

const REPOSITORY: &[u8] = include_bytes!("../fixtures/webhook-pull-repository.json");

#[test]
fn published_pull_repository_survives_without_discarding_metadata() {
    let record: PullRepository = serde_json::from_slice(REPOSITORY).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(REPOSITORY).unwrap(),
        amiss_fixtures::canonical_json(&serde_json::to_vec(&record).unwrap()).unwrap(),
    );
    assert_eq!(record.id, 186_853_002);
    assert_eq!(record.visibility, Visibility::Public);
    assert_eq!(record.has_discussions, None);
    assert!(
        record
            .custom_properties
            .as_ref()
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(
        amiss_wire::read_json::<PullRepository>(REPOSITORY, u64::MAX).unwrap(),
        record,
    );
    assert!(amiss_wire::read_json::<PullRepository>(REPOSITORY, 0).is_err());
}

#[test]
fn webhook_pull_repository_keeps_all_declared_additions() {
    let mut record: PullRepository = serde_json::from_slice(REPOSITORY).unwrap();
    for field in [
        &mut record.allow_auto_merge,
        &mut record.allow_forking,
        &mut record.allow_merge_commit,
        &mut record.allow_rebase_merge,
        &mut record.allow_squash_merge,
        &mut record.allow_update_branch,
        &mut record.delete_branch_on_merge,
        &mut record.disabled,
        &mut record.has_discussions,
        &mut record.has_pull_requests,
        &mut record.is_template,
        &mut record.public,
        &mut record.use_squash_pr_title_as_default,
        &mut record.web_commit_signoff_required,
    ] {
        *field = Some(true);
    }
    record.master_branch = Some("main".to_owned());
    record.organization = Some("octo-org".to_owned());
    record.merge_commit_message = Some(MergeCommitMessage::PrBody);
    record.merge_commit_title = Some(MergeCommitTitle::MergeMessage);
    record.squash_merge_commit_message = Some(SquashMergeCommitMessage::CommitMessages);
    record.squash_merge_commit_title = Some(SquashMergeCommitTitle::CommitOrPrTitle);
    record.pull_request_creation_policy = Some(PullRequestCreationPolicy::CollaboratorsOnly);
    record.permissions = Some(RepositoryAccess {
        admin: false,
        pull: true,
        push: false,
        maintain: Some(true),
        triage: Some(false),
    });
    record.role_name = Some(Nullable::Value("reader".to_owned()));
    record.stargazers = Some(js_int::UInt::MAX);
    record.custom_properties = Some(
        serde_json::from_slice(include_bytes!(
            "../fixtures/repository-custom-properties.json"
        ))
        .unwrap(),
    );
    record.license = Nullable::Value(License {
        key: "mit".to_owned(),
        name: "MIT License".to_owned(),
        node_id: "license-one".to_owned(),
        spdx_id: "MIT".to_owned(),
        url: Nullable::Null,
    });
    record.topics = vec!["docs".to_owned()];
    for (visibility, spelling) in [
        (Visibility::Public, r#""public""#),
        (Visibility::Private, r#""private""#),
        (Visibility::Internal, r#""internal""#),
    ] {
        assert_eq!(serde_json::to_string(&visibility).unwrap(), spelling);
        record.visibility = visibility;
        let encoded = serde_json::to_vec(&record).unwrap();
        assert_eq!(
            amiss_wire::read_json::<PullRepository>(&encoded, u64::MAX).unwrap(),
            record,
        );
    }
    record.role_name = Some(Nullable::Null);
    record.owner = None;
    record.pushed_at = Nullable::Null;
    let encoded = serde_json::to_vec(&record).unwrap();
    assert_eq!(
        amiss_wire::read_json::<PullRepository>(&encoded, u64::MAX).unwrap(),
        record,
    );
}

#[test]
fn webhook_timestamps_preserve_signed_numbers_and_strings() {
    let mut record: PullRepository = serde_json::from_slice(REPOSITORY).unwrap();
    for input in [
        "-9007199254740991",
        "-1",
        "0",
        "9007199254740991",
        r#""2019-05-15T15:19:25Z""#,
    ] {
        let timestamp: Timestamp = serde_json::from_str(input).unwrap();
        assert_eq!(serde_json::to_string(&timestamp).unwrap(), input);
        record.created_at = timestamp.clone();
        record.pushed_at = Nullable::Value(timestamp);
        let encoded = serde_json::to_vec(&record).unwrap();
        assert_eq!(
            amiss_wire::read_json::<PullRepository>(&encoded, u64::MAX).unwrap(),
            record,
        );
    }
    for input in [
        "-9007199254740992",
        "9007199254740992",
        "-0",
        "1.0",
        "1e0",
        "null",
        "{}",
        "true",
    ] {
        assert!(serde_json::from_str::<Timestamp>(input).is_err(), "{input}");
    }
}

#[test]
fn webhook_required_nullable_members_cannot_disappear() {
    let record: PullRepository = serde_json::from_slice(REPOSITORY).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    for (name, value) in [
        ("id", serde_json::to_string(&record.id).unwrap()),
        ("owner", serde_json::to_string(&record.owner).unwrap()),
        (
            "description",
            serde_json::to_string(&record.description).unwrap(),
        ),
        ("homepage", serde_json::to_string(&record.homepage).unwrap()),
        ("language", serde_json::to_string(&record.language).unwrap()),
        (
            "mirror_url",
            serde_json::to_string(&record.mirror_url).unwrap(),
        ),
        ("license", serde_json::to_string(&record.license).unwrap()),
        (
            "created_at",
            serde_json::to_string(&record.created_at).unwrap(),
        ),
        (
            "updated_at",
            serde_json::to_string(&record.updated_at).unwrap(),
        ),
        (
            "pushed_at",
            serde_json::to_string(&record.pushed_at).unwrap(),
        ),
        ("topics", serde_json::to_string(&record.topics).unwrap()),
        (
            "visibility",
            serde_json::to_string(&record.visibility).unwrap(),
        ),
    ] {
        let member = format!(r#""{name}":{value},"#);
        assert_eq!(wire.matches(&member).count(), 1, "{name}");
        let missing = wire.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<PullRepository>(&missing).is_err(),
            "{name}"
        );
        assert!(
            amiss_wire::read_json::<PullRepository>(missing.as_bytes(), u64::MAX).is_err(),
            "{name}"
        );
    }
    let mut without_disputed_fields = wire;
    for member in [
        r#""is_template":false,"#,
        r#""custom_properties":{},"#,
        r#","web_commit_signoff_required":false"#,
    ] {
        assert_eq!(without_disputed_fields.matches(member).count(), 1);
        without_disputed_fields = without_disputed_fields.replacen(member, "", 1);
    }
    assert!(serde_json::from_str::<PullRepository>(&without_disputed_fields).is_ok());
}

#[test]
fn webhook_repository_refuses_unknown_ambiguous_and_malformed_members() {
    let record: PullRepository = serde_json::from_slice(REPOSITORY).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""owner":{"#, r#""owner":{"unknown":true,"#),
        (r#""id":186853002"#, r#""id":9007199254740992"#),
        (r#""id":186853002"#, r#""id":-0"#),
        (r#""id":186853002"#, r#""id":1e0"#),
        (r#""size":0"#, r#""size":9007199254740992"#),
        (r#""owner":{"#, r#""\u006fwner":null,"owner":{"#),
        (r#""topics":[]"#, r#""topics":null"#),
        (r#""topics":[]"#, r#""topics":[false]"#),
        (r#""visibility":"public""#, r#""visibility":"unknown""#),
        (
            r#""visibility":"public""#,
            r#""visibility":{"public":null}"#,
        ),
        (
            r#""updated_at":"2019-05-15T15:19:27Z""#,
            r#""updated_at":null"#,
        ),
        (
            r#""created_at":"2019-05-15T15:19:25Z""#,
            r#""created_at":null"#,
        ),
        (r#""disabled":false"#, r#""disabled":null"#),
        (r#""custom_properties":{}"#, r#""custom_properties":null"#),
        (
            r#""custom_properties":{}"#,
            r#""custom_properties":{"flag":false}"#,
        ),
        (
            r#""custom_properties":{}"#,
            r#""custom_properties":{"team":"a","\u0074eam":"b"}"#,
        ),
    ] {
        assert!(wire.contains(old), "{old}");
        let invalid = wire.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRepository>(&invalid).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRepository>(invalid.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    for extra in [
        r#""has_discussions":null"#,
        r#""organization":{}"#,
        r#""permissions":{"pull":true,"push":true}"#,
        r#""permissions":{"pull":true,"push":true,"admin":false,"unknown":true}"#,
        r#""merge_commit_title":"unknown""#,
        r#""pull_request_creation_policy":{"all":null}"#,
        r#""role_name":false"#,
        r#""stargazers":-1"#,
    ] {
        let invalid = wire.replacen('{', &format!("{{{extra},"), 1);
        assert!(
            serde_json::from_str::<PullRepository>(&invalid).is_err(),
            "{extra}"
        );
    }
    let owner = serde_json::to_string(&record.owner).unwrap();
    assert_eq!(wire.matches(&owner).count(), 1);
    for invalid in [
        wire.replacen(&owner, r#"["Codertocat",21031067]"#, 1),
        format!("{wire} null"),
        format!("[{wire}]"),
    ] {
        assert!(amiss_wire::read_json::<PullRepository>(invalid.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn webhook_license_requires_its_distinct_five_field_contract() {
    let input =
        r#"{"key":"mit","name":"MIT License","node_id":"license-one","spdx_id":"MIT","url":null}"#;
    let license: License = serde_json::from_str(input).unwrap();
    assert_eq!(serde_json::to_string(&license).unwrap(), input);
    for (old, new) in [
        (r#""key":"mit","#, ""),
        (r#""name":"MIT License","#, ""),
        (r#""node_id":"license-one","#, ""),
        (r#""spdx_id":"MIT","#, ""),
        (r#","url":null"#, ""),
        (r#""spdx_id":"MIT""#, r#""spdx_id":null"#),
        ("{", r#"{"html_url":"https://example.com","#),
    ] {
        assert_eq!(input.matches(old).count(), 1);
        let invalid = input.replacen(old, new, 1);
        assert!(serde_json::from_str::<License>(&invalid).is_err(), "{new}");
    }
}
