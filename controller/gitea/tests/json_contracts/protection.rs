use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::protection::BranchProtectionRecord;

#[test]
fn protection_profiles_preserve_nullable_and_provider_specific_fields()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, gitea) in [
        (include_str!("../fixtures/gitea-protection.json"), true),
        (include_str!("../fixtures/forgejo-protection.json"), false),
    ] {
        let (mut record, length): (BranchProtectionRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(record.branch_name, "main");
        assert_eq!(record.rule_name, record.branch_name);
        assert_eq!(record.priority, gitea.then_some(0.into()));
        assert_eq!(record.block_on_codeowner_reviews, gitea.then_some(false));
        assert_eq!(record.block_admin_merge_override, gitea.then_some(true));
        assert_eq!(record.apply_to_admins, (!gitea).then_some(true));
        assert_eq!(record.created_at, "2026-09-01T00:00:00Z");
        assert_eq!(record.updated_at, record.created_at);
        assert!(!record.enable_merge_whitelist);
        assert!(record.merge_whitelist_usernames.is_empty());
        assert!(record.merge_whitelist_teams.is_empty());
        assert!(!record.block_on_official_review_requests);
        assert!(!record.require_signed_commits);
        assert_eq!(
            decode_bounded_json::<BranchProtectionRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| amiss_wire::read_json(bytes, u64::MAX),
            ),
            Err(ProviderError::InvalidResponse)
        );
        for contexts in [
            None,
            Some(Vec::new()),
            Some(vec!["required check".to_owned()]),
        ] {
            record.status_check_contexts = contexts;
            for approvals in [js_int::MIN_SAFE_INT, js_int::MAX_SAFE_INT] {
                record.required_approvals = approvals;
                let encoded = serde_json::to_vec(&record)?;
                assert_eq!(
                    amiss_wire::read_json::<BranchProtectionRecord>(&encoded, u64::MAX)?,
                    record
                );
            }
        }
        record.required_approvals = js_int::MAX_SAFE_INT + 1;
        assert!(serde_json::to_vec(&record).is_err());
    }
    Ok(())
}

#[test]
fn protection_records_reject_unknown_incomplete_and_wrongly_typed_fields() {
    for input in [
        include_str!("../fixtures/gitea-protection.json"),
        include_str!("../fixtures/forgejo-protection.json"),
    ] {
        for (old, new) in [
            (r#""rule_name":"#, r#""extra":true,"rule_name":"#),
            (r#""rule_name":"#, r#""ru\u006ce_name":"main","rule_name":"#),
            (r#""branch_name":"main","#, ""),
            (r#""enable_merge_whitelist":false,"#, ""),
            (r#""merge_whitelist_usernames":[],"#, ""),
            (r#""merge_whitelist_teams":[],"#, ""),
            (r#""enable_status_check":false,"#, ""),
            (r#""status_check_contexts":null,"#, ""),
            (r#""block_on_official_review_requests":false,"#, ""),
            (r#""require_signed_commits":false,"#, ""),
            (r#""created_at":"2026-09-01T00:00:00Z","#, ""),
            (r#""updated_at":"2026-09-01T00:00:00Z","#, ""),
            (
                r#""require_signed_commits":false"#,
                r#""require_signed_commits":null"#,
            ),
            (
                r#""merge_whitelist_usernames":[]"#,
                r#""merge_whitelist_usernames":null"#,
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
                r#""status_check_contexts":null"#,
                r#""status_check_contexts":[1]"#,
            ),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<BranchProtectionRecord>(&changed).is_err());
            assert!(
                amiss_wire::read_json::<BranchProtectionRecord>(changed.as_bytes(), u64::MAX)
                    .is_err()
            );
        }
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
