use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::rules::{
    BranchRule, RequiredStatus, RequiredStatusParameters, RequiredStatusRule,
};

#[test]
fn rule_pages_retain_status_bindings_and_discard_other_settings() {
    for (input, count, checks) in [
        (
            include_bytes!("../fixtures/branch-rules-amiss.json").as_slice(),
            4,
            5,
        ),
        (
            include_bytes!("../fixtures/branch-rules-docs.json").as_slice(),
            6,
            34,
        ),
        (
            include_bytes!("../fixtures/branch-rules-synthetic.json").as_slice(),
            23,
            2,
        ),
    ] {
        let (rules, length): (Vec<BranchRule>, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(rules.len(), count);
        let required: Vec<_> = rules
            .iter()
            .filter_map(|rule| {
                if let BranchRule::RequiredStatusChecks(rule) = rule {
                    Some(&rule.parameters)
                } else {
                    None
                }
            })
            .collect();
        let [parameters]: [_; 1] = required.try_into().unwrap();
        assert_eq!(parameters.required_status_checks.len(), checks);
        assert!(parameters.strict_required_status_checks_policy);
        assert_eq!(
            decode_bounded_json::<Vec<BranchRule>, _>(input, None, input.len() - 1, |bytes| {
                serde_json::from_slice(bytes)
            }),
            Err(ProviderError::InvalidResponse)
        );
        let trailing = format!("{} null", std::str::from_utf8(input).unwrap());
        assert!(serde_json::from_str::<Vec<BranchRule>>(&trailing).is_err());
    }
}

#[test]
fn native_rule_tags_discard_unrelated_payloads_without_hiding_malformed_status_rules() {
    for input in [
        r#"{"type":"future_rule","parameters":false}"#,
        r#"{"type":"deletion","ruleset_source_type":[],"ruleset_id":null}"#,
        r#"{"type":"merge_queue","parameters":{"check_response_timeout_minutes":-1}}"#,
        r#"{"type":"pull_request","parameters":null}"#,
        r#"{"type":"workflows","parameters":{"workflows":[{"unknown":true}]}}"#,
    ] {
        assert_eq!(
            serde_json::from_str::<BranchRule>(input).unwrap(),
            BranchRule::Other
        );
    }
    for input in [
        "{}",
        r#"{"type":null}"#,
        r#"{"type":false}"#,
        r#"{"type":1}"#,
        r#"{"type":"deletion","type":"required_status_checks"}"#,
        r#"{"type":"required_status_checks","\u0074ype":"future_rule"}"#,
        r#"{"type":"required_status_checks"}"#,
        r#"{"type":"required_status_checks","parameters":null}"#,
        r#"{"type":"required_status_checks","parameters":{}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[]}}"#,
        r#"{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true}}"#,
        r#"{"type":"required_status_checks","parameters":{"required_status_checks":[{}],"strict_required_status_checks_policy":true}}"#,
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
fn status_binding_fields_are_required_typed_unique_and_lossless() {
    let rule = BranchRule::RequiredStatusChecks(RequiredStatusRule {
        parameters: RequiredStatusParameters {
            strict_required_status_checks_policy: true,
            required_status_checks: vec![RequiredStatus {
                context: "amiss/provider".to_owned(),
                integration_id: Some(js_int::UInt::from(99_u8)),
            }],
        },
    });
    let input = serde_json::to_string(&rule).unwrap();
    let metadata = input
        .replacen(
            '{',
            r#"{"ruleset_id":null,"ruleset_source":false,"ruleset_source_type":{},"#,
            1,
        )
        .replace(
            r#""parameters":{"#,
            r#""parameters":{"do_not_enforce_on_create":null,"extra":[],"#,
        )
        .replace(r#""context":"#, r#""future":{},"context":"#);
    assert_eq!(serde_json::from_str::<BranchRule>(&metadata).unwrap(), rule);
    amiss_fixtures::assert_json_rejections::<BranchRule>(
        &input,
        &[
            (r#""parameters":"#, r#""parameters":{},"parameters":"#),
            (
                r#""required_status_checks":"#,
                r#""required_status_checks":[],"required_status_checks":"#,
            ),
            (r#""context":"amiss/provider""#, r#""context":null"#),
            (r#""context":"amiss/provider""#, r#""context":false"#),
            (
                r#""context":"amiss/provider""#,
                r#""context":"amiss/provider","\u0063ontext":"shadow""#,
            ),
            (
                r#""strict_required_status_checks_policy":true"#,
                r#""strict_required_status_checks_policy":null"#,
            ),
            (
                r#""strict_required_status_checks_policy":true"#,
                r#""strict_required_status_checks_policy":"true""#,
            ),
            (
                r#""strict_required_status_checks_policy":true"#,
                r#""strict_required_status_checks_policy":true,"strict_required_status_checks_policy":false"#,
            ),
            (
                r#""integration_id":99"#,
                r#""integration_id":99,"integration_id":100"#,
            ),
        ],
    );
    for invalid in [
        "null",
        "true",
        "-1",
        "1.5",
        "9007199254740992",
        r#""99""#,
        "[]",
        "{}",
    ] {
        amiss_fixtures::assert_json_rejections::<BranchRule>(
            &input,
            &[(
                r#""integration_id":99"#,
                &format!(r#""integration_id":{invalid}"#),
            )],
        );
    }
    for integration_id in [None, Some(js_int::UInt::MIN), Some(js_int::UInt::MAX)] {
        let check = RequiredStatus {
            context: "amiss/provider".to_owned(),
            integration_id,
        };
        let encoded = serde_json::to_vec(&check).unwrap();
        assert_eq!(
            serde_json::from_slice::<RequiredStatus>(&encoded).unwrap(),
            check
        );
    }
}
