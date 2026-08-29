#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid locale plan identities and inspect exact refusals"
)]

use std::{fs, path::Path};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json;
use amiss_wire::locale::{
    LOCALE_DOCUMENT_BYTES, LocaleCoveragePlan, LocaleCoveragePolicy, LocaleCoverageScope,
    LocalePageRequirement, PAGE_KEY_BYTES, PLAN_PAYLOAD_SCHEMA, parse_plan, plan,
};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::publication::{DocsCandidate, PublicationProducer};

mod evidence;

fn digest(digit: char) -> Digest {
    Digest::from_wire(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

fn oid(digit: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, digit.to_string().repeat(40)).unwrap()
}

fn identity(value: &str) -> ArtifactId {
    ArtifactId::new(value.to_owned()).unwrap()
}

fn locale_plan() -> LocaleCoveragePlan {
    LocaleCoveragePlan {
        report_payload_digest: digest('1'),
        docs: DocsCandidate {
            repository: RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap(),
            commit: oid('a'),
            tree: oid('b'),
            candidate_identity_digest: digest('2'),
        },
        scope: LocaleCoverageScope {
            site: identity("widget-docs"),
            source_locale: "en".to_owned(),
            target_locale: "de-DE".to_owned(),
            channel: identity("stable"),
            version: Some("1.2".to_owned()),
        },
        producer: PublicationProducer {
            identity: identity("sphinx-locale-manifest"),
            version: "1.0.0".to_owned(),
            context_digest: digest('3'),
        },
        policy: LocaleCoveragePolicy {
            identity: identity("product-docs-coverage"),
            context_digest: digest('4'),
            required: LocalePageRequirement::Named(vec![
                "guide/getting-started".to_owned(),
                "reference/api".to_owned(),
            ]),
        },
    }
}

#[test]
fn locale_plan_round_trips_with_its_payload_digest_and_example() {
    let expected = locale_plan();
    let value = plan(&expected).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_plan(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(PLAN_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    let example_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/locale-coverage-plan.json"),
    )
    .unwrap();
    let example_value = json::parse(&example_bytes).unwrap();
    let expected_digest = hj(
        PLAN_PAYLOAD_SCHEMA,
        example_value.member("payload").unwrap(),
    )
    .to_string();
    assert_eq!(
        example_value.text("payload_digest"),
        Some(expected_digest.as_str())
    );
    let example = parse_plan(&example_bytes).unwrap();
    assert_eq!(
        json::canonical(&plan(&example.payload).unwrap()),
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn locale_plan_keeps_all_source_and_named_policies_distinct() {
    let mut all_source = locale_plan();
    all_source.scope.version = None;
    all_source.policy.required = LocalePageRequirement::AllSource;

    let parsed = parse_plan(&json::canonical(&plan(&all_source).unwrap())).unwrap();
    assert_eq!(parsed.payload, all_source);
}

#[test]
fn locale_plan_refuses_ambiguous_scope_and_invalid_open_identities() {
    let mut same_locale = locale_plan();
    same_locale.scope.target_locale = same_locale.scope.source_locale.clone();
    let error = plan(&same_locale).unwrap_err();
    assert_eq!(error.path, "$.payload.scope");
    assert_eq!(error.kind, ErrorKind::Inconsistent);

    let mut invalid_locale = locale_plan();
    invalid_locale.scope.target_locale = "de/DE".to_owned();
    let error = plan(&invalid_locale).unwrap_err();
    assert_eq!(error.path, "$.payload.scope.target_locale");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut invalid_scope_version = locale_plan();
    invalid_scope_version.scope.version = Some(String::new());
    let error = plan(&invalid_scope_version).unwrap_err();
    assert_eq!(error.path, "$.payload.scope.version");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut invalid_producer_version = locale_plan();
    invalid_producer_version.producer.version = "v 1".to_owned();
    let error = plan(&invalid_producer_version).unwrap_err();
    assert_eq!(error.path, "$.payload.producer.version");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn locale_plan_requires_one_sorted_unique_bounded_named_set() {
    for (keys, path, kind) in [
        (
            Vec::new(),
            "$.payload.policy.required.keys",
            ErrorKind::InvalidValue,
        ),
        (
            vec!["reference/api".to_owned(), "guide/start".to_owned()],
            "$.payload.policy.required.keys",
            ErrorKind::UnsortedSet,
        ),
        (
            vec!["guide/start".to_owned(), "guide/start".to_owned()],
            "$.payload.policy.required.keys",
            ErrorKind::DuplicateMember,
        ),
        (
            vec!["guide/\nstart".to_owned()],
            "$.payload.policy.required.keys[0]",
            ErrorKind::InvalidValue,
        ),
        (
            vec!["x".repeat(PAGE_KEY_BYTES + 1)],
            "$.payload.policy.required.keys[0]",
            ErrorKind::InvalidValue,
        ),
    ] {
        let mut candidate = locale_plan();
        candidate.policy.required = LocalePageRequirement::Named(keys);
        let error = plan(&candidate).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, kind);
    }

    let mut boundary = locale_plan();
    boundary.policy.required = LocalePageRequirement::Named(vec!["x".repeat(PAGE_KEY_BYTES)]);
    assert!(plan(&boundary).is_ok());
}

#[test]
fn locale_plan_refuses_tampering_open_shapes_and_oversized_documents() {
    let value = plan(&locale_plan()).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_plan(&bytes).unwrap();
    let tampered = String::from_utf8(bytes)
        .unwrap()
        .replace(&parsed.payload_digest.to_string(), &digest('f').to_string());
    let error = parse_plan(tampered.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let value = plan(&locale_plan()).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let open = String::from_utf8(json::canonical(&value))
        .unwrap()
        .replacen(
            "\"report_payload_digest\":",
            "\"unknown\":true,\"report_payload_digest\":",
            1,
        );
    let open_value = json::parse(open.as_bytes()).unwrap();
    let rebound = open.replace(
        recorded,
        &hj(PLAN_PAYLOAD_SCHEMA, open_value.member("payload").unwrap()).to_string(),
    );
    let error = parse_plan(rebound.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.unknown");
    assert_eq!(error.kind, ErrorKind::UnknownField);

    let oversized = vec![b' '; usize::try_from(LOCALE_DOCUMENT_BYTES).unwrap() + 1];
    let error = parse_plan(&oversized).unwrap_err();
    assert_eq!(error.path, "$");
    assert_eq!(error.kind, ErrorKind::LimitExceeded);
}
