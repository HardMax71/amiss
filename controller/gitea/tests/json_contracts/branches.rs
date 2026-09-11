use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::branch::{BranchRecord, PayloadCommitRecord};

#[test]
fn branch_captures_keep_the_tip_and_effective_protection() -> Result<(), Box<dyn std::error::Error>>
{
    for (input, name, sha) in [
        (
            include_str!("../fixtures/gitea-branch.json"),
            "main",
            "58931b5d170d54e3f7e8da9873404f6311e8ecd6",
        ),
        (
            include_str!("../fixtures/forgejo-branch.json"),
            "forgejo",
            "ee74d47e1e302a1f129b0ce4c6b2584503a13790",
        ),
    ] {
        let (branch, length): (BranchRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(branch.name, name);
        assert!(branch.protected);
        assert_eq!(branch.required_approvals, 1);
        assert!(branch.effective_branch_protection_name.is_empty());
        assert_eq!(
            branch.commit.as_ref().ok_or("missing commit")?.id.as_str(),
            sha
        );
        assert_eq!(
            decode_bounded_json::<BranchRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
        let minimal = serde_json::to_string(&branch)?;
        let metadata = minimal
            .replacen('{', r#"{"extra":true,"enable_status_check":null,"status_check_contexts":{},"user_can_push":42,"user_can_merge":[],"#, 1)
            .replacen(r#""commit":{"#, r#""commit":{"message":null,"author":true,"verification":[],"added":42,"#, 1);
        assert_eq!(serde_json::from_str::<BranchRecord>(&metadata)?, branch);
        for field in [
            "name",
            "commit",
            "protected",
            "required_approvals",
            "effective_branch_protection_name",
        ] {
            let missing = minimal.replacen(
                &format!(r#""{field}":"#),
                &format!(r#""missing_{field}":"#),
                1,
            );
            assert_ne!(missing, minimal);
            assert!(
                serde_json::from_str::<BranchRecord>(&missing).is_err(),
                "{field}"
            );
        }
        for (old, new) in [
            (r#""protected":true"#, r#""protected":null"#),
            (
                r#""protected":true"#,
                r#""protected":true,"protec\u0074ed":true"#,
            ),
            (
                r#""required_approvals":1"#,
                r#""required_approvals":9007199254740992"#,
            ),
            (
                r#""required_approvals":1"#,
                r#""required_approvals":-9007199254740992"#,
            ),
            (r#""required_approvals":1"#, r#""required_approvals":1.0"#),
            (r#""commit":{"#, r#""commit":{"id":null,"#),
        ] {
            let invalid = minimal.replacen(old, new, 1);
            assert_ne!(invalid, minimal);
            assert!(
                serde_json::from_str::<BranchRecord>(&invalid).is_err(),
                "{new}"
            );
        }
    }
    Ok(())
}

#[test]
fn branch_commit_ids_and_approval_counts_remain_checked() -> Result<(), Box<dyn std::error::Error>>
{
    let mut branch: BranchRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-branch.json"))?;
    let commit = branch.commit.as_ref().ok_or("missing commit")?;
    let encoded = serde_json::to_string(commit)?;
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
    assert!(serde_json::from_str::<PayloadCommitRecord>("{}").is_err());
    for commit in [
        None,
        Some(PayloadCommitRecord {
            id: "a".repeat(64).parse()?,
        }),
    ] {
        branch.commit = commit;
        for required_approvals in [js_int::MIN_SAFE_INT, js_int::MAX_SAFE_INT] {
            branch.required_approvals = required_approvals;
            let wire = serde_json::to_vec(&branch)?;
            assert_eq!(serde_json::from_slice::<BranchRecord>(&wire)?, branch);
            let positional = serde_json::to_vec(&(
                &branch.name,
                &branch.commit,
                branch.protected,
                branch.required_approvals,
                &branch.effective_branch_protection_name,
            ))?;
            assert_eq!(serde_json::from_slice::<BranchRecord>(&positional)?, branch);
        }
    }
    for required_approvals in [js_int::MIN_SAFE_INT - 1, js_int::MAX_SAFE_INT + 1] {
        branch.required_approvals = required_approvals;
        assert!(serde_json::to_vec(&branch).is_err());
    }
    Ok(())
}
