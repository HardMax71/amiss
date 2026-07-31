use amiss_wire::controls::{DebtSnapshot, FACT_DOMAIN, FINDING_KEY_DOMAIN, Fact};
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::digest::hj;
use amiss_wire::json;
use amiss_wire::resolution::{BlobContent, BlobMode, Missing, Resolution, Target};

use crate::support::{
    PROJECTION_DIGEST, RAW_DIGEST, debt_item_json, debt_snapshot, fact_json_for, key_input_json,
};

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
