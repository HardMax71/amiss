#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests mutate values produced by the checked locale evidence writer"
)]

use sha2::Digest as _;
use std::{fs, path::Path};

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;

use amiss_wire::locale::{
    EVIDENCE_PAYLOAD_SCHEMA, EvidencePayloadSchema, LocaleCoverageEvidence, LocalePageInventory,
    LocaleSourcePage, LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage, evidence,
    parse_evidence, parse_plan, plan,
};
use serde_json::Value;

use super::{digest, locale_plan, product_resource};

pub(super) fn page_map<T>(pages: &[(&str, char)], value: impl Fn(&str, char) -> T) -> Vec<T> {
    pages
        .iter()
        .map(|(key, digit)| value(key, *digit))
        .collect()
}

pub(super) fn target_page(
    key: &str,
    resource_digit: char,
    based_on_source_digit: Option<char>,
) -> LocaleTargetPage {
    LocaleTargetPage {
        key: key.to_owned(),
        resource_digest: digest(resource_digit),
        origin: LocaleTargetOrigin::TargetResource {
            based_on_source_digest: based_on_source_digit
                .map_or(Nullable::Null, |digit| Nullable::Value(digest(digit))),
        },
    }
}

pub(super) fn fallback_page(
    key: &str,
    resource_digit: char,
    class: &str,
    source_digit: char,
) -> LocaleTargetPage {
    let class = super::identity(class);
    let source_resource_digest = digest(source_digit);
    LocaleTargetPage {
        key: key.to_owned(),
        resource_digest: digest(resource_digit),
        origin: LocaleTargetOrigin::Fallback {
            class,
            source_resource_digest,
        },
    }
}

pub(super) fn set_target_page(pages: &mut Vec<LocaleTargetPage>, page: LocaleTargetPage) {
    match pages.binary_search_by(|current| current.key.cmp(&page.key)) {
        Ok(index) => {
            let current = pages.get_mut(index).unwrap();
            *current = page;
        }
        Err(index) => pages.insert(index, page),
    }
}

pub(super) fn locale_evidence() -> LocaleCoverageEvidence {
    let planned = locale_plan();
    let plan_value = plan(&planned).unwrap();
    LocaleCoverageEvidence {
        schema: EvidencePayloadSchema::Current,
        plan_payload_digest: parse_plan(&plan_value).unwrap().payload_digest,
        docs: planned.docs,
        scope: planned.scope,
        producer: planned.producer,
        source: LocalePageInventory {
            input_digest: digest('5'),
            product: Nullable::Null,
            complete: true,
            pages: page_map(
                &[("guide/getting-started", '6'), ("reference/api", '7')],
                |key, digit| LocaleSourcePage {
                    key: key.to_owned(),
                    resource_digest: digest(digit),
                },
            ),
        },
        target: LocaleTargetInventory {
            input_digest: digest('8'),
            product: Nullable::Null,
            complete: true,
            pages: page_map(&[("guide/getting-started", '9')], |key, digit| {
                target_page(key, digit, None)
            }),
        },
    }
}

#[test]
fn locale_evidence_round_trips_with_independent_inventories_and_example() {
    let expected = locale_evidence();
    let bytes = evidence(&expected).unwrap();
    let value = serde_json::from_slice::<Value>(&bytes).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(
                    serde_json_canonicalizer::to_vec(value.get("payload").unwrap()).unwrap()
                )
                .finalize()
                .0
        )
    );

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let example_bytes = fs::read(examples.join("locale-coverage-evidence.json")).unwrap();
    let example_value = serde_json::from_slice::<Value>(&example_bytes).unwrap();
    let expected_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(
                serde_json_canonicalizer::to_vec(example_value.get("payload").unwrap()).unwrap(),
            )
            .finalize()
            .0,
    )
    .to_string();
    assert_eq!(
        example_value.get("payload_digest").and_then(Value::as_str),
        Some(expected_digest.as_str())
    );
    let example = parse_evidence(&example_bytes).unwrap();
    let planned =
        parse_plan(&fs::read(examples.join("locale-coverage-plan.json")).unwrap()).unwrap();
    assert_eq!(example.payload.plan_payload_digest, planned.payload_digest);
    assert_eq!(example.payload.docs, planned.payload.docs);
    assert_eq!(example.payload.scope, planned.payload.scope);
    assert_eq!(example.payload.producer, planned.payload.producer);
    assert_eq!(
        evidence(&example.payload).unwrap(),
        serde_json_canonicalizer::to_vec(&example_value).unwrap()
    );
}

#[test]
fn source_and_target_completeness_remain_independent() {
    let mut partial_source = locale_evidence();
    partial_source.source.complete = false;
    partial_source.target.complete = true;
    partial_source.source.pages.clear();
    let parsed = parse_evidence(&evidence(&partial_source).unwrap()).unwrap();

    assert!(!parsed.payload.source.complete);
    assert!(parsed.payload.target.complete);
    assert!(parsed.payload.source.pages.is_empty());
}

#[test]
fn source_and_target_product_receipts_remain_independent() {
    let mut input = locale_evidence();
    input.source.product = Nullable::Value(product_resource('b'));
    input.target.product = Nullable::Value(product_resource('c'));

    let bytes = evidence(&input).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();
    assert_eq!(parsed.payload.source.product, input.source.product);
    assert_eq!(parsed.payload.target.product, input.target.product);
}

#[test]
fn every_target_page_carries_a_closed_origin_and_exact_fallback_source() {
    let mut input = locale_evidence();
    set_target_page(
        &mut input.target.pages,
        target_page("guide/getting-started", '9', Some('6')),
    );
    set_target_page(
        &mut input.target.pages,
        fallback_page("reference/api", 'a', "source-copy", '7'),
    );
    let parsed = parse_evidence(&evidence(&input).unwrap()).unwrap();
    assert_eq!(parsed.payload, input);

    let value = serde_json::from_slice::<Value>(&evidence(&input).unwrap()).unwrap();
    let mut unknown = value.clone();
    let target = ((unknown).get_mut("payload").unwrap())
        .get_mut("target")
        .unwrap();
    let Value::Array(pages) = (target).get_mut("pages").unwrap() else {
        panic!("the checked writer produced a non-array target page set");
    };
    let fallback = pages.last_mut().unwrap();
    let origin = (fallback).get_mut("origin").unwrap();
    *(origin).get_mut("kind").unwrap() = Value::from("generated");
    let error = parse_evidence(&sealed(unknown)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[1].origin.kind");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut missing_origin =
        serde_json::from_slice::<Value>(&evidence(&locale_evidence()).unwrap()).unwrap();
    let target = ((missing_origin).get_mut("payload").unwrap())
        .get_mut("target")
        .unwrap();
    let Value::Array(pages) = (target).get_mut("pages").unwrap() else {
        panic!("the checked writer produced a non-array target page set");
    };
    *(pages.first_mut().unwrap()).get_mut("origin").unwrap() = Value::Null;
    let error = parse_evidence(&sealed(missing_origin)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[0].origin");
    assert_eq!(error.kind, ErrorKind::WrongType);

    let mut invalid_lineage = value.clone();
    let target = ((invalid_lineage).get_mut("payload").unwrap())
        .get_mut("target")
        .unwrap();
    let Value::Array(pages) = (target).get_mut("pages").unwrap() else {
        panic!("the checked writer produced a non-array target page set");
    };
    let origin = (pages.first_mut().unwrap()).get_mut("origin").unwrap();
    *(origin).get_mut("based_on_source_digest").unwrap() = Value::from("source-v1");
    let error = parse_evidence(&sealed(invalid_lineage)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[0].origin");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut missing_lineage = value;
    let target = ((missing_lineage).get_mut("payload").unwrap())
        .get_mut("target")
        .unwrap();
    let Value::Array(pages) = (target).get_mut("pages").unwrap() else {
        panic!("the checked writer produced a non-array target page set");
    };
    let origin = (pages.first_mut().unwrap()).get_mut("origin").unwrap();
    let Value::Object(members) = origin else {
        panic!("the checked writer produced a non-object origin");
    };
    members.remove("based_on_source_digest");
    let error = parse_evidence(&sealed(missing_lineage)).unwrap_err();
    assert_eq!(
        error.path,
        "$.payload.target.pages[0].origin.based_on_source_digest"
    );
    assert_eq!(error.kind, ErrorKind::MissingField);
}

#[test]
fn locale_evidence_refuses_invalid_page_keys_and_tampering() {
    let mut invalid_key = locale_evidence();
    invalid_key.source.pages.insert(
        1,
        LocaleSourcePage {
            key: "reference/\napi".to_owned(),
            resource_digest: digest('a'),
        },
    );
    let error = evidence(&invalid_key).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages[1].key");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let bytes = evidence(&locale_evidence()).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();
    let tampered = String::from_utf8(bytes)
        .unwrap()
        .replace(&parsed.payload_digest.to_string(), &digest('f').to_string());
    let error = parse_evidence(tampered.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
}

#[test]
fn locale_evidence_refuses_unsorted_duplicate_and_mistyped_inventories() {
    let bytes = evidence(&locale_evidence()).unwrap();
    let value = serde_json::from_slice::<Value>(&bytes).unwrap();
    let mut unsorted = value.clone();
    pages_mut(&mut unsorted).reverse();
    let error = parse_evidence(&sealed(unsorted)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut duplicate = value.clone();
    let pages = pages_mut(&mut duplicate);
    let first = pages.first().unwrap().clone();
    *pages.last_mut().unwrap() = first;
    let error = parse_evidence(&sealed(duplicate)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut mistyped = value.clone();
    let payload = (mistyped).get_mut("payload").unwrap();
    let source = (payload).get_mut("source").unwrap();
    *(source).get_mut("complete").unwrap() = Value::from("true");
    let error = parse_evidence(&sealed(mistyped)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.complete");
    assert_eq!(error.kind, ErrorKind::WrongType);

    for inventory in ["source", "target"] {
        let mut missing_product = value.clone();
        let payload = (missing_product).get_mut("payload").unwrap();
        let inventory_value = (payload).get_mut(inventory).unwrap();
        let Value::Object(members) = inventory_value else {
            panic!("the checked writer produced a non-object inventory");
        };
        members.remove("product");
        let error = parse_evidence(&sealed(missing_product)).unwrap_err();
        assert_eq!(error.path, format!("$.payload.{inventory}.product"));
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
}

fn pages_mut(value: &mut Value) -> &mut [Value] {
    let payload = (value).get_mut("payload").unwrap();
    let source = (payload).get_mut("source").unwrap();
    let Value::Array(pages) = (source).get_mut("pages").unwrap() else {
        panic!("the checked writer produced a non-array page set");
    };
    pages
}

fn sealed(mut value: Value) -> Vec<u8> {
    let digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(value.get("payload").unwrap()).unwrap())
            .finalize()
            .0,
    );
    *(value).get_mut("payload_digest").unwrap() = Value::from(digest.to_string());
    serde_json_canonicalizer::to_vec(&value).unwrap()
}
