use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::issue::IssueState;
use amiss_controller_gitea::pull::{PullRefRecord, PullRequestRecord};

#[test]
fn live_pulls_consume_identity_refs_and_merge_state() -> Result<(), Box<dyn std::error::Error>> {
    for (input, number) in [
        (include_str!("../fixtures/gitea-pull.json"), 1113),
        (include_str!("../fixtures/forgejo-pull.json"), 14286),
    ] {
        let (pull, length): (PullRequestRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(pull.number, number);
        assert_eq!(pull.state, IssueState::Open);
        assert!(pull.mergeable);
        assert!(!pull.merged);
        assert_eq!(pull.base.sha, pull.merge_base);
        for branch in [&pull.base, &pull.head] {
            let repo = branch.repo.as_ref().ok_or("missing repository")?;
            assert_eq!(u64::try_from(branch.repo_id)?, repo.id);
            assert!(!branch.branch.is_empty());
            assert!(branch.sha.is_some());
        }
        let minimal = serde_json::to_string(&pull)?;
        let metadata = minimal.replacen(
            '{',
            r#"{"title":false,"body":[],"labels":42,"milestone":true,"review_comments":{},"extra":null,"#,
            1,
        );
        assert_eq!(serde_json::from_str::<PullRequestRecord>(&metadata)?, pull);
        let positional = serde_json::to_vec(&(
            pull.id,
            pull.number,
            pull.state,
            pull.mergeable,
            pull.merged,
            &pull.base,
            &pull.head,
            &pull.merge_base,
        ))?;
        assert_eq!(
            serde_json::from_slice::<PullRequestRecord>(&positional)?,
            pull
        );
        assert_eq!(
            decode_bounded_json::<PullRequestRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
        for (field, value) in [
            ("id", pull.id.to_string()),
            ("number", pull.number.to_string()),
            ("state", serde_json::to_string(&pull.state)?),
            ("mergeable", pull.mergeable.to_string()),
            ("merged", pull.merged.to_string()),
            ("base", serde_json::to_string(&pull.base)?),
            ("head", serde_json::to_string(&pull.head)?),
        ] {
            for replacement in [
                format!(r#""missing_{field}":{value}"#),
                format!(r#""{field}":null"#),
                format!(r#""{field}":{value},"{field}":{value}"#),
            ] {
                let invalid = minimal.replacen(&format!(r#""{field}":{value}"#), &replacement, 1);
                assert_ne!(invalid, minimal, "{field}");
                assert!(
                    serde_json::from_str::<PullRequestRecord>(&invalid).is_err(),
                    "{field}"
                );
            }
        }
        for (old, new) in [
            (r#""state":"open""#, r#""state":"all""#),
            (r#""state":"open""#, r#""state":{"open":null}"#),
            (r#""mergeable":true"#, r#""mergeable":"true""#),
            (r#""merged":false"#, r#""merged":0"#),
            (r#""merge_base":"#, r#""missing_merge_base":"#),
        ] {
            let invalid = minimal.replacen(old, new, 1);
            assert_ne!(invalid, minimal, "{old}");
            assert!(
                serde_json::from_str::<PullRequestRecord>(&invalid).is_err(),
                "{new}"
            );
        }
    }
    Ok(())
}

#[test]
fn pull_identity_numbers_remain_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut pull: PullRequestRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-pull.json"))?;
    for bound in [0, js_int::MAX_SAFE_UINT] {
        pull.id = bound;
        pull.number = bound;
        let wire = serde_json::to_string(&pull)?;
        assert_eq!(serde_json::from_str::<PullRequestRecord>(&wire)?, pull);
        for field in ["id", "number"] {
            for invalid in ["9007199254740992", "-1", "1.5", r#""1""#, "false"] {
                let changed = wire.replacen(
                    &format!(r#""{field}":{bound}"#),
                    &format!(r#""{field}":{invalid}"#),
                    1,
                );
                assert_ne!(changed, wire);
                assert!(
                    serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
                    "{field}: {invalid}"
                );
            }
        }
    }
    pull.id = js_int::MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&pull).is_err());
    Ok(())
}

#[test]
fn pull_refs_preserve_deleted_repository_and_missing_commit_data()
-> Result<(), Box<dyn std::error::Error>> {
    let mut branch = PullRefRecord {
        branch: "topic".to_owned(),
        sha: None,
        repo_id: -1,
        repo: None,
    };
    let encoded = serde_json::to_string(&branch)?;
    assert!(encoded.contains(r#""sha":"""#));
    for changed in [
        encoded.replacen(r#""sha":"""#, r#""sha":null"#, 1),
        encoded.replacen(r#","repo":null"#, "", 1),
        encoded.replacen('{', r#"{"label":true,"extra":[],"#, 1),
    ] {
        assert_ne!(changed, encoded);
        assert_eq!(serde_json::from_str::<PullRefRecord>(&changed)?, branch);
    }
    for (old, new) in [
        (r#""ref":"topic","#, ""),
        (r#""sha":"","#, ""),
        (r#""sha":"""#, r#""sha":"invalid""#),
        (r#""sha":"""#, r#""sha":true"#),
        (r#""repo":null"#, r#""repo":{}"#),
        (r#""repo_id":-1"#, r#""repo_id":9007199254740992"#),
        (r#""repo_id":-1"#, r#""repo_id":-9007199254740992"#),
        (r#""repo_id":-1"#, r#""repo_id":null"#),
        (r#""repo_id":-1"#, r#""repo_id":-1,"\u0072epo_id":-1"#),
    ] {
        let changed = encoded.replacen(old, new, 1);
        assert_ne!(changed, encoded);
        assert!(
            serde_json::from_str::<PullRefRecord>(&changed).is_err(),
            "{old} -> {new}"
        );
    }
    for length in [40, 64] {
        branch.sha = Some("a".repeat(length).parse()?);
        let wire = serde_json::to_vec(&branch)?;
        assert_eq!(serde_json::from_slice::<PullRefRecord>(&wire)?, branch);
    }
    branch.repo_id = js_int::MAX_SAFE_INT + 1;
    assert!(serde_json::to_vec(&branch).is_err());
    let mut pull: PullRequestRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-pull.json"))?;
    pull.merge_base = None;
    let wire = serde_json::to_string(&pull)?;
    for empty in [r#""""#, "null"] {
        let changed = wire.replacen(r#""merge_base":"""#, &format!(r#""merge_base":{empty}"#), 1);
        assert_eq!(serde_json::from_str::<PullRequestRecord>(&changed)?, pull);
    }
    Ok(())
}
