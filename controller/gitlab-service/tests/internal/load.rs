#![cfg(test)]

use amiss_controller_service::EvaluationConfig;

use super::{HINT_BODY_BYTES, POLICY_JOB_HEADER_BYTES, POLICY_JOB_HEADERS, policy_job_endpoint};

/// The policy-job endpoint clamps what the configuration asked for, and a
/// smaller configured value still wins.
#[test]
fn the_policy_job_endpoint_tightens_the_configured_ceilings() {
    let generous = EvaluationConfig {
        path: "/amiss/policy".to_owned(),
        max_body_bytes: HINT_BODY_BYTES.saturating_mul(8),
        max_headers: POLICY_JOB_HEADERS.saturating_mul(8),
        max_header_bytes: POLICY_JOB_HEADER_BYTES.saturating_mul(8),
        max_concurrent_evaluations: 4,
    };
    let clamped = policy_job_endpoint(generous);
    assert_eq!(clamped.max_body_bytes, 1_024);
    assert_eq!(clamped.max_headers, 32);
    assert_eq!(clamped.max_header_bytes, 32_768);
    assert_eq!(clamped.max_concurrent_evaluations, 4);

    let modest = EvaluationConfig {
        path: "/amiss/policy".to_owned(),
        max_body_bytes: 64,
        max_headers: 2,
        max_header_bytes: 512,
        max_concurrent_evaluations: 1,
    };
    let kept = policy_job_endpoint(modest);
    assert_eq!(kept.max_body_bytes, 64);
    assert_eq!(kept.max_headers, 2);
    assert_eq!(kept.max_header_bytes, 512);
}
