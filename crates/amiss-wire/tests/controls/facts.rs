use amiss_wire::assessment::Nullable;
use amiss_wire::controls::{
    DebtSnapshot, FACT_DOMAIN, FINDING_KEY_DOMAIN, Fact, FactSchema, FindingKeyInput,
    MissingResolution, StructuralResolution, parse_debt_snapshot, parse_fact,
};
use amiss_wire::de::{Error, ErrorKind};
use sha2::Digest as _;

use amiss_wire::report::model::{FindingFactEvidence, RepoPath};
use amiss_wire::resolution::{BlobContent, BlobMode, Target};
use serde_json::Value;

use super::support::{DEBT, PROJECTION_DIGEST, RAW_DIGEST};

#[expect(
    clippy::unwrap_used,
    reason = "the published fixture supplies the production key model"
)]
fn key_input(finding_kind: &str) -> FindingKeyInput<String> {
    let snapshot: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    let key = snapshot
        .items
        .into_iter()
        .next()
        .unwrap()
        .accepted_fact
        .key_input;
    FindingKeyInput {
        schema: key.schema,
        finding_kind: finding_kind.to_owned(),
        scope: key.scope,
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "negative fixtures replace only the fields under test"
)]
fn fact<'a>(
    finding_kind: &'a str,
    key_input: &str,
    resolution: &str,
) -> Fact<Value, FindingFactEvidence<RepoPath, Value>, &'a str> {
    Fact {
        schema: FactSchema::Current,
        finding_kind,
        key_input: serde_json::from_str(key_input).unwrap(),
        evidence: FindingFactEvidence::Reference {
            resolution: serde_json::from_str(resolution).unwrap(),
            occurrence_multiplicity: 1,
        },
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "the fixture key serializes through Serde"
)]
fn parse_debt_fact_case(
    fact_finding_kind: &str,
    key_finding_kind: &str,
    resolution: &str,
) -> Result<DebtSnapshot, Error> {
    let key = serde_json::to_string(&key_input(key_finding_kind)).unwrap();
    parse_debt_fact(fact_finding_kind, &key, resolution)
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "negative fixtures keep the published outer control and rebind the edited fact"
)]
fn parse_debt_fact(
    fact_finding_kind: &str,
    key_input: &str,
    resolution: &str,
) -> Result<DebtSnapshot, Error> {
    let fact = fact(fact_finding_kind, key_input, resolution);
    let finding_key = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(FINDING_KEY_DOMAIN)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&fact.key_input).unwrap())
            .finalize()
            .0,
    );
    let fact_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(FACT_DOMAIN)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&fact).unwrap())
            .finalize()
            .0,
    );
    let template: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    let mut document = serde_json::to_value(template).unwrap();
    let item = &mut document["items"][0];
    item["accepted_fact"] = serde_json::to_value(fact).unwrap();
    item["finding_key"] = serde_json::json!(finding_key);
    item["accepted_fact_digest"] = serde_json::json!(fact_digest);
    parse_debt_snapshot(&serde_json::to_vec(&document).unwrap())
}

#[test]
fn structural_facts_accept_an_optional_full_commit_identity() {
    let with_commit = |commit_oid: &str| {
        serde_json::to_string_pretty(&key_input("explicit-target-missing"))
            .unwrap()
            .replace(
                "\"kind\": \"repository-path\",",
                &format!("\"kind\": \"repository-path\",\n      \"commit_oid\": \"{commit_oid}\","),
            )
    };
    for commit_oid in ["a".repeat(40), "b".repeat(64)] {
        let key_input = with_commit(&commit_oid);
        let parsed = parse_debt_fact(
            "explicit-target-missing",
            &key_input,
            r#"{"kind":"missing","reason":"path-not-found","path":"docs/example.md","near":null}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&parsed.items[0].accepted_fact.key_input).unwrap(),
            serde_json_canonicalizer::to_vec(
                &serde_json::from_slice::<Value>(key_input.as_bytes()).unwrap()
            )
            .unwrap(),
        );
        assert_eq!(
            parsed.items[0]
                .accepted_fact
                .key_input
                .scope
                .normalized_target_intent
                .commit_oid
                .as_ref()
                .map(amiss_wire::model::Oid::as_str),
            Some(commit_oid.as_str())
        );
    }

    let invalid = with_commit("deadbeef");
    let defect = parse_debt_fact(
        "explicit-target-missing",
        &invalid,
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/example.md","near":null}"#,
    )
    .unwrap_err();
    assert_eq!(defect.kind, ErrorKind::InvalidValue);
    assert!(
        defect
            .path
            .ends_with(".normalized_target_intent.commit_oid")
    );

    let null = serde_json::to_string_pretty(&key_input("explicit-target-missing"))
        .unwrap()
        .replace(
            "\"kind\": \"repository-path\",",
            "\"kind\": \"repository-path\",\n      \"commit_oid\": null,",
        );
    let defect = parse_debt_fact(
        "explicit-target-missing",
        &null,
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/example.md","near":null}"#,
    )
    .unwrap_err();
    assert_eq!(defect.kind, ErrorKind::DigestMismatch);
    let fact = fact(
        "explicit-target-missing",
        &null,
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/example.md","near":null}"#,
    );
    let parsed = parse_fact(&serde_json::to_vec(&fact).unwrap()).unwrap();
    assert_eq!(
        parsed.key_input.scope.normalized_target_intent.commit_oid,
        None
    );
}

#[test]
fn structural_resolution_facts_accept_both_missing_reasons() {
    let path_missing = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{
          "kind": "missing",
          "reason": "path-not-found",
          "path": "docs/missing.md",
          "near": null,
          "same_object_at": "docs/moved.md"
        }"#,
    )
    .unwrap();
    assert!(matches!(
        &path_missing.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::Missing(MissingResolution::PathNotFound {
            path,
            same_object_at: Some(Nullable::Value(moved)),
            ..
        }) if path.as_str() == "docs/missing.md" && moved.as_str() == "docs/moved.md"
    ));

    let explicit_null = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{
          "kind": "missing",
          "reason": "path-not-found",
          "path": "docs/missing.md",
          "near": null,
          "same_object_at": null
        }"#,
    )
    .unwrap();
    assert!(matches!(
        &explicit_null.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::Missing(MissingResolution::PathNotFound {
            same_object_at: Some(Nullable::Null),
            ..
        })
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
        &line_missing.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::Missing(MissingResolution::LineFragmentOutOfRange { path })
            if path.as_str() == "src/lib.rs"
    ));

    let omitted = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md","near":null}"#,
    )
    .unwrap();
    let mut digests = std::collections::BTreeSet::new();
    for snapshot in [omitted, explicit_null, path_missing] {
        let item = &snapshot.items[0];
        assert_eq!(
            serde_json::to_vec(&item.accepted_fact.key_input).unwrap(),
            serde_json_canonicalizer::to_vec(
                &serde_json::from_slice::<Value>(
                    serde_json::to_string_pretty(&key_input("explicit-target-missing"))
                        .unwrap()
                        .as_bytes()
                )
                .unwrap()
            )
            .unwrap(),
        );
        let bytes = serde_json_canonicalizer::to_vec(&item.accepted_fact).unwrap();
        let digest = amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-fact")
                .chain_update([0_u8])
                .chain_update(&bytes)
                .finalize()
                .0,
        );
        assert_eq!(digest, item.accepted_fact_digest);
        assert_eq!(parse_fact(&bytes).unwrap(), item.accepted_fact);
        assert!(
            digests.insert(digest),
            "absence, null and a path have distinct facts"
        );
    }
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
        &tree_mismatch.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::TypeMismatch { target: Target::Tree { path } }
            if path.as_str() == "docs"
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
        &available_blob.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::TypeMismatch { target: Target::Blob(blob) }
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
        &lfs_blob.items[0].accepted_fact.evidence.resolution,
        StructuralResolution::TypeMismatch { target: Target::Blob(blob) }
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
            r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md","near":null,"status":"missing","code":"path-not-found","entry_kind":null,"git_mode":null,"raw_digest":null,"projection_digest":null,"content_availability":"not-applicable"}"#,
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
    let missing =
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md","near":null}"#;
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
fn structural_fact_validation_rejects_invalid_programmatic_states() {
    let parsed = parse_debt_fact_case(
        "explicit-target-missing",
        "explicit-target-missing",
        r#"{"kind":"missing","reason":"path-not-found","path":"docs/missing.md","near":null}"#,
    )
    .unwrap();
    let accepted = &parsed.items[0].accepted_fact;
    assert!(accepted.validate().is_ok());

    let mut mismatched = accepted.clone();
    mismatched.evidence.resolution = StructuralResolution::TypeMismatch {
        target: Target::Tree {
            path: mismatched
                .key_input
                .scope
                .normalized_target_intent
                .path
                .clone(),
        },
    };
    assert_eq!(
        mismatched.validate().unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    mismatched.finding_kind = amiss_wire::controls::EligibleFindingKind::ExplicitTargetTypeMismatch;
    assert_eq!(
        mismatched.validate().unwrap_err().kind,
        ErrorKind::Inconsistent
    );
}
