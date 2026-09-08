use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::commit::{CommitFileStatus, CommitRecord, CommitSigner};

#[test]
fn captured_commits_retain_accounts_and_disabled_metadata() {
    for (input, metadata) in [
        (
            include_bytes!("../fixtures/gitea-commit-full.json").as_slice(),
            true,
        ),
        (
            include_bytes!("../fixtures/forgejo-commit-full.json").as_slice(),
            true,
        ),
        (
            include_bytes!("../fixtures/gitea-commit-disabled.json").as_slice(),
            false,
        ),
        (
            include_bytes!("../fixtures/forgejo-commit-disabled.json").as_slice(),
            false,
        ),
    ] {
        let (commit, length): (CommitRecord, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(commit.files.is_some(), metadata);
        assert_eq!(commit.stats.is_some(), metadata);
        assert_eq!(commit.commit.verification.is_some(), metadata);
        assert_eq!(commit.parents.len(), 1);
        assert_eq!(commit.parents[0].created, "0001-01-01T00:00:00Z");
        assert_eq!(commit.commit.tree.sha, commit.sha);
        assert_ne!(commit.parents[0].sha, commit.sha);
        let committer = commit.committer.as_ref().unwrap();
        assert_eq!(commit.author.is_some(), committer.pronouns.is_some());
        if let Some(stats) = &commit.stats {
            assert_eq!(stats.total, stats.additions + stats.deletions);
        }
        let page = vec![commit];
        let encoded = serde_json::to_vec(&page).unwrap();
        let (decoded, _) = decode_bounded_json::<Vec<CommitRecord>, _>(
            encoded.as_slice(),
            None,
            encoded.len(),
            |bytes| amiss_wire::read_json(bytes, u64::MAX),
        )
        .unwrap();
        assert_eq!(decoded, page);
    }
    let mut commit: CommitRecord = amiss_wire::read_json(
        include_bytes!("../fixtures/gitea-commit-full.json"),
        u64::MAX,
    )
    .unwrap();
    commit.parents.clear();
    let verification = commit.commit.verification.as_mut().unwrap();
    verification.signer = Some(CommitSigner {
        name: "Fixture signer".to_owned(),
        email: "signer@example.com".to_owned(),
        username: "signer".to_owned(),
    });
    for status in [
        CommitFileStatus::Added,
        CommitFileStatus::Removed,
        CommitFileStatus::Modified,
    ] {
        commit.files.as_mut().unwrap()[0].status = status;
        let encoded = serde_json::to_vec(&commit).unwrap();
        assert_eq!(
            amiss_wire::read_json::<CommitRecord>(&encoded, u64::MAX).unwrap(),
            commit
        );
    }
    assert!(
        amiss_wire::read_json::<Vec<CommitRecord>>(b"[]", 2)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_commit_page_cannot_hide_malformed_or_discarded_records() {
    let input = include_str!("../fixtures/gitea-commit-full.json");
    for (original, replacement) in [
        (r#""html_url":"#, r#""unknown":true,"html_url":"#),
        (r#""commit":{"#, r#""commit":{"unknown":true,"#),
        (
            r#""author":{"name":"#,
            r#""author":{"unknown":true,"name":"#,
        ),
        (r#""tree":{"#, r#""tree":{"unknown":true,"#),
        (r#""verification":{"#, r#""verification":{"unknown":true,"#),
        (
            r#""committer":{"id":"#,
            r#""committer":{"unknown":true,"id":"#,
        ),
        (r#""parents":[{"#, r#""parents":[{"unknown":true,"#),
        (r#""files":[{"#, r#""files":[{"unknown":true,"#),
        (r#""stats":{"#, r#""stats":{"unknown":true,"#),
        (
            r#""stats":{"total":58,"additions":27,"deletions":31}"#,
            r#""stats":[58,27,31]"#,
        ),
        (
            r#""sha":"555f1ae516acccb818f1510af58e6098f24f3c42""#,
            r#""sha":"not-an-id""#,
        ),
        (
            r#""sha":"1c690c5ff862f85ef44b90a5e8f426b669b0128c""#,
            r#""sha":"not-an-id""#,
        ),
        (r#""parents":"#, r#""missing_parents":"#),
        (r#""author":null,"#, ""),
        (r#""signer":null"#, r#""signer":{}"#),
        (r#""status":"added""#, r#""status":"unknown""#),
        (r#""verified":false"#, r#""verified":"false""#),
        (r#""total":58"#, r#""total":-1"#),
        (r#""total":58"#, r#""total":9007199254740992"#),
        (r#""total":58"#, r#""total":58,"\u0074otal":58"#),
    ] {
        let invalid = input.replace(original, replacement);
        assert_ne!(invalid, input);
        let page = format!("[{invalid}]");
        assert_eq!(
            decode_bounded_json::<Vec<CommitRecord>, _>(
                page.as_bytes(),
                None,
                page.len(),
                |bytes| amiss_wire::read_json(bytes, u64::MAX)
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    let disabled = include_str!("../fixtures/gitea-commit-disabled.json");
    for missing in [
        r#""files":null,"#,
        r#","stats":null"#,
        r#","verification":null"#,
    ] {
        let invalid = disabled.replace(missing, "");
        assert_ne!(invalid, disabled);
        assert!(amiss_wire::read_json::<CommitRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
    for invalid in ["null", "{}", "[null]", "[true]", "[{}]", "[[]]"] {
        assert!(amiss_wire::read_json::<Vec<CommitRecord>>(invalid.as_bytes(), u64::MAX).is_err());
    }
}
