#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests mutate values produced by the checked locale evidence writer"
)]

use std::{collections::BTreeMap, fs, path::Path};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json::{self, Value};
use amiss_wire::locale::{
    EVIDENCE_PAYLOAD_SCHEMA, LocaleCoverageEvidence, LocalePageInventory, LocaleTargetInventory,
    LocaleTargetOrigin, LocaleTargetPage, evidence, parse_evidence, parse_plan, plan,
};

use super::{digest, locale_plan};

pub(super) fn page_map<T>(
    pages: &[(&str, char)],
    value: impl Fn(char) -> T,
) -> BTreeMap<String, T> {
    pages
        .iter()
        .map(|(key, digit)| ((*key).to_owned(), value(*digit)))
        .collect()
}

pub(super) fn target_page(resource_digit: char) -> LocaleTargetPage {
    LocaleTargetPage {
        resource_digest: digest(resource_digit),
        origin: LocaleTargetOrigin::TargetResource,
    }
}

pub(super) fn fallback_page(
    resource_digit: char,
    class: &str,
    source_digit: char,
) -> LocaleTargetPage {
    LocaleTargetPage {
        resource_digest: digest(resource_digit),
        origin: LocaleTargetOrigin::Fallback {
            class: super::identity(class),
            source_resource_digest: digest(source_digit),
        },
    }
}

pub(super) fn locale_evidence() -> LocaleCoverageEvidence {
    let planned = locale_plan();
    let plan_value = plan(&planned).unwrap();
    LocaleCoverageEvidence {
        plan_payload_digest: hj(
            amiss_wire::locale::PLAN_PAYLOAD_SCHEMA,
            plan_value.member("payload").unwrap(),
        ),
        docs: planned.docs,
        scope: planned.scope,
        producer: planned.producer,
        source: LocalePageInventory {
            input_digest: digest('5'),
            complete: true,
            pages: page_map(
                &[("guide/getting-started", '6'), ("reference/api", '7')],
                digest,
            ),
        },
        target: LocaleTargetInventory {
            input_digest: digest('8'),
            complete: true,
            pages: page_map(&[("guide/getting-started", '9')], target_page),
        },
    }
}

#[test]
fn locale_evidence_round_trips_with_independent_inventories_and_example() {
    let expected = locale_evidence();
    let value = evidence(&expected).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(EVIDENCE_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let example_bytes = fs::read(examples.join("locale-coverage-evidence.json")).unwrap();
    let example_value = json::parse(&example_bytes).unwrap();
    let expected_digest = hj(
        EVIDENCE_PAYLOAD_SCHEMA,
        example_value.member("payload").unwrap(),
    )
    .to_string();
    assert_eq!(
        example_value.text("payload_digest"),
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
        json::canonical(&evidence(&example.payload).unwrap()),
        json::canonical(&example_value)
    );
}

#[test]
fn source_and_target_completeness_remain_independent() {
    let mut partial_source = locale_evidence();
    partial_source.source.complete = false;
    partial_source.target.complete = true;
    partial_source.source.pages = BTreeMap::new();
    let parsed = parse_evidence(&json::canonical(&evidence(&partial_source).unwrap())).unwrap();

    assert!(!parsed.payload.source.complete);
    assert!(parsed.payload.target.complete);
    assert!(parsed.payload.source.pages.is_empty());
}

#[test]
fn every_target_page_carries_a_closed_origin_and_exact_fallback_source() {
    let mut input = locale_evidence();
    input.target.pages.insert(
        "reference/api".to_owned(),
        fallback_page('a', "source-copy", '7'),
    );
    let parsed = parse_evidence(&json::canonical(&evidence(&input).unwrap())).unwrap();
    assert_eq!(parsed.payload, input);

    let mut unknown = evidence(&input).unwrap();
    let target = member_mut(member_mut(&mut unknown, "payload"), "target");
    let Value::Array(pages) = member_mut(target, "pages") else {
        panic!("the checked writer produced a non-array target page set");
    };
    let fallback = pages.last_mut().unwrap();
    let origin = member_mut(fallback, "origin");
    *member_mut(origin, "kind") = Value::string("generated");
    let error = parse_evidence(&sealed(unknown)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[1].origin.kind");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut missing_origin = evidence(&locale_evidence()).unwrap();
    let target = member_mut(member_mut(&mut missing_origin, "payload"), "target");
    let Value::Array(pages) = member_mut(target, "pages") else {
        panic!("the checked writer produced a non-array target page set");
    };
    *member_mut(pages.first_mut().unwrap(), "origin") = Value::Null;
    let error = parse_evidence(&sealed(missing_origin)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[0].origin");
    assert_eq!(error.kind, ErrorKind::WrongType);
}

#[test]
fn locale_evidence_refuses_invalid_page_keys_and_tampering() {
    let mut invalid_key = locale_evidence();
    invalid_key
        .source
        .pages
        .insert("reference/\napi".to_owned(), digest('a'));
    let error = evidence(&invalid_key).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages[1].key");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let value = evidence(&locale_evidence()).unwrap();
    let bytes = json::canonical(&value);
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
    let mut unsorted = evidence(&locale_evidence()).unwrap();
    pages_mut(&mut unsorted).reverse();
    let error = parse_evidence(&sealed(unsorted)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut duplicate = evidence(&locale_evidence()).unwrap();
    let pages = pages_mut(&mut duplicate);
    let first = pages.first().unwrap().clone();
    *pages.last_mut().unwrap() = first;
    let error = parse_evidence(&sealed(duplicate)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut mistyped = evidence(&locale_evidence()).unwrap();
    let payload = member_mut(&mut mistyped, "payload");
    let source = member_mut(payload, "source");
    *member_mut(source, "complete") = Value::string("true");
    let error = parse_evidence(&sealed(mistyped)).unwrap_err();
    assert_eq!(error.path, "$.payload.source.complete");
    assert_eq!(error.kind, ErrorKind::WrongType);
}

fn pages_mut(value: &mut Value) -> &mut [Value] {
    let payload = member_mut(value, "payload");
    let source = member_mut(payload, "source");
    let Value::Array(pages) = member_mut(source, "pages") else {
        panic!("the checked writer produced a non-array page set");
    };
    pages
}

fn sealed(mut value: Value) -> Vec<u8> {
    let digest = hj(EVIDENCE_PAYLOAD_SCHEMA, value.member("payload").unwrap());
    *member_mut(&mut value, "payload_digest") = Value::string(digest.to_string());
    json::canonical(&value)
}

fn member_mut<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    let Value::Object(members) = value else {
        panic!("the checked writer produced a non-object value");
    };
    members
        .iter_mut()
        .find(|(key, _value)| key == name)
        .map(|(_key, value)| value)
        .unwrap()
}
