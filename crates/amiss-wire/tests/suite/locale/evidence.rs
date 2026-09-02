#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests mutate values produced by the checked locale evidence writer"
)]

use std::{fs, path::Path};

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json::{self, Value};
use amiss_wire::locale::{
    EVIDENCE_PAYLOAD_SCHEMA, EvidencePayloadSchema, LocaleCoverageEvidence, LocalePageInventory,
    LocaleSourcePage, LocaleTargetInventory, LocaleTargetOrigin, LocaleTargetPage, evidence,
    parse_evidence, parse_plan, plan,
};

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
        plan_payload_digest: hj(
            amiss_wire::locale::PLAN_PAYLOAD_SCHEMA,
            plan_value.member("payload").unwrap(),
        ),
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
    partial_source.source.pages.clear();
    let parsed = parse_evidence(&json::canonical(&evidence(&partial_source).unwrap())).unwrap();

    assert!(!parsed.payload.source.complete);
    assert!(parsed.payload.target.complete);
    assert!(parsed.payload.source.pages.is_empty());
}

#[test]
fn source_and_target_product_receipts_remain_independent() {
    let mut input = locale_evidence();
    input.source.product = Nullable::Value(product_resource('b'));
    input.target.product = Nullable::Value(product_resource('c'));

    let parsed = parse_evidence(&json::canonical(&evidence(&input).unwrap())).unwrap();
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

    let mut invalid_lineage = evidence(&input).unwrap();
    let target = member_mut(member_mut(&mut invalid_lineage, "payload"), "target");
    let Value::Array(pages) = member_mut(target, "pages") else {
        panic!("the checked writer produced a non-array target page set");
    };
    let origin = member_mut(pages.first_mut().unwrap(), "origin");
    *member_mut(origin, "based_on_source_digest") = Value::string("source-v1");
    let error = parse_evidence(&sealed(invalid_lineage)).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages[0].origin");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut missing_lineage = evidence(&input).unwrap();
    let target = member_mut(member_mut(&mut missing_lineage, "payload"), "target");
    let Value::Array(pages) = member_mut(target, "pages") else {
        panic!("the checked writer produced a non-array target page set");
    };
    let origin = member_mut(pages.first_mut().unwrap(), "origin");
    let Value::Object(members) = origin else {
        panic!("the checked writer produced a non-object origin");
    };
    let members = members
        .iter()
        .filter(|(name, _value)| name != "based_on_source_digest")
        .cloned()
        .collect();
    *origin = Value::object(members);
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

    for inventory in ["source", "target"] {
        let mut missing_product = evidence(&locale_evidence()).unwrap();
        let payload = member_mut(&mut missing_product, "payload");
        let inventory_value = member_mut(payload, inventory);
        let Value::Object(members) = inventory_value else {
            panic!("the checked writer produced a non-object inventory");
        };
        let members = members
            .iter()
            .filter(|(name, _value)| name != "product")
            .cloned()
            .collect();
        *inventory_value = Value::object(members);
        let error = parse_evidence(&sealed(missing_product)).unwrap_err();
        assert_eq!(error.path, format!("$.payload.{inventory}.product"));
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
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
