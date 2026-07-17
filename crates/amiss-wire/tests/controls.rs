use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, FACT_DOMAIN, FINDING_KEY_DOMAIN, Fact,
    FloorDefect, OrganizationFloor, ResourceName, STATEMENT_TTL_MAX_SECONDS, ScannerPolicy,
    TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::digest::hj;
use amiss_wire::json;
use amiss_wire::model::{BranchRef, UtcInstant};
use amiss_wire::resolution::{BlobContent, BlobMode, Missing, Resolution, Target};

const POLICY: &[u8] = include_bytes!("fixtures/scanner-policy.json");
const FLOOR: &[u8] = include_bytes!("fixtures/organization-floor.json");

const RAW_DIGEST: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const PROJECTION_DIGEST: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn key_input_json(finding_kind: &str) -> String {
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

fn fact_json_for(finding_kind: &str, key_input: &str, resolution: &str) -> String {
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

fn fact_json() -> String {
    fact_json_for(
        "explicit-target-missing",
        &key_input_json("explicit-target-missing"),
        r#"{
          "kind": "missing",
          "reason": "path-not-found",
          "path": "docs/example.md"
        }"#,
    )
}

#[expect(clippy::unwrap_used, reason = "test helper on known-valid templates")]
fn computed_digests() -> (String, String) {
    let key_input = key_input_json("explicit-target-missing");
    let key = hj(
        FINDING_KEY_DOMAIN,
        &json::parse(key_input.as_bytes()).unwrap(),
    )
    .to_string();
    let fact = hj(FACT_DOMAIN, &json::parse(fact_json().as_bytes()).unwrap()).to_string();
    (key, fact)
}

fn debt_item_json(
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

fn debt_item(
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

fn debt_snapshot(created_at: &str, items: &[String]) -> String {
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

#[expect(
    clippy::unwrap_used,
    reason = "test helper on syntactically valid JSON templates"
)]
fn parse_debt_fact_case(
    fact_finding_kind: &str,
    key_finding_kind: &str,
    resolution: &str,
) -> Result<DebtSnapshot, Error> {
    let key_input = key_input_json(key_finding_kind);
    let fact = fact_json_for(fact_finding_kind, &key_input, resolution);
    let finding_key = hj(
        FINDING_KEY_DOMAIN,
        &json::parse(key_input.as_bytes()).unwrap(),
    )
    .to_string();
    let fact_digest = hj(FACT_DOMAIN, &json::parse(fact.as_bytes()).unwrap()).to_string();
    let item = debt_item_json(
        "debt/resolution-case",
        &finding_key,
        &fact,
        &fact_digest,
        ("2026-07-01T00:00:00Z", "2026-08-01T00:00:00Z"),
    );
    let document = debt_snapshot("2026-07-02T00:00:00Z", &[item]);
    DebtSnapshot::parse(document.as_bytes())
}

fn waiver_item(waiver_id: &str, finding_key: &str, fact_digest: &str, issuer: &str) -> String {
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

fn waiver_bundle(items: &[String]) -> String {
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
fn flip_last(digest: &str) -> String {
    let mut chars: Vec<char> = digest.chars().collect();
    let last = chars.last_mut().unwrap();
    *last = if *last == '0' { '1' } else { '0' };
    chars.into_iter().collect()
}

#[test]
fn parses_the_policy_fixture() {
    let policy = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(policy.document_includes.len(), 2);
    assert_eq!(policy.protected_inventory.len(), 2);
    assert_eq!(policy.finding_dispositions.len(), 1);
    assert_eq!(policy.digest, ScannerPolicy::parse(POLICY).unwrap().digest);
}

#[test]
fn parses_the_floor_fixture() {
    let floor = OrganizationFloor::parse(FLOOR).unwrap();
    assert_eq!(floor.schema(), "amiss/organization-floor");
    assert_eq!(
        floor.digest,
        hj("amiss/organization-floor", &json::parse(FLOOR).unwrap())
    );
    assert_eq!(floor.floor_id.as_str(), "platform/scanner-floor-2026-07");
    assert_eq!(floor.ref_name.as_str(), "refs/heads/main");
    assert_eq!(floor.resource_limits.len(), 2);
    assert_ne!(floor.digest, ScannerPolicy::parse(POLICY).unwrap().digest);
}

#[test]
fn parses_a_floor_declaring_every_resource() {
    let mut declared: Vec<(&'static str, i64)> = ResourceName::all()
        .map(|resource| {
            let maximum = if resource == ResourceName::MachineJsonBytes {
                67_108_864
            } else if resource == ResourceName::TypedAnalysisErrorsRetained {
                64
            } else {
                100_000
            };
            (resource.as_str(), maximum)
        })
        .collect();
    declared.sort_unstable();
    let rows: Vec<String> = declared
        .iter()
        .map(|(resource, maximum)| {
            format!("{{ \"resource\": \"{resource}\", \"maximum\": {maximum} }}")
        })
        .collect();
    let doc = format!(
        r#"{{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/every-resource",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": [{rows}]
}}"#,
        rows = rows.join(",")
    );
    let floor = OrganizationFloor::parse(doc.as_bytes()).unwrap();
    assert_eq!(floor.resource_limits.len(), ResourceName::all().len());
}

#[test]
fn rejects_policy_shape_defects() {
    let unknown = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": [],
      "extra": 1
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unknown).unwrap_err().kind,
        ErrorKind::UnknownField
    );

    let wrong_schema = br#"{
      "schema": "assure/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(wrong_schema).unwrap_err().kind,
        ErrorKind::InvalidValue
    );

    let unsorted = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": ["b.md", "a.md"],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unsorted).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    for bad_path in ["/abs.md", "a//b.md", "a/../b.md", "a\\\\b.md", "a/./b.md"] {
        let doc = format!(
            r#"{{
              "schema": "amiss/scanner-policy",
              "document_includes": [],
              "protected_inventory": ["{bad_path}"],
              "finding_dispositions": []
            }}"#
        );
        assert_eq!(
            ScannerPolicy::parse(doc.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "path {bad_path}"
        );
    }
}

#[expect(clippy::panic, reason = "test helper narrowing the defect family")]
fn floor_schema_kind(defect: FloorDefect) -> ErrorKind {
    match defect {
        FloorDefect::Schema(error) => error.kind,
        FloorDefect::Entries { .. } => panic!("expected a schema defect, got an entries crossing"),
    }
}

#[test]
fn rejects_floor_bound_defects() {
    let doc = String::from_utf8(FLOOR.to_vec()).unwrap();
    let wrong_ceiling = doc.replace("67108864", "67108863");
    assert_eq!(
        floor_schema_kind(OrganizationFloor::parse(wrong_ceiling.as_bytes()).unwrap_err()),
        ErrorKind::InvalidValue
    );

    let wrong_errors = doc.replace("\"maximum\": 64", "\"maximum\": 65");
    assert_eq!(
        floor_schema_kind(OrganizationFloor::parse(wrong_errors.as_bytes()).unwrap_err()),
        ErrorKind::InvalidValue
    );

    let unsorted_limits = doc.replace(
        "{ \"resource\": \"machine-json-bytes\", \"maximum\": 67108864 },\n    { \"resource\": \"typed-analysis-errors-retained\", \"maximum\": 64 }",
        "{ \"resource\": \"typed-analysis-errors-retained\", \"maximum\": 64 },\n    { \"resource\": \"machine-json-bytes\", \"maximum\": 67108864 }",
    );
    assert_eq!(
        floor_schema_kind(OrganizationFloor::parse(unsorted_limits.as_bytes()).unwrap_err()),
        ErrorKind::UnsortedSet
    );
}

#[test]
fn rejects_floors_over_the_combined_entry_limit() {
    let paths = |count: usize, prefix: &str| {
        let items: Vec<String> = (0..count)
            .map(|index| format!("\"{prefix}/{index:07}.md\""))
            .collect();
        items.join(",")
    };
    let doc = format!(
        r#"{{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/too-big",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [{inventory}],
  "protected_control_paths": [{controls}],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}}"#,
        inventory = paths(60_000, "docs/a"),
        controls = paths(45_000, "ops/b"),
    );
    assert_eq!(
        OrganizationFloor::parse(doc.as_bytes()).unwrap_err(),
        FloorDefect::Entries {
            configured_limit: 100_000,
            observed_lower_bound: 100_001,
        }
    );
}

#[test]
fn rejects_floors_inconsistent_with_their_own_declared_entry_limit() {
    let doc = br#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/self-inconsistent",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": ["docs/a.md", "docs/b.md", "docs/c.md"],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": [
    { "resource": "organization-policy-entries", "maximum": 3 }
  ]
}"#;
    assert_eq!(
        OrganizationFloor::parse(doc).unwrap_err(),
        FloorDefect::Entries {
            configured_limit: 3,
            observed_lower_bound: 4,
        }
    );
}

#[test]
fn branch_refs_follow_ref_format() {
    let valid = [
        "refs/heads/main",
        "refs/heads/feature/a+b",
        "refs/heads/\u{e9}",
        "refs/heads/@",
        "refs/heads/-dash",
    ];
    for case in valid {
        assert!(BranchRef::new(case.to_owned()).is_some(), "{case}");
    }
    let invalid = [
        "refs/heads/".to_owned(),
        "refs/heads//main".to_owned(),
        "refs/heads/.hidden".to_owned(),
        "refs/heads/main.lock".to_owned(),
        "refs/heads/a..b".to_owned(),
        "refs/heads/a b".to_owned(),
        "refs/heads/a~b".to_owned(),
        "refs/heads/a?b".to_owned(),
        "refs/heads/a[b".to_owned(),
        "refs/heads/a\\b".to_owned(),
        "refs/heads/a@{b".to_owned(),
        "refs/heads/a.".to_owned(),
        format!("refs/heads/{}", "a".repeat(256)),
    ];
    for case in invalid {
        assert!(BranchRef::new(case.clone()).is_none(), "{case}");
    }
}

#[test]
fn instants_are_strictly_gregorian() {
    for valid in [
        "2026-02-28T23:59:59Z",
        "2024-02-29T00:00:00Z",
        "2000-02-29T12:00:00Z",
        "0001-01-01T00:00:00Z",
    ] {
        assert!(UtcInstant::new(valid.to_owned()).is_some(), "{valid}");
    }
    for invalid in [
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-00-10T00:00:00Z",
        "2026-07-00T00:00:00Z",
        "2026-07-12T24:00:00Z",
        "2026-07-12T00:00:60Z",
        "2026-07-12T00:00:00",
        "2026-7-12T00:00:00Z",
    ] {
        assert!(UtcInstant::new(invalid.to_owned()).is_none(), "{invalid}");
    }
}

#[test]
fn structural_resolution_facts_accept_both_missing_reasons() {
    let path_missing = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{
          "kind": "missing",
          "reason": "path-not-found",
          "path": "docs/missing.md"
        }"#,
    )
    .unwrap();
    assert!(matches!(
        path_missing.items[0].accepted_fact.resolution(),
        Resolution::Missing(Missing::PathNotFound { path })
            if path.as_str() == "docs/missing.md"
    ));

    let line_missing = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{
          "kind": "missing",
          "reason": "line-fragment-out-of-range",
          "path": "src/lib.rs"
        }"#,
    )
    .unwrap();
    assert!(matches!(
        line_missing.items[0].accepted_fact.resolution(),
        Resolution::Missing(Missing::LineFragmentOutOfRange { path })
            if path.as_str() == "src/lib.rs"
    ));
}

#[test]
fn structural_resolution_facts_accept_typed_mismatch_targets() {
    let tree_mismatch = parse_debt_fact_case(
        "explicit-target-type-mismatch",
        "explicit-target-type-mismatch",
        r#"{
          "kind": "type-mismatch",
          "target": {
            "kind": "tree",
            "path": "docs"
          }
        }"#,
    )
    .unwrap();
    assert!(matches!(
        tree_mismatch.items[0].accepted_fact.resolution(),
        Resolution::TypeMismatch(Target::Tree { path }) if path.as_str() == "docs"
    ));

    let available_blob = parse_debt_fact_case(
        "explicit-target-type-mismatch",
        "explicit-target-type-mismatch",
        &format!(
            r#"{{
              "kind": "type-mismatch",
              "target": {{
                "kind": "blob",
                "path": "docs/guide.md",
                "mode": "100644",
                "content": {{
                  "kind": "available",
                  "raw_digest": "{RAW_DIGEST}",
                  "projection_digest": "{PROJECTION_DIGEST}"
                }}
              }}
            }}"#
        ),
    )
    .unwrap();
    assert!(matches!(
        available_blob.items[0].accepted_fact.resolution(),
        Resolution::TypeMismatch(Target::Blob(blob))
            if blob.path.as_str() == "docs/guide.md"
                && blob.mode == BlobMode::Regular
                && matches!(blob.content, BlobContent::Available { raw_digest, projection_digest }
                    if raw_digest.to_string() == RAW_DIGEST
                        && projection_digest.to_string() == PROJECTION_DIGEST)
    ));

    let lfs_blob = parse_debt_fact_case(
        "explicit-target-type-mismatch",
        "explicit-target-type-mismatch",
        &format!(
            r#"{{
              "kind": "type-mismatch",
              "target": {{
                "kind": "blob",
                "path": "assets/model.bin",
                "mode": "100755",
                "content": {{
                  "kind": "lfs-pointer",
                  "raw_digest": "{RAW_DIGEST}"
                }}
              }}
            }}"#
        ),
    )
    .unwrap();
    assert!(matches!(
        lfs_blob.items[0].accepted_fact.resolution(),
        Resolution::TypeMismatch(Target::Blob(blob))
            if blob.path.as_str() == "assets/model.bin"
                && blob.mode == BlobMode::Executable
                && matches!(blob.content, BlobContent::LfsPointer { raw_digest }
                    if raw_digest.to_string() == RAW_DIGEST)
    ));
}

#[test]
fn structural_resolution_facts_reject_nonstructural_kinds() {
    let cases = [
        (
            "resolved",
            r#"{"kind":"resolved","target":{"kind":"tree","path":"docs"}}"#,
        ),
        (
            "unsupported-target",
            r#"{"kind":"unsupported-target","reason":"symlink","path":"docs/link.md"}"#,
        ),
        (
            "unsupported-semantics",
            r#"{"kind":"unsupported-semantics","reason":"site-route"}"#,
        ),
        (
            "unsupported-version",
            r#"{"kind":"unsupported-version","scope":{"kind":"unknown-path"}}"#,
        ),
        ("invalid", r#"{"kind":"invalid","reason":"syntax"}"#),
        ("external", r#"{"kind":"external","reason":"url"}"#),
    ];

    for (kind, resolution) in cases {
        let error = parse_debt_fact_case(
            "explicit-target-missing",
            "explicit-target-missing",
            resolution,
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidValue, "{kind}");
        assert!(error.path.ends_with(".resolution.kind"), "{kind}");
    }
}

#[test]
fn structural_resolution_facts_reject_bad_missing_reasons_and_legacy_bags() {
    let cases = [
        (
            "wrong-family reason",
            r#"{"kind":"missing","reason":"symlink","path":"docs/missing.md"}"#,
            ErrorKind::InvalidValue,
        ),
        (
            "missing reason",
            r#"{"kind":"missing","path":"docs/missing.md"}"#,
            ErrorKind::MissingField,
        ),
        (
            "legacy nullable bag",
            r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md","status":"missing","code":"path-not-found","entry_kind":null,"git_mode":null,"raw_digest":null,"projection_digest":null,"content_availability":"not-applicable"}"#,
            ErrorKind::UnknownField,
        ),
    ];

    for (case, resolution, expected) in cases {
        let error = parse_debt_fact_case(
            "explicit-target-missing",
            "explicit-target-missing",
            resolution,
        )
        .unwrap_err();
        assert_eq!(error.kind, expected, "{case}");
    }
}

#[test]
fn structural_resolution_facts_reject_invalid_target_and_content_shapes() {
    let cases = [
        (
            "non-object target",
            r#"{"kind":"type-mismatch","target":"docs"}"#,
            ErrorKind::WrongType,
        ),
        (
            "unknown target kind",
            r#"{"kind":"type-mismatch","target":{"kind":"symlink","path":"docs/link"}}"#,
            ErrorKind::InvalidValue,
        ),
        (
            "tree carrying blob content",
            r#"{"kind":"type-mismatch","target":{"kind":"tree","path":"docs","mode":"100644"}}"#,
            ErrorKind::UnknownField,
        ),
        (
            "special-entry blob mode",
            r#"{"kind":"type-mismatch","target":{"kind":"blob","path":"docs/link.md","mode":"120000","content":{"kind":"lfs-pointer","raw_digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"}}}"#,
            ErrorKind::InvalidValue,
        ),
        (
            "missing blob content",
            r#"{"kind":"type-mismatch","target":{"kind":"blob","path":"docs/guide.md","mode":"100644"}}"#,
            ErrorKind::MissingField,
        ),
        (
            "available content without projection digest",
            r#"{"kind":"type-mismatch","target":{"kind":"blob","path":"docs/guide.md","mode":"100644","content":{"kind":"available","raw_digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"}}}"#,
            ErrorKind::MissingField,
        ),
        (
            "LFS content with projection digest",
            r#"{"kind":"type-mismatch","target":{"kind":"blob","path":"assets/model.bin","mode":"100644","content":{"kind":"lfs-pointer","raw_digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111","projection_digest":"sha256:2222222222222222222222222222222222222222222222222222222222222222"}}}"#,
            ErrorKind::UnknownField,
        ),
        (
            "unknown content kind",
            r#"{"kind":"type-mismatch","target":{"kind":"blob","path":"docs/guide.md","mode":"100644","content":{"kind":"inline","raw_digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"}}}"#,
            ErrorKind::InvalidValue,
        ),
    ];

    for (case, resolution, expected) in cases {
        let error = parse_debt_fact_case(
            "explicit-target-type-mismatch",
            "explicit-target-type-mismatch",
            resolution,
        )
        .unwrap_err();
        assert_eq!(error.kind, expected, "{case}");
    }
}

#[test]
fn structural_resolution_facts_reject_finding_kind_mismatches() {
    let missing = r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md"}"#;
    let cases = [
        (
            "resolution versus fact",
            "explicit-target-type-mismatch",
            "explicit-target-type-mismatch",
            missing,
        ),
        (
            "fact versus embedded key",
            "explicit-target-missing",
            "explicit-target-type-mismatch",
            missing,
        ),
    ];

    for (case, fact_kind, key_kind, resolution) in cases {
        let error = parse_debt_fact_case(fact_kind, key_kind, resolution).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Inconsistent, "{case}");
    }
}

#[test]
fn structural_fact_constructor_rejects_invalid_programmatic_states() {
    let parsed = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md"}"#,
    )
    .unwrap();
    let accepted = &parsed.items[0].accepted_fact;
    let key_input = accepted.key_input().clone();
    let path = key_input.scope.normalized_target_intent.path.clone();

    assert!(Fact::new(key_input.clone(), accepted.resolution().clone()).is_some());
    assert!(
        Fact::new(
            key_input.clone(),
            Resolution::TypeMismatch(Target::Tree { path: path.clone() }),
        )
        .is_none()
    );
    assert!(Fact::new(key_input, Resolution::Resolved(Target::Tree { path })).is_none());
}

#[test]
fn parses_a_valid_debt_snapshot() {
    let (key, fact) = computed_digests();
    let item = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[item]);
    let snapshot = DebtSnapshot::parse(doc.as_bytes()).unwrap();
    assert_eq!(snapshot.schema(), "amiss/debt-snapshot");
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items.first().unwrap().finding_key.to_string(), key);
}

#[test]
fn rejects_debt_digest_and_order_defects() {
    let (key, fact) = computed_digests();

    let bad_key = debt_item(
        "debt/readme",
        &flip_last(&key),
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[bad_key]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    let bad_fact = debt_item(
        "debt/readme",
        &key,
        &flip_last(&fact),
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[bad_fact]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    let first = debt_item(
        "debt/b",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let second = debt_item(
        "debt/a",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[first, second]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    let first = debt_item(
        "debt/a",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let second = debt_item(
        "debt/b",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[first, second]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember
    );

    let late = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-03T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[late]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let inverted = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-08-01T00:00:00Z",
        "2026-07-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-08-02T00:00:00Z", &[inverted]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );
}

#[test]
fn parses_a_valid_waiver_bundle_and_rejects_duplicates() {
    let (key, fact) = computed_digests();

    let item = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let doc = waiver_bundle(&[item]);
    let bundle = WaiverBundle::parse(doc.as_bytes()).unwrap();
    assert_eq!(bundle.schema(), "amiss/waiver-bundle");
    assert_eq!(bundle.items.len(), 1);

    let same_owner = waiver_item("waiver/one", &key, &fact, "team:docs-platform");
    let doc = waiver_bundle(&[same_owner]);
    assert!(
        WaiverBundle::parse(doc.as_bytes()).is_ok(),
        "owner==issuer is a selected-item defect, not a parse defect"
    );

    let first = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let second = waiver_item("waiver/two", &key, &fact, "team:release-engineering");
    let doc = waiver_bundle(&[first, second]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "duplicate (candidate_tree, finding_key) pair"
    );

    let bad_window = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"not_before\": \"2026-07-02T00:00:00Z\"",
        "\"not_before\": \"2026-09-01T00:00:00Z\"",
    );
    let doc = waiver_bundle(&[bad_window]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let bad_residual = waiver_item("waiver/one", &key, &fact, "team:release-engineering").replace(
        "\"residual_disposition\": \"warn\"",
        "\"residual_disposition\": \"record\"",
    );
    let doc = waiver_bundle(&[bad_residual]);
    assert_eq!(
        WaiverBundle::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::InvalidValue
    );
}

const TIME_STATEMENT: &str = r#"{
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

#[test]
fn parses_a_trusted_time_statement_and_enforces_the_ttl() {
    assert_eq!(STATEMENT_TTL_MAX_SECONDS, 600);
    let statement = TrustedTimeStatement::parse(TIME_STATEMENT.as_bytes()).unwrap();
    assert_eq!(statement.schema(), "amiss/scanner-trusted-time-statement");
    assert_eq!(statement.controller(), "external-required-check-clock");
    assert_eq!(statement.provider, "gitlab-ci");
    assert_eq!(
        statement.digest,
        hj(
            "amiss/scanner-trusted-time-statement",
            &json::parse(TIME_STATEMENT.as_bytes()).unwrap()
        )
    );
    assert_eq!(statement.provider_run_id, "pipeline/01J2Z9-7");
    assert_eq!(statement.provider_run_attempt, 2);
    assert_eq!(
        statement.evaluation_instant.as_str(),
        "2026-07-12T10:00:00Z"
    );
    assert_eq!(
        statement.valid_until.epoch_seconds() - statement.evaluation_instant.epoch_seconds(),
        600
    );

    let too_long = TIME_STATEMENT.replace("10:10:00Z", "10:10:01Z");
    assert_eq!(
        TrustedTimeStatement::parse(too_long.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let not_after = TIME_STATEMENT.replace("10:10:00Z", "10:00:00Z");
    assert_eq!(
        TrustedTimeStatement::parse(not_after.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let trailing_separator = TIME_STATEMENT.replace("pipeline/01J2Z9-7", "pipeline/");
    assert_eq!(
        TrustedTimeStatement::parse(trailing_separator.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let uppercase_provider = TIME_STATEMENT.replace("gitlab-ci", "GitLab-CI");
    assert_eq!(
        TrustedTimeStatement::parse(uppercase_provider.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let numeric_run = TIME_STATEMENT.replace("pipeline/01J2Z9-7", "987654321");
    assert!(TrustedTimeStatement::parse(numeric_run.as_bytes()).is_ok());
}

#[test]
fn controls_accept_open_forge_identities() {
    let floor = OrganizationFloor::parse(FLOOR).unwrap();
    assert_eq!(floor.schema(), "amiss/organization-floor");
    assert_eq!(floor.repository.host, "gitlab.com");
    assert_eq!(floor.repository.owner, "platform/security");

    let (key, fact) = computed_digests();
    let item = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let debt = debt_snapshot("2026-07-02T00:00:00Z", &[item])
        .replace("\"host\": \"github.com\"", "\"host\": \"gitlab.com\"")
        .replace("\"owner\": \"acme\"", "\"owner\": \"platform/security\"");
    let debt_value = json::parse(debt.as_bytes()).unwrap();
    let debt = DebtSnapshot::parse(debt.as_bytes()).unwrap();
    assert_eq!(debt.schema(), "amiss/debt-snapshot");
    assert_eq!(debt.repository.owner, "platform/security");
    assert_eq!(debt.digest, hj("amiss/debt-snapshot", &debt_value));

    let item = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let waiver = waiver_bundle(&[item])
        .replace("\"host\": \"github.com\"", "\"host\": \"gitlab.com\"")
        .replace("\"owner\": \"acme\"", "\"owner\": \"platform/security\"");
    let waiver_value = json::parse(waiver.as_bytes()).unwrap();
    let waiver = WaiverBundle::parse(waiver.as_bytes()).unwrap();
    assert_eq!(waiver.schema(), "amiss/waiver-bundle");
    assert_eq!(waiver.repository.owner, "platform/security");
    assert_eq!(waiver.digest, hj("amiss/waiver-bundle", &waiver_value));

    let time = TrustedTimeStatement::parse(TIME_STATEMENT.as_bytes()).unwrap();
    assert_eq!(time.schema(), "amiss/scanner-trusted-time-statement");
    assert_eq!(time.controller(), "external-required-check-clock");
    assert_eq!(time.repository.owner, "platform/security");
    assert_eq!(time.provider, "gitlab-ci");
    assert_eq!(time.provider_run_id, "pipeline/01J2Z9-7");
}

const CONSTRAINT: &str = r#"{
  "schema": "amiss/scanner-execution-constraint",
  "action_repository": { "host": "github.com", "owner": "acme", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#;

#[test]
fn parses_an_execution_constraint_descriptor() {
    let descriptor = ExecutionConstraintDescriptor::parse(CONSTRAINT.as_bytes()).unwrap();
    assert_eq!(descriptor.selected_platform.as_str(), "linux-x86_64");
    assert_eq!(
        descriptor.required_status_name,
        "amiss / documentation assurance"
    );

    let open_repository = CONSTRAINT.replace(
        "\"host\": \"github.com\", \"owner\": \"acme\"",
        "\"host\": \"git.example.internal\", \"owner\": \"platform/security\"",
    );
    let descriptor = ExecutionConstraintDescriptor::parse(open_repository.as_bytes()).unwrap();
    assert_eq!(descriptor.action_repository.host, "git.example.internal");
    assert_eq!(descriptor.action_repository.owner, "platform/security");

    let slash_host = CONSTRAINT.replace("github.com", "git.example/internal");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(slash_host.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let malformed_owner =
        CONSTRAINT.replace("\"owner\": \"acme\"", "\"owner\": \"platform//security\"");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(malformed_owner.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );

    let trailing_space = CONSTRAINT.replace("assurance\"", "assurance \"");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(trailing_space.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let short_oid = CONSTRAINT.replace(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    assert_eq!(
        ExecutionConstraintDescriptor::parse(short_oid.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
}
