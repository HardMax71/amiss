use amiss_controller::decode_bounded_json;
use amiss_controller_github::repository::metadata::{
    CodeSearchIndexStatus, LicenseRecord, MergeCommitMessage, MergeCommitTitle,
    PullRequestCreationPolicy, RepositoryAccess, SquashMergeCommitMessage, SquashMergeCommitTitle,
};
use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_wire::assessment::Nullable;

const CAPTURE: &str = include_str!("../fixtures/pull-repository.json");

#[test]
fn pull_repository_capture_keeps_the_repository_and_owner_identity() {
    let (captured, consumed): (PullRepositoryRecord, _) = decode_bounded_json(
        CAPTURE.as_bytes(),
        Some(u64::try_from(CAPTURE.len()).unwrap()),
        CAPTURE.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, CAPTURE.len());
    assert_eq!(captured.id, 1_298_463_903);
    assert_eq!(captured.full_name, "HardMax71/amiss");
    assert_eq!(captured.owner.login, "HardMax71");
    assert_eq!(captured.default_branch, "main");
    assert!(captured.contents_url.ends_with("{+path}"));
    assert_eq!(
        serde_json::from_slice::<PullRepositoryRecord>(&serde_json::to_vec(&captured).unwrap())
            .unwrap(),
        captured
    );
    assert!(
        decode_bounded_json::<PullRepositoryRecord, _>(
            CAPTURE.as_bytes(),
            None,
            CAPTURE.len() - 1,
            |bytes| serde_json::from_slice(bytes),
        )
        .is_err()
    );
    let trailing = format!("{CAPTURE} {{}}");
    assert!(serde_json::from_str::<PullRepositoryRecord>(&trailing).is_err());
}

#[test]
fn pull_repository_retains_every_optional_policy_and_metadata_field() {
    let complete = PullRepositoryRecord {
        allow_auto_merge: Some(true),
        allow_forking: Some(true),
        allow_merge_commit: Some(false),
        allow_rebase_merge: Some(true),
        allow_squash_merge: Some(true),
        allow_update_branch: Some(true),
        anonymous_access_enabled: Some(false),
        code_search_index_status: Some(CodeSearchIndexStatus {
            lexical_commit_sha: Some("a".repeat(40)),
            lexical_search_ok: Some(true),
        }),
        delete_branch_on_merge: Some(true),
        has_discussions: Some(true),
        has_pull_requests: Some(true),
        is_template: Some(false),
        master_branch: Some("main".to_owned()),
        merge_commit_message: Some(MergeCommitMessage::PrBody),
        merge_commit_title: Some(MergeCommitTitle::PrTitle),
        permissions: Some(
            amiss_wire::read_json(
                include_bytes!("../fixtures/repository-access.json"),
                u64::MAX,
            )
            .unwrap(),
        ),
        pull_request_creation_policy: Some(PullRequestCreationPolicy::CollaboratorsOnly),
        squash_merge_commit_message: Some(SquashMergeCommitMessage::CommitMessages),
        squash_merge_commit_title: Some(SquashMergeCommitTitle::CommitOrPrTitle),
        starred_at: Some("2026-09-10T00:00:00Z".to_owned()),
        temp_clone_token: Some("synthetic-token".to_owned()),
        topics: Some(vec!["documentation".to_owned()]),
        use_squash_pr_title_as_default: Some(false),
        visibility: Some("public".to_owned()),
        web_commit_signoff_required: Some(true),
        ..serde_json::from_str(CAPTURE).unwrap()
    };
    let encoded = serde_json::to_vec(&complete).unwrap();
    assert_eq!(
        serde_json::from_slice::<PullRepositoryRecord>(&encoded).unwrap(),
        complete
    );
    assert_eq!(
        amiss_wire::read_json::<PullRepositoryRecord>(&encoded, u64::MAX).unwrap(),
        complete
    );
}

#[test]
fn pull_repository_nulls_do_not_make_required_members_optional() {
    let nullable = PullRepositoryRecord {
        description: Nullable::Null,
        homepage: Nullable::Null,
        language: Nullable::Null,
        mirror_url: Nullable::Null,
        license: Nullable::Null,
        pushed_at: Nullable::Null,
        created_at: Nullable::Null,
        updated_at: Nullable::Null,
        ..serde_json::from_str(CAPTURE).unwrap()
    };
    let encoded = serde_json::to_string(&nullable).unwrap();
    assert_eq!(
        serde_json::from_str::<PullRepositoryRecord>(&encoded).unwrap(),
        nullable
    );
    assert_eq!(
        amiss_wire::read_json::<PullRepositoryRecord>(encoded.as_bytes(), u64::MAX).unwrap(),
        nullable
    );
    for field in [
        "description",
        "homepage",
        "language",
        "mirror_url",
        "license",
        "pushed_at",
        "created_at",
        "updated_at",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        let missing = encoded.replacen(&member, "", 1);
        assert!(serde_json::from_str::<PullRepositoryRecord>(&missing).is_err());
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(missing.as_bytes(), u64::MAX).is_err()
        );
    }
}

#[test]
fn repository_details_preserve_their_own_requiredness() {
    let license = LicenseRecord {
        key: "other".to_owned(),
        name: "Other".to_owned(),
        node_id: "license-node".to_owned(),
        spdx_id: Nullable::Null,
        url: Nullable::Null,
        html_url: Some("https://example.com/license".to_owned()),
    };
    let encoded = serde_json::to_string(&license).unwrap();
    assert_eq!(
        amiss_wire::read_json::<LicenseRecord>(encoded.as_bytes(), u64::MAX).unwrap(),
        license
    );
    for (old, new) in [
        ("\"key\":\"other\",", ""),
        ("\"name\":\"Other\",", ""),
        ("\"node_id\":\"license-node\",", ""),
        ("\"spdx_id\":null,", ""),
        ("\"url\":null,", ""),
        (
            "\"html_url\":\"https://example.com/license\"",
            "\"html_url\":null",
        ),
        ("\"key\":", "\"extra\":true,\"key\":"),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<LicenseRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(amiss_wire::read_json::<LicenseRecord>(changed.as_bytes(), u64::MAX).is_err());
    }
    let positional = serde_json::to_vec(&(
        &license.key,
        &license.name,
        &license.node_id,
        &license.spdx_id,
        &license.url,
        &license.html_url,
    ))
    .unwrap();
    assert!(serde_json::from_slice::<LicenseRecord>(&positional).is_ok());
    assert!(amiss_wire::read_json::<LicenseRecord>(&positional, u64::MAX).is_err());

    let permissions = RepositoryAccess {
        admin: false,
        pull: true,
        push: false,
        maintain: None,
        triage: None,
    };
    let encoded = serde_json::to_string(&permissions).unwrap();
    assert_eq!(
        amiss_wire::read_json::<RepositoryAccess>(encoded.as_bytes(), u64::MAX).unwrap(),
        permissions
    );
    for member in ["\"admin\":false,", "\"pull\":true,", ",\"push\":false"] {
        assert_eq!(encoded.matches(member).count(), 1, "{member}");
        let missing = encoded.replacen(member, "", 1);
        assert!(serde_json::from_str::<RepositoryAccess>(&missing).is_err());
        assert!(amiss_wire::read_json::<RepositoryAccess>(missing.as_bytes(), u64::MAX).is_err());
    }
    let empty = CodeSearchIndexStatus {
        lexical_commit_sha: None,
        lexical_search_ok: None,
    };
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
    assert_eq!(
        amiss_wire::read_json::<CodeSearchIndexStatus>(b"{}", u64::MAX).unwrap(),
        empty
    );
}

#[test]
fn repository_models_refuse_unknown_null_and_invalid_members() {
    for injected in [
        r#""extra":true"#,
        r#""permissions":null"#,
        r#""permissions":{"admin":false,"pull":true,"push":false,"extra":true}"#,
        r#""permissions":{"admin":false,"pull":true,"push":false,"maintain":null}"#,
        r#""permissions":{"admin":false,"pull":true,"push":false,"triage":null}"#,
        r#""code_search_index_status":null"#,
        r#""code_search_index_status":{"extra":true}"#,
        r#""code_search_index_status":{"lexical_commit_sha":null}"#,
        r#""code_search_index_status":{"lexical_search_ok":null}"#,
        r#""master_branch":null"#,
        r#""temp_clone_token":null"#,
        r#""anonymous_access_enabled":null"#,
        r#""merge_commit_message":"COMMIT_MESSAGES""#,
        r#""merge_commit_title":"COMMIT_OR_PR_TITLE""#,
        r#""squash_merge_commit_message":"PR_TITLE""#,
        r#""squash_merge_commit_title":"MERGE_MESSAGE""#,
        r#""merge_commit_message":{"PR_BODY":null}"#,
        r#""merge_commit_message":"pr_body""#,
        r#""merge_commit_title":null"#,
        r#""squash_merge_commit_message":1"#,
        r#""squash_merge_commit_title":"future""#,
    ] {
        let changed = CAPTURE.replacen('{', &format!("{{{injected},"), 1);
        assert!(
            serde_json::from_str::<PullRepositoryRecord>(&changed).is_err(),
            "{injected}"
        );
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
    for (old, new) in [
        ("\"id\":1298463903,", ""),
        ("\"id\":1298463903", "\"id\":9007199254740992"),
        ("\"id\":1298463903", "\"id\":-1"),
        ("\"id\":1298463903", "\"id\":1.5"),
        ("\"default_branch\":\"main\",", ""),
        ("\"has_downloads\":false,", ""),
        ("\"has_downloads\":false", "\"has_downloads\":null"),
        (
            "\"full_name\":",
            "\"\\u0066ull_name\":\"HardMax71/amiss\",\"full_name\":",
        ),
        ("\"license\":{", "\"license\":{\"extra\":true,"),
    ] {
        assert_eq!(CAPTURE.matches(old).count(), 1, "{old}");
        let changed = CAPTURE.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRepositoryRecord>(&changed).is_err(),
            "{old}"
        );
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
    let captured: PullRepositoryRecord = serde_json::from_str(CAPTURE).unwrap();
    let member = format!("\"size\":{}", captured.size);
    assert_eq!(CAPTURE.matches(&member).count(), 1);
    for value in ["-1", "1.5", "9007199254740992", "null"] {
        let changed = CAPTURE.replacen(&member, &format!("\"size\":{value}"), 1);
        assert!(
            serde_json::from_str::<PullRepositoryRecord>(&changed).is_err(),
            "{value}"
        );
        assert!(
            amiss_wire::read_json::<PullRepositoryRecord>(changed.as_bytes(), u64::MAX).is_err()
        );
    }
}

#[test]
fn merge_policies_use_only_their_declared_string_spellings() {
    for (encoded, expected) in [
        (
            serde_json::to_string(&MergeCommitMessage::PrBody).unwrap(),
            "\"PR_BODY\"",
        ),
        (
            serde_json::to_string(&MergeCommitMessage::PrTitle).unwrap(),
            "\"PR_TITLE\"",
        ),
        (
            serde_json::to_string(&MergeCommitMessage::Blank).unwrap(),
            "\"BLANK\"",
        ),
        (
            serde_json::to_string(&MergeCommitTitle::PrTitle).unwrap(),
            "\"PR_TITLE\"",
        ),
        (
            serde_json::to_string(&MergeCommitTitle::MergeMessage).unwrap(),
            "\"MERGE_MESSAGE\"",
        ),
        (
            serde_json::to_string(&SquashMergeCommitMessage::PrBody).unwrap(),
            "\"PR_BODY\"",
        ),
        (
            serde_json::to_string(&SquashMergeCommitMessage::CommitMessages).unwrap(),
            "\"COMMIT_MESSAGES\"",
        ),
        (
            serde_json::to_string(&SquashMergeCommitMessage::Blank).unwrap(),
            "\"BLANK\"",
        ),
        (
            serde_json::to_string(&SquashMergeCommitTitle::PrTitle).unwrap(),
            "\"PR_TITLE\"",
        ),
        (
            serde_json::to_string(&SquashMergeCommitTitle::CommitOrPrTitle).unwrap(),
            "\"COMMIT_OR_PR_TITLE\"",
        ),
    ] {
        assert_eq!(encoded, expected);
    }
}
