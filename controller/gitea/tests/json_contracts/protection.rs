use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::protection::BranchProtectionRecord;

#[test]
fn protection_profiles_preserve_authority_and_ignore_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, gitea) in [
        (include_str!("../fixtures/gitea-protection.json"), true),
        (include_str!("../fixtures/forgejo-protection.json"), false),
    ] {
        let (mut record, length): (BranchProtectionRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(record.rule_name, "main");
        assert_eq!(record.priority, gitea.then_some(0.into()));
        assert_eq!(record.block_on_codeowner_reviews, gitea.then_some(false));
        assert_eq!(record.block_admin_merge_override, gitea.then_some(true));
        assert_eq!(record.apply_to_admins, (!gitea).then_some(true));
        let encoded = serde_json::to_string(&record)?;
        let metadata = encoded.replacen(
            '{',
            r#"{"branch_name":null,"enable_merge_whitelist":{},"status_check_contexts":false,"require_signed_commits":[],"created_at":42,"extra":{},"#,
            1,
        );
        assert_eq!(
            serde_json::from_str::<BranchProtectionRecord>(&metadata)?,
            record
        );
        assert_eq!(
            decode_bounded_json::<BranchProtectionRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
        for approvals in [js_int::MIN_SAFE_INT, js_int::MAX_SAFE_INT] {
            record.required_approvals = approvals;
            let encoded = serde_json::to_vec(&record)?;
            assert_eq!(
                serde_json::from_slice::<BranchProtectionRecord>(&encoded)?,
                record
            );
        }
        record.required_approvals = js_int::MAX_SAFE_INT + 1;
        assert!(serde_json::to_vec(&record).is_err());
    }
    Ok(())
}

#[test]
fn protection_records_reject_incomplete_and_wrongly_typed_authority() {
    for input in [
        include_str!("../fixtures/gitea-protection.json"),
        include_str!("../fixtures/forgejo-protection.json"),
    ] {
        amiss_fixtures::assert_json_rejections::<BranchProtectionRecord>(
            input,
            &[
                (r#""rule_name":"#, r#""ru\u006ce_name":"main","rule_name":"#),
                (r#""rule_name":"main","#, ""),
                (r#""enable_push":false,"#, ""),
                (r#""push_whitelist_usernames":[],"#, ""),
                (r#""push_whitelist_teams":[],"#, ""),
                (r#""enable_approvals_whitelist":true,"#, ""),
                (r#""block_on_rejected_reviews":true,"#, ""),
                (r#""block_on_outdated_branch":true,"#, ""),
                (r#""dismiss_stale_approvals":true,"#, ""),
                (r#""enable_push":false"#, r#""enable_push":null"#),
                (
                    r#""push_whitelist_usernames":[]"#,
                    r#""push_whitelist_usernames":null"#,
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
                (
                    r#""push_whitelist_teams":[]"#,
                    r#""push_whitelist_teams":[1]"#,
                ),
            ],
        );
    }
}

#[test]
fn optional_protection_capabilities_cannot_be_null() {
    let gitea = include_str!("../fixtures/gitea-protection.json");
    for (field, value) in [
        ("priority", "0"),
        ("enable_force_push", "false"),
        ("enable_force_push_allowlist", "false"),
        ("force_push_allowlist_usernames", "[]"),
        ("force_push_allowlist_teams", "[]"),
        ("force_push_allowlist_deploy_keys", "false"),
        ("enable_bypass_allowlist", "false"),
        ("bypass_allowlist_usernames", "[]"),
        ("bypass_allowlist_teams", "[]"),
        ("block_on_codeowner_reviews", "false"),
        ("block_admin_merge_override", "true"),
    ] {
        let changed = gitea.replace(
            &format!("\"{field}\":{value}"),
            &format!("\"{field}\":null"),
        );
        assert_ne!(changed, gitea);
        assert!(serde_json::from_str::<BranchProtectionRecord>(&changed).is_err());
    }
    let forgejo = include_str!("../fixtures/forgejo-protection.json");
    let changed = forgejo.replace(r#""apply_to_admins":true"#, r#""apply_to_admins":null"#);
    assert_ne!(changed, forgejo);
    assert!(serde_json::from_str::<BranchProtectionRecord>(&changed).is_err());
    for invalid in [
        "9007199254740992",
        "-9007199254740992",
        "1.5",
        "true",
        r#""0""#,
    ] {
        let changed = gitea.replace(r#""priority":0"#, &format!("\"priority\":{invalid}"));
        assert_ne!(changed, gitea);
        assert!(serde_json::from_str::<BranchProtectionRecord>(&changed).is_err());
    }
}
