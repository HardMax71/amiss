use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::branch::{BranchRecord, PayloadCommitRecord};

#[test]
fn branch_captures_retain_all_commit_and_control_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    for (input, name, sha, contexts) in [
        (
            include_str!("../fixtures/gitea-branch.json"),
            "main",
            "58931b5d170d54e3f7e8da9873404f6311e8ecd6",
            1,
        ),
        (
            include_str!("../fixtures/forgejo-branch.json"),
            "forgejo",
            "ee74d47e1e302a1f129b0ce4c6b2584503a13790",
            6,
        ),
    ] {
        let (branch, length): (BranchRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(branch.name, name);
        assert!(branch.protected);
        assert_eq!(branch.required_approvals, 1);
        assert!(branch.enable_status_check);
        assert_eq!(
            branch
                .status_check_contexts
                .as_ref()
                .ok_or("missing contexts")?
                .len(),
            contexts
        );
        assert!(!branch.user_can_push);
        assert!(!branch.user_can_merge);
        let commit = branch.commit.as_ref().ok_or("missing commit")?;
        assert_eq!(commit.id.as_str(), sha);
        assert!(!commit.message.is_empty());
        assert!(commit.url.ends_with(sha));
        assert!(
            !commit
                .author
                .as_ref()
                .ok_or("missing author")?
                .username
                .is_empty()
        );
        assert!(
            !commit
                .committer
                .as_ref()
                .ok_or("missing committer")?
                .name
                .is_empty()
        );
        let verification = commit.verification.as_ref().ok_or("missing verification")?;
        assert!(!verification.verified);
        assert_eq!(verification.reason, "gpg.error.not_signed_commit");
        assert!(!commit.timestamp.is_empty());
        assert_eq!(
            (&commit.added, &commit.removed, &commit.modified),
            (&None, &None, &None)
        );
        assert_eq!(
            decode_bounded_json::<BranchRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| amiss_wire::read_json(bytes, u64::MAX)
            ),
            Err(ProviderError::InvalidResponse)
        );
        for (old, new) in [
            (r#""commit":{"#, r#""extra":true,"commit":{"#),
            (r#""timestamp":"#, r#""extra":true,"timestamp":"#),
            (r#""username":"#, r#""extra":true,"username":"#),
            (r#""verified":"#, r#""extra":true,"verified":"#),
            (r#""protected":"#, r#""protec\u0074ed":true,"protected":"#),
            (r#""user_can_push":false"#, r#""user_can_push":0"#),
            (
                r#""required_approvals":1"#,
                r#""required_approvals":9007199254740992"#,
            ),
            (
                r#""required_approvals":1"#,
                r#""required_approvals":-9007199254740992"#,
            ),
            (r#""required_approvals":1"#, r#""required_approvals":1.0"#),
            (r#""added":null"#, r#""added":[1]"#),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<BranchRecord>(&changed).is_err());
            assert!(amiss_wire::read_json::<BranchRecord>(changed.as_bytes(), u64::MAX).is_err());
        }
    }
    Ok(())
}

#[test]
fn branch_and_payload_fields_cannot_be_omitted() -> Result<(), Box<dyn std::error::Error>> {
    let branch: BranchRecord = serde_json::from_str(include_str!("../fixtures/gitea-branch.json"))?;
    let encoded = serde_json::to_string(&branch)?;
    for field in [
        format!("\"commit\":{},", serde_json::to_string(&branch.commit)?),
        format!(
            "\"status_check_contexts\":{},",
            serde_json::to_string(&branch.status_check_contexts)?
        ),
        "\"enable_status_check\":true,".to_owned(),
        "\"user_can_push\":false,".to_owned(),
        "\"user_can_merge\":false,".to_owned(),
    ] {
        let changed = encoded.replace(&field, "");
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<BranchRecord>(&changed).is_err());
    }
    let commit = branch.commit.ok_or("missing commit")?;
    let encoded = serde_json::to_string(&commit)?;
    for invalid in [
        "g".repeat(40),
        "A".repeat(40),
        "a".repeat(39),
        "a".repeat(41),
    ] {
        let changed = encoded.replace(commit.id.as_str(), &invalid);
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<PayloadCommitRecord>(&changed).is_err());
    }
    for field in [
        format!("\"message\":{},", serde_json::to_string(&commit.message)?),
        format!("\"url\":{},", serde_json::to_string(&commit.url)?),
        format!("\"author\":{},", serde_json::to_string(&commit.author)?),
        format!(
            "\"committer\":{},",
            serde_json::to_string(&commit.committer)?
        ),
        format!(
            "\"verification\":{},",
            serde_json::to_string(&commit.verification)?
        ),
        format!(
            "\"timestamp\":{},",
            serde_json::to_string(&commit.timestamp)?
        ),
        ",\"added\":null".to_owned(),
        ",\"removed\":null".to_owned(),
        ",\"modified\":null".to_owned(),
    ] {
        let changed = encoded.replace(&field, "");
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<PayloadCommitRecord>(&changed).is_err());
    }
    Ok(())
}

#[test]
fn nullable_branch_metadata_is_explicit_and_lossless() -> Result<(), Box<dyn std::error::Error>> {
    let branch: BranchRecord =
        serde_json::from_slice(include_bytes!("../fixtures/forgejo-branch.json"))?;
    let commit = branch.commit.as_ref().ok_or("missing commit")?;
    for files in [None, Some(Vec::new()), Some(vec!["src/lib.rs".to_owned()])] {
        let payload = PayloadCommitRecord {
            id: "a".repeat(64).parse()?,
            author: None,
            committer: None,
            verification: None,
            added: files.clone(),
            removed: files.clone(),
            modified: files.clone(),
            ..commit.clone()
        };
        for commit in [None, Some(payload)] {
            let response = BranchRecord {
                commit,
                status_check_contexts: files.clone(),
                required_approvals: js_int::MAX_SAFE_INT,
                ..branch.clone()
            };
            let encoded = serde_json::to_vec(&response)?;
            assert_eq!(
                amiss_wire::read_json::<BranchRecord>(&encoded, u64::MAX)?,
                response
            );
        }
    }
    let positional = serde_json::to_vec(&(
        &branch.name,
        &branch.commit,
        branch.protected,
        branch.required_approvals,
        branch.enable_status_check,
        &branch.status_check_contexts,
        branch.user_can_push,
        branch.user_can_merge,
        &branch.effective_branch_protection_name,
    ))?;
    assert_eq!(serde_json::from_slice::<BranchRecord>(&positional)?, branch);
    assert!(amiss_wire::read_json::<BranchRecord>(&positional, u64::MAX).is_err());
    let minimum = BranchRecord {
        required_approvals: js_int::MIN_SAFE_INT,
        ..branch.clone()
    };
    assert_eq!(
        amiss_wire::read_json::<BranchRecord>(&serde_json::to_vec(&minimum)?, u64::MAX)?,
        minimum
    );
    for required_approvals in [js_int::MIN_SAFE_INT - 1, js_int::MAX_SAFE_INT + 1] {
        let response = BranchRecord {
            required_approvals,
            ..branch.clone()
        };
        assert!(serde_json::to_vec(&response).is_err());
    }
    Ok(())
}
