use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::repository::metadata::{
    CodeSearchIndexStatus, LicenseRecord, MergeCommitMessage, MergeCommitTitle, RepositoryAccess,
    SquashMergeCommitMessage, SquashMergeCommitTitle,
};
use amiss_wire::assessment::Nullable;

#[test]
fn pull_repository_capture_keeps_shared_identity_without_metadata_requirements() {
    let input: &[u8] = include_bytes!("../fixtures/pull-repository.json");
    let (captured, consumed): (WorkflowRepositoryRecord, _) = decode_bounded_json(
        input,
        Some(u64::try_from(input.len()).unwrap()),
        input.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, input.len());
    assert_eq!(captured.id, 1_298_463_903);
    assert_eq!(captured.name, "amiss");
    assert_eq!(captured.full_name, "HardMax71/amiss");
    assert_eq!(captured.owner.login, "HardMax71");
    let encoded = serde_json::to_string(&captured).unwrap();
    let metadata = encoded.replacen('{', r#"{"default_branch":null,"disabled":[],"permissions":false,"license":0,"custom_properties":true,"template_repository":{},"extra":null,"#, 1);
    assert_eq!(
        serde_json::from_str::<WorkflowRepositoryRecord>(&metadata).unwrap(),
        captured
    );
    assert_eq!(
        decode_bounded_json::<WorkflowRepositoryRecord, _>(input, None, input.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        },),
        Err(ProviderError::InvalidResponse)
    );
    assert!(serde_json::from_str::<WorkflowRepositoryRecord>(&format!("{encoded} {{}}")).is_err());
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
