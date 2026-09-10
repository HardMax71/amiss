#![cfg(test)]
#![allow(clippy::unwrap_used, reason = "fixed API contracts must fail loudly")]

use amiss_controller::{ProviderError, decode_bounded_json};
use wary::Validate as _;

use super::{BranchRule, RulesetSourceType};

#[test]
fn current_effective_rule_pages_decode_all_observed_settings() {
    for (input, expected) in [
        (
            include_bytes!("../../../tests/fixtures/branch-rules-amiss.json").as_slice(),
            4,
        ),
        (
            include_bytes!("../../../tests/fixtures/branch-rules-docs.json").as_slice(),
            6,
        ),
    ] {
        let rules: Vec<BranchRule> = serde_json::from_slice(input).unwrap();
        assert_eq!(rules.len(), expected);
        for rule in &rules {
            rule.validate(&()).unwrap();
        }
        let encoded = serde_json::to_vec(&rules).unwrap();
        assert_eq!(
            serde_json::from_slice::<Vec<BranchRule>>(&encoded).unwrap(),
            rules
        );
        let (bounded, length): (Vec<BranchRule>, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(bounded, rules);
        assert_eq!(length, input.len());
    }
}

#[test]
fn observed_rule_settings_preserve_presence_and_only_accept_booleans() {
    let input = include_str!("../../../tests/fixtures/branch-rules-docs.json");
    for name in [
        "ignore_approvals_from_contributors",
        "actor_controlled_merging",
    ] {
        let member = format!(",\"{name}\":false");
        assert_eq!(input.matches(&member).count(), 1, "{name}");
        for value in [false, true] {
            let supplied = format!(",\"{name}\":{value}");
            let changed = input.replacen(&member, &supplied, 1);
            let rules: Vec<BranchRule> = serde_json::from_str(&changed).unwrap();
            for rule in &rules {
                rule.validate(&()).unwrap();
            }
            let encoded = serde_json::to_string(&rules).unwrap();
            assert_eq!(encoded.matches(&format!("\"{name}\":{value}")).count(), 1);
            assert_eq!(
                serde_json::from_str::<Vec<BranchRule>>(&encoded).unwrap(),
                rules
            );
        }
        let absent = input.replacen(&member, "", 1);
        let rules: Vec<BranchRule> = serde_json::from_str(&absent).unwrap();
        let encoded = serde_json::to_string(&rules).unwrap();
        assert!(!encoded.contains(&format!("\"{name}\":")));
        assert_eq!(
            serde_json::from_str::<Vec<BranchRule>>(&encoded).unwrap(),
            rules
        );

        for value in ["null", "0", r#""false""#, "[]", "{}"] {
            let changed = input.replacen(&member, &format!(",\"{name}\":{value}"), 1);
            assert!(
                serde_json::from_str::<Vec<BranchRule>>(&changed).is_err(),
                "{name}: {value}"
            );
            assert_eq!(
                decode_bounded_json::<Vec<BranchRule>, _>(
                    changed.as_bytes(),
                    None,
                    changed.len(),
                    |bytes| serde_json::from_slice(bytes)
                ),
                Err(ProviderError::InvalidResponse)
            );
        }
        for replacement in [
            format!("{member},\"{name}\":true"),
            format!("{member},\"{name}_unknown\":true"),
        ] {
            let changed = input.replacen(&member, &replacement, 1);
            assert!(serde_json::from_str::<Vec<BranchRule>>(&changed).is_err());
        }
    }
}

#[test]
fn real_effective_rules_keep_source_and_unattributed_review_policy() -> Result<(), &'static str> {
    let input = br#"[
        {"type":"deletion","ruleset_source_type":"Repository","ruleset_source":"HardMax71/amiss","ruleset_id":18948623},
        {"type":"non_fast_forward","ruleset_source_type":"Repository","ruleset_source":"HardMax71/amiss","ruleset_id":18948623},
        {"type":"pull_request","parameters":{"required_approving_review_count":0,"dismiss_stale_reviews_on_push":false,"required_reviewers":[],"require_code_owner_review":false,"require_last_push_approval":false,"required_review_thread_resolution":false,"require_extra_approval_for_unattributed_changes":true,"allowed_merge_methods":["rebase"]},"ruleset_source_type":"Repository","ruleset_source":"HardMax71/amiss","ruleset_id":18948623},
        {"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"do_not_enforce_on_create":false,"required_status_checks":[{"context":"gates"},{"context":"self-scan"},{"context":"platform-tests (ubuntu-latest)"},{"context":"coverage"},{"context":"json-contract-drift"}]},"ruleset_source_type":"Repository","ruleset_source":"HardMax71/amiss","ruleset_id":18948623}
    ]"#;
    let (rules, count): (Vec<BranchRule>, _) =
        decode_bounded_json(input.as_slice(), None, input.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(count, input.len());
    assert_eq!(rules.len(), 4);
    for rule in &rules {
        rule.validate(&()).unwrap();
    }
    let BranchRule::PullRequest(rule) = &rules[2] else {
        return Err("expected the observed pull-request rule");
    };
    assert_eq!(
        rule.ruleset_source_type,
        Some(RulesetSourceType::Repository)
    );
    assert_eq!(rule.ruleset_source.as_deref(), Some("HardMax71/amiss"));
    assert_eq!(rule.ruleset_id, Some(18_948_623));
    assert_eq!(
        rule.parameters
            .as_ref()
            .unwrap()
            .require_extra_approval_for_unattributed_changes,
        Some(true)
    );
    let BranchRule::RequiredStatusChecks(rule) = &rules[3] else {
        return Err("expected the observed status-check rule");
    };
    assert!(rule.parameters.strict_required_status_checks_policy);
    assert_eq!(rule.parameters.do_not_enforce_on_create, Some(false));
    assert_eq!(rule.parameters.required_status_checks.len(), 5);
    assert!(
        rule.parameters
            .required_status_checks
            .iter()
            .all(|check| check.integration_id.is_none())
    );
    let encoded = serde_json::to_vec(&rules).unwrap();
    let (replayed, _): (Vec<BranchRule>, _) =
        decode_bounded_json(encoded.as_slice(), None, encoded.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(replayed, rules);
    Ok(())
}

#[test]
fn every_published_rule_shape_is_typed() {
    let input = br#"[
        {"type":"creation"},
        {"type":"update","parameters":{"update_allows_fetch_and_merge":true}},
        {"type":"deletion"},
        {"type":"required_linear_history"},
        {"type":"merge_queue","parameters":{"check_response_timeout_minutes":60,"grouping_strategy":"ALLGREEN","max_entries_to_build":5,"max_entries_to_merge":5,"merge_method":"SQUASH","min_entries_to_merge":1,"min_entries_to_merge_wait_minutes":5}},
        {"type":"required_deployments","parameters":{"required_deployment_environments":["production"]}},
        {"type":"required_signatures"},
        {"type":"pull_request","parameters":{"allowed_merge_methods":["merge","squash","rebase"],"dismiss_stale_reviews_on_push":true,"dismissal_restriction":{"allowed_actors":[{"id":1,"type":"User"},{"id":2,"type":"Team"},{"id":3,"type":"IntegrationInstallation"},{"id":4,"type":"RepositoryRole"}],"enabled":true},"require_code_owner_review":true,"require_last_push_approval":true,"required_approving_review_count":2,"required_review_thread_resolution":true,"required_reviewers":[{"file_patterns":["*.rs"],"minimum_approvals":1,"reviewer":{"id":2,"type":"Team"}}]}},
        {"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"amiss/provider","integration_id":99},{"context":"unbound","integration_id":null}],"strict_required_status_checks_policy":true}},
        {"type":"non_fast_forward"},
        {"type":"commit_message_pattern","parameters":{"operator":"starts_with","pattern":"issue"}},
        {"type":"commit_author_email_pattern","parameters":{"operator":"ends_with","pattern":"@example.com"}},
        {"type":"committer_email_pattern","parameters":{"operator":"contains","pattern":"example"}},
        {"type":"branch_name_pattern","parameters":{"operator":"regex","pattern":"^main$","name":"main","negate":false}},
        {"type":"tag_name_pattern","parameters":{"operator":"starts_with","pattern":"v"}},
        {"type":"workflows","parameters":{"do_not_enforce_on_create":true,"workflows":[{"path":".github/workflows/ci.yml","ref":"main","repository_id":1,"sha":"0123456789abcdef0123456789abcdef01234567"}]}},
        {"type":"code_scanning","parameters":{"code_scanning_tools":[{"alerts_threshold":"errors_and_warnings","security_alerts_threshold":"high_or_higher","tool":"CodeQL"}]}},
        {"type":"copilot_code_review","parameters":{"review_draft_pull_requests":false,"review_on_push":true}},
        {"type":"license_compliance_scanning"},
        {"type":"file_path_restriction","parameters":{"restricted_file_paths":["secrets/**"]}},
        {"type":"max_file_path_length","parameters":{"max_file_path_length":32767}},
        {"type":"file_extension_restriction","parameters":{"restricted_file_extensions":["*.exe"]}},
        {"type":"max_file_size","parameters":{"max_file_size":100}}
    ]"#;
    let (rules, _): (Vec<BranchRule>, _) =
        decode_bounded_json(input.as_slice(), None, input.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(rules.len(), 23);
    for rule in &rules {
        rule.validate(&()).unwrap();
    }
    let encoded = serde_json::to_vec(&rules).unwrap();
    let (replayed, _): (Vec<BranchRule>, _) =
        decode_bounded_json(encoded.as_slice(), None, encoded.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(replayed, rules);
}

#[test]
fn required_status_parameters_refuse_malformed_data_at_ingress() {
    for input in [
        r#"{"type":"required_status_checks"}"#,
        r#"{"type":"required_status_checks","parameters":null}"#,
        r#"{"type":"required_status_checks","parameters":{"unexpected":[]}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[]}}"#,
        r#"{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{}],"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"amiss/provider","integration_id":"99"}],"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"amiss/provider","integration_id":-1}],"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[],"strict_required_status_checks_policy":"true"}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[],"strict_required_status_checks_policy":true,"unknown":false}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"amiss/provider","unknown":false}],"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[],"strict_required_status_checks_policy":true,"strict_required_status_checks_policy":false}}"#,
    ] {
        assert_eq!(
            decode_bounded_json::<BranchRule, _>(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            }),
            Err(ProviderError::InvalidResponse),
            "{input}"
        );
    }
}

#[test]
fn rule_tags_and_nested_parameters_are_closed() {
    for input in [
        r#"{"type":"future_rule"}"#,
        r#"{"type":"deletion","unknown":true}"#,
        r#"{"type":"deletion","parameters":{}}"#,
        r#"{"type":"deletion","type":"creation"}"#,
        r#"{"type":"deletion","ruleset_source_type":"Enterprise"}"#,
        r#"{"type":"update","parameters":{"update_allows_fetch_and_merge":true,"unknown":false}}"#,
        r#"{"type":"branch_name_pattern","parameters":{"operator":"future","pattern":"main"}}"#,
        r#"{"type":"workflows","parameters":{"workflows":[{"path":"ci.yml","repository_id":1,"unknown":true}]}}"#,
        r#"{"type":"code_scanning","parameters":{"code_scanning_tools":[{"alerts_threshold":"errors","security_alerts_threshold":"future","tool":"CodeQL"}]}}"#,
        r#"{"type":"max_file_size","parameters":{"max_file_size":1.5}}"#,
        r#"{"type":"max_file_size","parameters":{"max_file_size":-1}}"#,
    ] {
        assert_eq!(
            decode_bounded_json::<BranchRule, _>(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            }),
            Err(ProviderError::InvalidResponse),
            "{input}"
        );
    }
    for input in [
        r#"{"type":"update"}"#,
        r#"{"type":"pull_request"}"#,
        r#"{"type":"workflows","parameters":null}"#,
        r#"{"type":"copilot_code_review","parameters":{}}"#,
        r#"{"type":"deletion","ruleset_source_type":"Organization"}"#,
    ] {
        let (rule, _): (BranchRule, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        rule.validate(&()).unwrap();
    }
}

#[test]
fn derived_rule_validation_checks_parameter_ranges() {
    for (input, valid) in [
        (
            r#"{"type":"merge_queue","parameters":{"check_response_timeout_minutes":0,"grouping_strategy":"HEADGREEN","max_entries_to_build":5,"max_entries_to_merge":5,"merge_method":"MERGE","min_entries_to_merge":1,"min_entries_to_merge_wait_minutes":5}}"#,
            false,
        ),
        (
            r#"{"type":"merge_queue","parameters":{"check_response_timeout_minutes":360,"grouping_strategy":"HEADGREEN","max_entries_to_build":101,"max_entries_to_merge":5,"merge_method":"REBASE","min_entries_to_merge":1,"min_entries_to_merge_wait_minutes":5}}"#,
            false,
        ),
        (
            r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":true,"require_code_owner_review":true,"require_last_push_approval":true,"required_approving_review_count":10,"required_review_thread_resolution":true}}"#,
            true,
        ),
        (
            r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":true,"require_code_owner_review":true,"require_last_push_approval":true,"required_approving_review_count":11,"required_review_thread_resolution":true}}"#,
            false,
        ),
        (
            r#"{"type":"max_file_size","parameters":{"max_file_size":0}}"#,
            false,
        ),
        (
            r#"{"type":"max_file_size","parameters":{"max_file_size":1}}"#,
            true,
        ),
        (
            r#"{"type":"max_file_size","parameters":{"max_file_size":100}}"#,
            true,
        ),
        (
            r#"{"type":"max_file_size","parameters":{"max_file_size":101}}"#,
            false,
        ),
        (
            r#"{"type":"max_file_path_length","parameters":{"max_file_path_length":0}}"#,
            false,
        ),
        (
            r#"{"type":"max_file_path_length","parameters":{"max_file_path_length":1}}"#,
            true,
        ),
        (
            r#"{"type":"max_file_path_length","parameters":{"max_file_path_length":32767}}"#,
            true,
        ),
        (
            r#"{"type":"max_file_path_length","parameters":{"max_file_path_length":32768}}"#,
            false,
        ),
    ] {
        let (rule, _): (BranchRule, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(rule.validate(&()).is_ok(), valid, "{input}");
    }
}
