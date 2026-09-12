pub(crate) const POLICY: &[u8] = include_bytes!("../fixtures/scanner-policy.json");
pub(crate) const FLOOR: &[u8] = include_bytes!("../fixtures/organization-floor.json");
pub(crate) const DEBT: &[u8] = include_bytes!("../../../../spec/examples/debt-snapshot.json");
pub(crate) const WAIVER: &[u8] = include_bytes!("../../../../spec/examples/waiver-bundle.json");
pub(crate) const RAW_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
pub(crate) const PROJECTION_DIGEST: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

pub(crate) const TIME_STATEMENT: &str = r#"{
  "schema": "amiss/scanner-trusted-time-statement",
  "controller": "external-required-check-clock",
  "repository": { "host": "gitlab.com", "owner": "platform/security", "name": "docs" },
  "ref": "refs/heads/main",
  "candidate_identity_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  "provider": "gitlab-ci",
  "provider_run_id": "pipeline/01J2Z9-7",
  "provider_run_attempt": 2,
  "evaluation_instant": "2026-07-12T10:00:00Z",
  "valid_until": "2026-07-12T10:10:00Z"
}"#;
