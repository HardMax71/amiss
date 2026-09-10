use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::rules::BranchRule;
use wary::Validate as _;

#[test]
fn complete_rule_pages_preserve_native_fields_before_strict_decoding() {
    for (input, expected) in [
        (
            include_bytes!("../fixtures/branch-rules-amiss.json").as_slice(),
            4,
        ),
        (
            include_bytes!("../fixtures/branch-rules-docs.json").as_slice(),
            6,
        ),
        (
            include_bytes!("../fixtures/branch-rules-synthetic.json").as_slice(),
            23,
        ),
    ] {
        let native: Vec<BranchRule> = serde_json::from_slice(input).unwrap();
        assert_eq!(native.len(), expected);
        for rule in &native {
            rule.validate(&()).unwrap();
        }
        let encoded = serde_json::to_vec(&native).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(&encoded).unwrap(),
            amiss_fixtures::canonical_json(input).unwrap()
        );
        let (strict, length): (Vec<BranchRule>, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(strict, native);
        assert_eq!(length, input.len());
        assert_eq!(
            amiss_wire::read_json::<Vec<BranchRule>>(&encoded, u64::MAX).unwrap(),
            native
        );
        assert_eq!(
            decode_bounded_json::<Vec<BranchRule>, _>(input, None, input.len() - 1, |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            }),
            Err(ProviderError::InvalidResponse)
        );
        let mut trailing = input.to_vec();
        trailing.extend_from_slice(b" null");
        assert!(amiss_wire::read_json::<Vec<BranchRule>>(&trailing, u64::MAX).is_err());
    }
}

#[test]
fn every_rule_preserves_optional_source_metadata_and_rejects_ambiguous_members() {
    let rules: Vec<BranchRule> =
        serde_json::from_slice(include_bytes!("../fixtures/branch-rules-synthetic.json")).unwrap();
    for rule in rules {
        let input = serde_json::to_string(&rule).unwrap();
        let fields = input.strip_prefix('{').unwrap();
        for source in ["Repository", "Organization"] {
            for id in [0, u64::from(js_int::UInt::MAX)] {
                let full = format!(
                    "{{\"ruleset_id\":{id},\"ruleset_source\":\"owner/repo\",\"ruleset_source_type\":\"{source}\",{fields}"
                );
                let native: BranchRule = serde_json::from_str(&full).unwrap();
                assert_eq!(
                    amiss_fixtures::canonical_json(&serde_json::to_vec(&native).unwrap()).unwrap(),
                    amiss_fixtures::canonical_json(full.as_bytes()).unwrap()
                );
                assert_eq!(
                    amiss_wire::read_json::<BranchRule>(full.as_bytes(), u64::MAX).unwrap(),
                    native
                );
            }
        }
        for extra in [
            r#""ruleset_id":null"#,
            r#""ruleset_id":9007199254740992"#,
            r#""ruleset_source":null"#,
            r#""ruleset_source_type":null"#,
            r#""ruleset_source_type":{"Repository":null}"#,
            r#""ruleset_source_type":"Enterprise""#,
            r#""ruleset_id":1,"\u0072uleset_id":2"#,
            r#""unknown":false"#,
            r#""type":"creation""#,
        ] {
            let changed = format!("{{{extra},{fields}");
            assert!(
                serde_json::from_str::<BranchRule>(&changed).is_err(),
                "{changed}"
            );
            assert!(
                amiss_wire::read_json::<BranchRule>(changed.as_bytes(), u64::MAX).is_err(),
                "{changed}"
            );
        }
    }
}

#[test]
fn optional_rule_parameters_and_members_are_absent_or_typed_not_null() {
    for input in [
        r#"{"type":"update"}"#,
        r#"{"type":"pull_request"}"#,
        r#"{"type":"workflows"}"#,
        r#"{"type":"copilot_code_review","parameters":{}}"#,
        r#"{"type":"commit_message_pattern","parameters":{"operator":"contains","pattern":""}}"#,
        r#"{"type":"workflows","parameters":{"workflows":[{"path":"ci.yml","repository_id":1}]}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"unbound"}],"strict_required_status_checks_policy":false}}"#,
        r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_approving_review_count":0,"required_review_thread_resolution":false,"dismissal_restriction":{"enabled":false}}}"#,
    ] {
        let native: BranchRule = serde_json::from_str(input).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&native).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
        );
        assert_eq!(
            amiss_wire::read_json::<BranchRule>(input.as_bytes(), u64::MAX).unwrap(),
            native
        );
    }
    for input in [
        r#"{"type":"update","parameters":null}"#,
        r#"{"type":"workflows","parameters":null}"#,
        r#"{"type":"required_status_checks"}"#,
        r#"{"type":"required_status_checks","parameters":null}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"unbound","integration_id":null}],"strict_required_status_checks_policy":false}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[],"strict_required_status_checks_policy":false,"do_not_enforce_on_create":null}}"#,
        r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_approving_review_count":0,"required_review_thread_resolution":false,"dismissal_restriction":null}}"#,
        r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_approving_review_count":0,"required_review_thread_resolution":false,"required_reviewers":null}}"#,
        r#"{"type":"pull_request","parameters":{"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_approving_review_count":0,"required_review_thread_resolution":false,"require_extra_approval_for_unattributed_changes":null}}"#,
        r#"{"type":"copilot_code_review","parameters":{"review_on_push":null}}"#,
        r#"{"type":"copilot_code_review","parameters":{"review_draft_pull_requests":null}}"#,
        r#"{"type":"commit_message_pattern","parameters":{"operator":"contains","pattern":"","name":null}}"#,
        r#"{"type":"commit_message_pattern","parameters":{"operator":"contains","pattern":"","negate":null}}"#,
    ] {
        assert!(
            serde_json::from_str::<BranchRule>(input).is_err(),
            "{input}"
        );
        assert!(amiss_wire::read_json::<BranchRule>(input.as_bytes(), u64::MAX).is_err());
    }
    let input = include_str!("../fixtures/branch-rules-synthetic.json");
    for (old, new) in [
        (
            r#""allowed_merge_methods":["merge","squash","rebase"]"#,
            r#""allowed_merge_methods":null"#,
        ),
        (
            r#""allowed_actors":[{"id":1,"type":"User"},{"id":2,"type":"Team"},{"id":3,"type":"IntegrationInstallation"},{"id":4,"type":"RepositoryRole"}]"#,
            r#""allowed_actors":null"#,
        ),
        (
            r#""do_not_enforce_on_create":true"#,
            r#""do_not_enforce_on_create":null"#,
        ),
        (r#""ref":"main""#, r#""ref":null"#),
        (
            r#""sha":"0123456789abcdef0123456789abcdef01234567""#,
            r#""sha":null"#,
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<Vec<BranchRule>>(&changed).is_err(),
            "{new}"
        );
        assert!(amiss_wire::read_json::<Vec<BranchRule>>(changed.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn nested_rule_enums_and_integers_refuse_noncanonical_or_untyped_representations() {
    let input = include_str!("../fixtures/branch-rules-synthetic.json");
    for (old, new) in [
        (
            r#""grouping_strategy":"ALLGREEN""#,
            r#""grouping_strategy":{"ALLGREEN":null}"#,
        ),
        (
            r#""merge_method":"SQUASH""#,
            r#""merge_method":{"SQUASH":null}"#,
        ),
        (
            r#""allowed_merge_methods":["merge","squash","rebase"]"#,
            r#""allowed_merge_methods":[{"merge":null}]"#,
        ),
        (r#""type":"User""#, r#""type":{"User":null}"#),
        (
            r#""reviewer":{"id":2,"type":"Team"}"#,
            r#""reviewer":{"id":2,"type":{"Team":null}}"#,
        ),
        (
            r#""operator":"contains""#,
            r#""operator":{"contains":null}"#,
        ),
        (
            r#""alerts_threshold":"errors_and_warnings""#,
            r#""alerts_threshold":{"errors_and_warnings":null}"#,
        ),
        (
            r#""security_alerts_threshold":"high_or_higher""#,
            r#""security_alerts_threshold":{"high_or_higher":null}"#,
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<Vec<BranchRule>>(&changed).is_err(),
            "{new}"
        );
        assert!(amiss_wire::read_json::<Vec<BranchRule>>(changed.as_bytes(), u64::MAX).is_err());
    }
    for old in [
        r#""id":1"#,
        r#""minimum_approvals":1"#,
        r#""repository_id":1"#,
        r#""integration_id":99"#,
        r#""check_response_timeout_minutes":60"#,
        r#""required_approving_review_count":2"#,
        r#""max_file_path_length":32767"#,
        r#""max_file_size":100"#,
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let (key, _) = old.split_once(':').unwrap();
        for value in ["-1", "1.5", "9007199254740992", "null", "\"1\"", "{}", "[]"] {
            let changed = input.replacen(old, &format!("{key}:{value}"), 1);
            assert!(
                serde_json::from_str::<Vec<BranchRule>>(&changed).is_err(),
                "{old}: {value}"
            );
            assert!(
                amiss_wire::read_json::<Vec<BranchRule>>(changed.as_bytes(), u64::MAX).is_err()
            );
        }
        for value in ["-0", "1e0", "1.0"] {
            let changed = input.replacen(old, &format!("{key}:{value}"), 1);
            assert!(
                amiss_wire::read_json::<Vec<BranchRule>>(changed.as_bytes(), u64::MAX).is_err(),
                "{old}: {value}"
            );
        }
    }
    for (old, new) in [
        (
            r#""reviewer":{"id":2,"type":"Team"}"#,
            r#""reviewer":[2,"Team"]"#,
        ),
        (
            r#""update_allows_fetch_and_merge":true"#,
            r#""update_allows_fetch_and_merge":true,"unknown":false"#,
        ),
        (
            r#""context":"unbound""#,
            r#""context":"unbound","\u0063ontext":"shadow""#,
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let changed = input.replacen(old, new, 1);
        assert_eq!(
            decode_bounded_json::<Vec<BranchRule>, _>(
                changed.as_bytes(),
                None,
                changed.len(),
                |bytes| { amiss_wire::read_json(bytes, u64::MAX) }
            ),
            Err(ProviderError::InvalidResponse),
            "{new}"
        );
    }
}
