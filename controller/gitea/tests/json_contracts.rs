use std::io::Cursor;

#[path = "json_contracts/branches.rs"]
mod branches;

#[path = "json_contracts/commits.rs"]
mod commits;

#[path = "json_contracts/numbers.rs"]
mod numbers;

#[path = "json_contracts/protection.rs"]
mod protection;

#[path = "json_contracts/repositories.rs"]
mod repositories;

#[path = "json_contracts/refs.rs"]
mod refs;

#[path = "json_contracts/reviews.rs"]
mod reviews;

#[path = "json_contracts/statuses.rs"]
mod statuses;

use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::content::{
    ContentEncoding, ContentKind, ContentRecord, ContentResponse,
};
use amiss_controller_gitea::user::{UserRecord, UserVisibility};
use amiss_wire::controls::GitMode;

#[test]
fn user_profiles_preserve_both_provider_shapes_and_reject_projection_loss() {
    for (input, pronouns) in [
        (include_str!("fixtures/gitea-user.json"), None),
        (include_str!("fixtures/forgejo-user.json"), Some("")),
    ] {
        let (user, length): (UserRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(user.pronouns.as_deref(), pronouns);
        assert_eq!(user.id, 77);
        assert_eq!(user.username, user.login);
        assert_eq!(user.visibility, UserVisibility::Public);
        assert_eq!(user.last_login, "0001-01-01T00:00:00Z");
        for visibility in [
            UserVisibility::Public,
            UserVisibility::Limited,
            UserVisibility::Private,
        ] {
            let response = UserRecord {
                visibility,
                ..user.clone()
            };
            let encoded = serde_json::to_vec(&response).unwrap();
            assert_eq!(
                amiss_wire::read_json::<UserRecord>(&encoded, u64::MAX).unwrap(),
                response
            );
        }
        for (original, replacement) in [
            (r#""id": 77"#, r#""id": 77, "unknown": true"#),
            (r#""id": 77"#, r#""id": -1"#),
            (r#""id": 77"#, r#""id": 77, "\u0069d": 77"#),
            (r#""id": 77"#, r#""id": 9007199254740992"#),
            (r#""source_id": 0"#, r#""source_id": false"#),
            (r#""login_name": "","#, ""),
            (r#""username": "amiss-controller""#, r#""username": null"#),
            (r#""is_admin": false"#, r#""is_admin": 0"#),
            (r#""visibility": "public""#, r#""visibility": "unknown""#),
            (r#""followers_count": "#, r#""followers_count": -"#),
        ] {
            let invalid = input.replace(original, replacement);
            assert_ne!(invalid, input);
            assert_eq!(
                decode_bounded_json::<UserRecord, _>(
                    invalid.as_bytes(),
                    None,
                    invalid.len(),
                    |bytes| amiss_wire::read_json(bytes, u64::MAX)
                ),
                Err(ProviderError::InvalidResponse)
            );
        }
    }
    let input = include_str!("fixtures/forgejo-user.json");
    let invalid = input.replace(r#""pronouns": """#, r#""pronouns": null"#);
    assert_ne!(invalid, input);
    assert!(amiss_wire::read_json::<UserRecord>(invalid.as_bytes(), u64::MAX).is_err());
    for invalid in ["null", "true", "[]", "{}", "[{}]"] {
        assert!(amiss_wire::read_json::<UserRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn live_file_and_directory_responses_replay_without_defaults() {
    for (input, mode, single) in [
        (
            include_bytes!("fixtures/gitea-file.json").as_slice(),
            Some(GitMode::RegularFile),
            true,
        ),
        (
            include_bytes!("fixtures/forgejo-file.json").as_slice(),
            None,
            true,
        ),
        (
            include_bytes!("fixtures/gitea-directory.json").as_slice(),
            Some(GitMode::RegularFile),
            false,
        ),
        (
            include_bytes!("fixtures/forgejo-directory.json").as_slice(),
            None,
            false,
        ),
    ] {
        let (response, length): (ContentResponse, _) =
            decode_bounded_json(Cursor::new(input), None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(matches!(response, ContentResponse::Entry(_)), single);
        let entries: &[ContentRecord] = match &response {
            ContentResponse::Entry(entry) => std::slice::from_ref(entry),
            ContentResponse::Directory(entries) => entries,
        };
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.kind, ContentKind::File);
        assert_eq!(entry.mode, mode);
        assert_eq!(entry.content.is_some(), single);
        assert_eq!(entry.encoding, single.then_some(ContentEncoding::Base64));
        assert_eq!(entry.last_commit_when.is_some(), mode.is_none());
        assert_eq!(entry.last_author_date.is_some(), mode.is_some());
        assert_eq!(entry.last_committer_date.is_some(), mode.is_some());
        let links = entry.links.as_ref().unwrap();
        assert_eq!(links.self_url, entry.url);
        assert_eq!(links.git, entry.git_url);
        assert_eq!(links.html, entry.html_url);
        let encoded = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            amiss_wire::read_json::<ContentResponse>(&encoded, u64::MAX).unwrap(),
            response
        );
    }
}

#[test]
fn declared_entry_kinds_and_optional_metadata_remain_typed() {
    let ContentResponse::Entry(original) = amiss_wire::read_json::<ContentResponse>(
        include_bytes!("fixtures/gitea-file.json"),
        u64::MAX,
    )
    .unwrap() else {
        panic!("the capture names one file");
    };
    for (kind, mode, target, submodule) in [
        (ContentKind::Dir, GitMode::Tree, None, None),
        (
            ContentKind::Symlink,
            GitMode::Symlink,
            Some("../target.md"),
            None,
        ),
        (
            ContentKind::Submodule,
            GitMode::Gitlink,
            None,
            Some("https://example.com/repo.git"),
        ),
    ] {
        let mut entry = *original.clone();
        entry.kind = kind;
        entry.mode = Some(mode);
        entry.size = 0;
        entry.content = None;
        entry.encoding = None;
        entry.target = target.map(str::to_owned);
        entry.submodule_git_url = submodule.map(str::to_owned);
        let response = ContentResponse::Directory(vec![entry]);
        let encoded = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            amiss_wire::read_json::<ContentResponse>(&encoded, u64::MAX).unwrap(),
            response
        );
    }
    let mut file = original;
    file.last_commit_message = Some("Retain optional metadata".to_owned());
    file.lfs_oid = Some("a".repeat(64));
    file.lfs_size = Some(js_int::uint!(17));
    let response = ContentResponse::Entry(file);
    let encoded = serde_json::to_vec(&response).unwrap();
    assert_eq!(
        amiss_wire::read_json::<ContentResponse>(&encoded, u64::MAX).unwrap(),
        response
    );
    assert_eq!(
        amiss_wire::read_json::<ContentResponse>(b"[]", 2).unwrap(),
        ContentResponse::Directory(Vec::new())
    );
}

#[test]
fn content_ingress_refuses_unknown_or_malformed_payloads() {
    let input = include_str!("fixtures/gitea-file.json");
    for (original, replacement) in [
        (
            r#""name":".dockerignore""#,
            r#""name":".dockerignore","unknown":true"#,
        ),
        (r#""_links":{"#, r#""_links":{"unknown":true,"#),
        (r#""type":"file""#, r#""type":"unknown""#),
        (r#""type":"file""#, r#""type":null"#),
        (r#""encoding":"base64""#, r#""encoding":"rot13""#),
        (r#""target":null,"#, ""),
        (r#""encoding":"base64","#, ""),
        (r#""name":".dockerignore","#, ""),
        (r#""content":"RG9ja2VyZmlsZQp0ZWEK""#, r#""content":[]"#),
        (
            "d586e98b5abce19e9bd24bd509af35cddbac7743",
            "not-an-object-id",
        ),
        (
            "555f1ae516acccb818f1510af58e6098f24f3c42",
            "not-an-object-id",
        ),
        (r#""mode":"100644""#, r#""mode":"100645""#),
        (r#""mode":"100644""#, r#""mode":null"#),
        (r#""size":15"#, r#""size":-1"#),
        (r#""size":15"#, r#""size":9007199254740992"#),
        (r#""size":15"#, r#""size":15,"size":15"#),
        (r#""size":15"#, r#""size":15,"\u0073ize":15"#),
    ] {
        let invalid = input.replace(original, replacement);
        assert_ne!(invalid, input);
        assert_eq!(
            decode_bounded_json::<ContentResponse, _>(
                invalid.as_bytes(),
                None,
                invalid.len(),
                |bytes| { amiss_wire::read_json(bytes, u64::MAX) }
            ),
            Err(ProviderError::InvalidResponse),
            "{invalid}"
        );
    }
    let prefix = input.split_once(r#""_links":"#).unwrap().0;
    for invalid in [
        "null".to_owned(),
        "true".to_owned(),
        "42".to_owned(),
        "{}".to_owned(),
        "[null]".to_owned(),
        "[{}]".to_owned(),
        format!(r#"{prefix}"_links":[null,null,null]}}"#),
    ] {
        assert_eq!(
            decode_bounded_json::<ContentResponse, _>(
                invalid.as_bytes(),
                None,
                invalid.len(),
                |bytes| { amiss_wire::read_json(bytes, u64::MAX) }
            ),
            Err(ProviderError::InvalidResponse),
            "{invalid}"
        );
    }
}
