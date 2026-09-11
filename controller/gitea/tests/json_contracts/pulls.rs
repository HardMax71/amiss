use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::issue::{IssueState, Label, Milestone};
use amiss_controller_gitea::pull::{PullRefRecord, PullRequestRecord};

#[test]
fn live_pulls_retain_complete_provider_and_branch_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, number, additions, gitea) in [
        (
            include_str!("../fixtures/gitea-pull.json"),
            1113,
            1462_u16,
            true,
        ),
        (
            include_str!("../fixtures/forgejo-pull.json"),
            14286,
            210,
            false,
        ),
    ] {
        let (pull, length): (PullRequestRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(pull.number, number);
        assert_eq!(pull.state, IssueState::Open);
        assert_eq!(pull.additions, Some(additions.into()));
        assert_eq!(pull.content_version, gitea.then_some(1_u8.into()));
        assert_eq!(pull.flow, (!gitea).then_some(0.into()));
        assert!(pull.user.is_some());
        assert!(!pull.title.is_empty());
        assert!(!pull.body.is_empty());
        assert!(pull.created_at.is_some());
        assert!(pull.updated_at.is_some());
        assert_eq!(pull.base.sha, pull.merge_base);
        for branch in [&pull.base, &pull.head] {
            let repo = branch.repo.as_ref().ok_or("missing repository")?;
            assert_eq!(u64::try_from(branch.repo_id)?, repo.id);
            assert_eq!(branch.label, branch.branch);
            assert!(branch.sha.is_some());
        }
        let encoded = serde_json::to_vec(&pull)?;
        assert_eq!(
            amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX)?,
            pull
        );
        assert_eq!(
            decode_bounded_json::<PullRequestRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| amiss_wire::read_json(bytes, u64::MAX),
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    Ok(())
}

#[test]
fn pull_nullability_and_omitted_counts_do_not_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let original: PullRequestRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-pull.json"))?;
    let mut empty = PullRequestRecord {
        user: None,
        labels: None,
        milestone: None,
        assignee: None,
        assignees: None,
        requested_reviewers: None,
        requested_reviewers_teams: None,
        merged_at: None,
        merge_commit_sha: None,
        merged_by: None,
        due_date: None,
        created_at: None,
        updated_at: None,
        closed_at: None,
        review_comments: None,
        additions: None,
        deletions: None,
        changed_files: None,
        content_version: None,
        flow: None,
        merge_base: None,
        ..original
    };
    let encoded = serde_json::to_string(&empty)?;
    assert!(encoded.contains("\"merge_base\":\"\""));
    let null = encoded.replacen("\"merge_base\":\"\"", "\"merge_base\":null", 1);
    assert!(amiss_wire::read_json::<PullRequestRecord>(null.as_bytes(), u64::MAX).is_err());
    assert_eq!(
        amiss_wire::read_json::<PullRequestRecord>(encoded.as_bytes(), u64::MAX)?,
        empty
    );
    for field in [
        "user",
        "labels",
        "milestone",
        "assignee",
        "assignees",
        "requested_reviewers",
        "requested_reviewers_teams",
        "merged_at",
        "merge_commit_sha",
        "merged_by",
        "due_date",
        "created_at",
        "updated_at",
        "closed_at",
    ] {
        let changed = encoded.replacen(&format!("\"{field}\":null,"), "", 1);
        assert_ne!(changed, encoded);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
            "{field}"
        );
    }
    for field in [
        "review_comments",
        "additions",
        "deletions",
        "changed_files",
        "content_version",
        "flow",
    ] {
        let changed = encoded.replacen("\"title\":", &format!("\"{field}\":null,\"title\":"), 1);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
            "{field}"
        );
    }
    empty.labels = Some(Vec::new());
    empty.assignees = Some(Vec::new());
    empty.requested_reviewers = Some(Vec::new());
    empty.requested_reviewers_teams = Some(Vec::new());
    empty.review_comments = Some(0_u8.into());
    empty.merge_base = empty.base.sha.clone();
    empty.merge_commit_sha = empty.base.sha.clone();
    let encoded = serde_json::to_vec(&empty)?;
    assert_eq!(
        amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX)?,
        empty
    );
    Ok(())
}

#[test]
fn pull_inputs_reject_unknown_members_and_invalid_scalar_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    for input in [
        include_str!("../fixtures/gitea-pull.json"),
        include_str!("../fixtures/forgejo-pull.json"),
    ] {
        for (old, new) in [
            ("\"title\":", "\"unknown\":true,\"title\":"),
            ("\"comments\":", "\"comm\\u0065nts\":0,\"comments\":"),
            ("\"is_locked\":false,", ""),
            ("\"state\":\"open\"", "\"state\":\"all\""),
            ("\"state\":\"open\"", "\"state\":{\"open\":null}"),
            ("\"mergeable\":true", "\"mergeable\":null"),
            ("\"review_comments\":0", "\"review_comments\":-1"),
            (
                "\"review_comments\":0",
                "\"review_comments\":9007199254740992",
            ),
            ("\"pin_order\":0", "\"pin_order\":9007199254740992"),
            ("\"pin_order\":0", "\"pin_order\":1.5"),
        ] {
            let changed = input.replacen(old, new, 1);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<PullRequestRecord>(&changed).is_err());
            assert!(
                amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err()
            );
        }
    }
    let mut pull: PullRequestRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-pull.json"))?;
    for id in [0, js_int::MAX_SAFE_UINT] {
        pull.id = id;
        let encoded = serde_json::to_vec(&pull)?;
        assert_eq!(
            amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX)?,
            pull
        );
    }
    pull.id = js_int::MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&pull).is_err());
    Ok(())
}

#[test]
fn pull_refs_preserve_deleted_repository_and_missing_commit_data()
-> Result<(), Box<dyn std::error::Error>> {
    let mut branch = PullRefRecord {
        label: "topic".to_owned(),
        branch: "topic".to_owned(),
        sha: None,
        repo_id: -1,
        repo: None,
    };
    let encoded = serde_json::to_string(&branch)?;
    assert!(encoded.contains("\"sha\":\"\""));
    let null = encoded.replacen("\"sha\":\"\"", "\"sha\":null", 1);
    assert!(amiss_wire::read_json::<PullRefRecord>(null.as_bytes(), u64::MAX).is_err());
    assert_eq!(
        amiss_wire::read_json::<PullRefRecord>(encoded.as_bytes(), u64::MAX)?,
        branch
    );
    for (old, new) in [
        ("\"label\":", "\"unknown\":true,\"label\":"),
        ("\"label\":\"topic\",", ""),
        ("\"sha\":\"\",", ""),
        ("\"sha\":\"\"", "\"sha\":\"invalid\""),
        ("\"repo\":null", "\"repo\":{}"),
        ("\"repo_id\":-1", "\"repo_id\":9007199254740992"),
        ("\"repo_id\":-1", "\"repo_id\":-9007199254740992"),
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
        let encoded = serde_json::to_vec(&branch)?;
        assert_eq!(
            amiss_wire::read_json::<PullRefRecord>(&encoded, u64::MAX)?,
            branch
        );
    }
    branch.repo_id = js_int::MAX_SAFE_INT + 1;
    assert!(serde_json::to_vec(&branch).is_err());
    Ok(())
}

#[test]
fn nested_issue_metadata_stays_closed_and_uses_one_state_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let original: PullRequestRecord =
        serde_json::from_slice(include_bytes!("../fixtures/forgejo-pull.json"))?;
    let label: &Label = original
        .labels
        .as_ref()
        .and_then(|labels| labels.first())
        .ok_or("missing label")?;
    let encoded = serde_json::to_string(label)?;
    for (old, new) in [
        ("\"id\":", "\"unknown\":true,\"id\":"),
        ("\"exclusive\":false,", ""),
    ] {
        let changed = encoded.replacen(old, new, 1);
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<Label>(&changed).is_err());
    }
    for state in [IssueState::Open, IssueState::Closed] {
        let milestone = Milestone {
            id: 1,
            title: "next".to_owned(),
            description: String::new(),
            state,
            open_issues: 1,
            closed_issues: 0,
            created_at: "2026-09-01T00:00:00Z".to_owned(),
            updated_at: None,
            closed_at: None,
            due_on: None,
        };
        let encoded = serde_json::to_string(&milestone)?;
        assert_eq!(
            amiss_wire::read_json::<Milestone>(encoded.as_bytes(), u64::MAX)?,
            milestone
        );
        for (old, new) in [
            ("\"id\":", "\"unknown\":true,\"id\":"),
            ("\"updated_at\":null,", ""),
            ("\"closed_at\":null,", ""),
            ("\"open_issues\":1", "\"open_issues\":-1"),
        ] {
            let changed = encoded.replacen(old, new, 1);
            assert_ne!(changed, encoded);
            assert!(serde_json::from_str::<Milestone>(&changed).is_err());
        }
        let populated = PullRequestRecord {
            milestone: Some(milestone),
            state,
            ..original.clone()
        };
        let encoded = serde_json::to_vec(&populated)?;
        assert_eq!(
            amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX)?,
            populated
        );
    }
    Ok(())
}
