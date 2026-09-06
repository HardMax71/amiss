use amiss_wire::controls::{FACT_DOMAIN, FINDING_KEY_DOMAIN};

use amiss_wire::json;

pub(crate) const POLICY: &[u8] = include_bytes!("../fixtures/scanner-policy.json");

pub(crate) const FLOOR: &[u8] = include_bytes!("../fixtures/organization-floor.json");

pub(crate) const DEFAULT_CONSTRUCT: &str = "markdown-inline-link";

pub(crate) const RAW_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

pub(crate) const PROJECTION_DIGEST: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

pub(crate) fn key_input_json(finding_kind: &str) -> String {
    format!(
        r#"{{
  "schema": "amiss/scanner-finding-key-input",
  "finding_kind": "{finding_kind}",
  "scope": {{
    "kind": "reference",
    "document": "README.md",
    "source_construct": "markdown-inline-link",
    "normalized_target_intent": {{
      "kind": "repository-path",
      "path": "docs/example.md",
      "target_kind": "either",
      "query_digest": null,
      "fragment_digest": null
    }},
    "occurrence": {{
      "kind": "source-projection",
      "source_projection_digest": "sha256:7777777777777777777777777777777777777777777777777777777777777777"
    }}
  }}
}}"#
    )
}

pub(crate) fn fact_json_for(finding_kind: &str, key_input: &str, resolution: &str) -> String {
    format!(
        r#"{{
  "schema": "amiss/scanner-fact",
  "finding_kind": "{finding_kind}",
  "key_input": {key_input},
  "evidence": {{
    "kind": "reference",
    "resolution": {resolution},
    "occurrence_multiplicity": 1
  }}
}}"#
    )
}

pub(crate) fn fact_json() -> String {
    fact_json_for(
        "explicit-target-missing",
        &key_input_json("explicit-target-missing"),
        r#"{
          "kind": "missing",
          "reason": "path-not-found",
          "path": "docs/example.md",
          "near": null
        }"#,
    )
}

#[expect(clippy::unwrap_used, reason = "test helper on known-valid templates")]
pub(crate) fn computed_digests() -> (String, String) {
    let key_input = key_input_json("explicit-target-missing");
    let key = amiss_wire::digest::hb(
        FINDING_KEY_DOMAIN,
        &serde_json_canonicalizer::to_vec(&json::parse(key_input.as_bytes()).unwrap()).unwrap(),
    )
    .to_string();
    let fact = amiss_wire::digest::hb(
        FACT_DOMAIN,
        &serde_json_canonicalizer::to_vec(&json::parse(fact_json().as_bytes()).unwrap()).unwrap(),
    )
    .to_string();
    (key, fact)
}

pub(crate) fn debt_item_json(
    debt_id: &str,
    finding_key: &str,
    fact: &str,
    fact_digest: &str,
    validity: (&str, &str),
) -> String {
    let (created, expires) = validity;
    format!(
        r#"{{
  "debt_id": "{debt_id}",
  "finding_key": "{finding_key}",
  "accepted_fact": {fact},
  "accepted_fact_digest": "{fact_digest}",
  "owner": "team:docs-platform",
  "reason": "Legacy link scheduled for removal.",
  "created_at": "{created}",
  "expires_at": "{expires}"
}}"#
    )
}

pub(crate) fn debt_item(
    debt_id: &str,
    finding_key: &str,
    fact_digest: &str,
    created: &str,
    expires: &str,
) -> String {
    debt_item_json(
        debt_id,
        finding_key,
        &fact_json(),
        fact_digest,
        (created, expires),
    )
}

pub(crate) fn debt_snapshot(created_at: &str, items: &[String]) -> String {
    format!(
        r#"{{
  "schema": "amiss/debt-snapshot",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "spec-to-rest" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "sha256:464a7c6d84ab06c1fd0766b983b8027af18ada5dcefd1ba3252c0cc459430a48",
  "adoption_tree": {{ "object_format": "sha1", "tree_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }},
  "adoption_report_payload_digest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "created_at": "{created_at}",
  "items": [{items}]
}}"#,
        items = items.join(",")
    )
}

pub(crate) fn waiver_item(
    waiver_id: &str,
    finding_key: &str,
    fact_digest: &str,
    issuer: &str,
) -> String {
    format!(
        r#"{{
  "waiver_id": "{waiver_id}",
  "finding_key": "{finding_key}",
  "authorized_fact": {fact},
  "authorized_fact_digest": "{fact_digest}",
  "candidate_tree": {{ "object_format": "sha1", "tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }},
  "owner": "team:docs-platform",
  "issuer": "{issuer}",
  "reason": "Release window exception.",
  "created_at": "2026-07-01T00:00:00Z",
  "not_before": "2026-07-02T00:00:00Z",
  "expires_at": "2026-08-01T00:00:00Z",
  "residual_disposition": "warn"
}}"#,
        fact = fact_json()
    )
}

pub(crate) fn waiver_bundle(items: &[String]) -> String {
    format!(
        r#"{{
  "schema": "amiss/waiver-bundle",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "spec-to-rest" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "sha256:464a7c6d84ab06c1fd0766b983b8027af18ada5dcefd1ba3252c0cc459430a48",
  "created_at": "2026-07-03T00:00:00Z",
  "items": [{items}]
}}"#,
        items = items.join(",")
    )
}

#[expect(clippy::unwrap_used, reason = "test helper on nonempty digest strings")]
pub(crate) fn flip_last(digest: &str) -> String {
    let mut chars: Vec<char> = digest.chars().collect();
    let last = chars.last_mut().unwrap();
    *last = if *last == '0' { '1' } else { '0' };
    chars.into_iter().collect()
}

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
