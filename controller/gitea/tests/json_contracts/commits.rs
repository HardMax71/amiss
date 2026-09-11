use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::commit::CommitRecord;

#[test]
fn captured_commits_keep_only_the_consumed_graph() {
    for input in [
        include_str!("../fixtures/gitea-commit-full.json"),
        include_str!("../fixtures/forgejo-commit-full.json"),
        include_str!("../fixtures/gitea-commit-disabled.json"),
        include_str!("../fixtures/forgejo-commit-disabled.json"),
    ] {
        let (commit, length): (CommitRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(commit.parents.len(), 1);
        assert_eq!(commit.commit.tree.sha, commit.sha);
        assert_ne!(commit.parents[0].sha, commit.sha);
        let minimal = serde_json::to_vec(&commit).unwrap();
        assert!(minimal.len() < input.len());
        assert_eq!(
            serde_json::from_slice::<CommitRecord>(&minimal).unwrap(),
            commit
        );
        for (old, new) in [
            (
                r#""html_url":"#,
                r#""extra":{"future":[null,true]},"html_url":"#,
            ),
            (
                r#""commit":{"#,
                r#""commit":{"extra":{"future":[null,true]},"#,
            ),
            (r#""tree":{"#, r#""tree":{"extra":{"future":[null,true]},"#),
            (
                r#""parents":[{"#,
                r#""parents":[{"extra":{"future":[null,true]},"#,
            ),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert_eq!(
                serde_json::from_str::<CommitRecord>(&changed).unwrap(),
                commit
            );
        }
        let positional =
            serde_json::to_vec(&(&commit.sha, &commit.commit, &commit.parents)).unwrap();
        assert_eq!(
            serde_json::from_slice::<CommitRecord>(&positional).unwrap(),
            commit
        );
    }
}

#[test]
fn commit_graphs_reject_missing_invalid_and_duplicate_consumed_fields() {
    let input = include_str!("../fixtures/gitea-commit-full.json");
    for (old, new) in [
        (
            r#""sha":"555f1ae516acccb818f1510af58e6098f24f3c42""#,
            r#""sha":"not-an-id""#,
        ),
        (
            r#""sha":"1c690c5ff862f85ef44b90a5e8f426b669b0128c""#,
            r#""sha":"not-an-id""#,
        ),
        (r#""sha":"#, r#""sha":null,"sha":"#),
        (r#""sha":"#, r#""missing_sha":"#),
        (r#""commit":"#, r#""missing_commit":"#),
        (r#""tree":"#, r#""missing_tree":"#),
        (r#""parents":"#, r#""missing_parents":"#),
    ] {
        let invalid = input.replace(old, new);
        assert_ne!(invalid, input);
        let page = format!("[{invalid}]");
        assert_eq!(
            decode_bounded_json::<Vec<CommitRecord>, _>(
                page.as_bytes(),
                None,
                page.len(),
                |bytes| serde_json::from_slice(bytes)
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    for invalid in ["null", "{}", "[null]", "[true]", "[{}]", "[[]]"] {
        assert!(serde_json::from_str::<Vec<CommitRecord>>(invalid).is_err());
    }
    for suffix in [" {}", " trailing"] {
        assert!(serde_json::from_str::<CommitRecord>(&format!("{input}{suffix}")).is_err());
    }
    assert_eq!(
        decode_bounded_json::<CommitRecord, _>(input.as_bytes(), None, input.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        }),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn commit_inputs_do_not_require_unused_metadata_or_nonempty_parents() {
    let input = include_str!("../fixtures/gitea-commit-full.json");
    let mut commit: CommitRecord = serde_json::from_str(input).unwrap();
    commit.parents.clear();
    let minimal = serde_json::to_string(&commit).unwrap();
    assert_eq!(
        serde_json::from_str::<CommitRecord>(&minimal).unwrap(),
        commit
    );
    assert!(
        serde_json::from_str::<Vec<CommitRecord>>("[]")
            .unwrap()
            .is_empty()
    );
    for (old, new) in [
        (r#""verification":{"#, r#""verification":{"unknown":true,"#),
        (
            r#""stats":{"total":58,"additions":27,"deletions":31}"#,
            r#""stats":"unused""#,
        ),
        (r#""author":null,"#, ""),
        (r#""status":"added""#, r#""status":{"unrelated":true}"#),
    ] {
        let changed = input.replace(old, new);
        assert_ne!(changed, input);
        assert_eq!(
            serde_json::from_str::<CommitRecord>(&changed).unwrap(),
            serde_json::from_str::<CommitRecord>(input).unwrap()
        );
    }
}
