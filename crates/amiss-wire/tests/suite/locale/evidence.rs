#![expect(
    clippy::unwrap_used,
    reason = "locale fixtures construct checked plans and evidence"
)]

use std::{fs, path::Path};

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;

use amiss_wire::locale::{
    EVIDENCE_DOCUMENT_BYTES, EVIDENCE_PAYLOAD_SCHEMA, EvidencePayloadSchema,
    LocaleCoverageEvidence, LocalePageInventory, LocaleSourcePage, LocaleTargetInventory,
    LocaleTargetOrigin, LocaleTargetPage, evidence, parse_evidence, parse_plan, plan,
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
    let planned = plan(locale_plan()).unwrap();
    LocaleCoverageEvidence {
        schema: EvidencePayloadSchema::Current,
        plan_payload_digest: planned.payload_digest,
        docs: planned.payload.docs,
        scope: planned.payload.scope,
        producer: planned.payload.producer,
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
    let envelope = evidence(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, EVIDENCE_DOCUMENT_BYTES).unwrap();
    assert_eq!(parse_evidence(&bytes).unwrap(), envelope);
    assert_eq!(envelope.payload, expected);
    assert_eq!(
        envelope.payload_digest,
        amiss_wire::digest::hb(
            EVIDENCE_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let example_bytes = fs::read(examples.join("locale-coverage-evidence.json")).unwrap();
    let example = parse_evidence(&example_bytes).unwrap();
    let planned =
        parse_plan(&fs::read(examples.join("locale-coverage-plan.json")).unwrap()).unwrap();
    assert_eq!(example.payload.plan_payload_digest, planned.payload_digest);
    assert_eq!(example.payload.docs, planned.payload.docs);
    assert_eq!(example.payload.scope, planned.payload.scope);
    assert_eq!(example.payload.producer, planned.payload.producer);
    assert_eq!(evidence(example.payload.clone()).unwrap(), example);
}

#[test]
fn source_and_target_completeness_remain_independent() {
    let mut partial_source = locale_evidence();
    partial_source.source.complete = false;
    partial_source.target.complete = true;
    partial_source.source.pages.clear();
    let envelope = evidence(partial_source).unwrap();

    assert!(!envelope.payload.source.complete);
    assert!(envelope.payload.target.complete);
    assert!(envelope.payload.source.pages.is_empty());
}

#[test]
fn source_and_target_product_receipts_remain_independent() {
    let mut input = locale_evidence();
    input.source.product = Nullable::Value(product_resource('b'));
    input.target.product = Nullable::Value(product_resource('c'));

    let envelope = evidence(input.clone()).unwrap();
    assert_eq!(envelope.payload.source.product, input.source.product);
    assert_eq!(envelope.payload.target.product, input.target.product);
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
    let envelope = evidence(input.clone()).unwrap();
    assert_eq!(envelope.payload, input);
    let text = serde_json::to_string(&envelope).unwrap();
    let origin = serde_json::to_string(&input.target.pages[0].origin).unwrap();
    let lineage = format!(
        ",\"based_on_source_digest\":{}",
        serde_json::to_string(&digest('6')).unwrap()
    );
    for (member, replacement, path) in [
        (
            r#""kind":"fallback""#.to_owned(),
            r#""kind":"generated""#.to_owned(),
            "$.payload.target.pages[1].origin.kind",
        ),
        (
            origin,
            "null".to_owned(),
            "$.payload.target.pages[0].origin",
        ),
        (
            lineage.clone(),
            r#","based_on_source_digest":"source-v1""#.to_owned(),
            "$.payload.target.pages[0].origin",
        ),
        (lineage, String::new(), "$.payload.target.pages[0].origin"),
    ] {
        assert_eq!(text.matches(&member).count(), 1);
        let changed = text.replacen(&member, &replacement, 1);
        assert_ne!(changed, text);
        let error = parse_evidence(changed.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
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
    let error = evidence(invalid_key).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages[1].key");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));

    let mut tampered = evidence(locale_evidence()).unwrap();
    tampered.payload_digest = digest('f');
    let error = parse_evidence(&serde_json::to_vec(&tampered).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert!(matches!(error.kind, ErrorKind::DigestMismatch));
}

#[test]
fn locale_evidence_refuses_unsorted_duplicate_and_mistyped_inventories() {
    let envelope = evidence(locale_evidence()).unwrap();
    let mut unsorted = envelope.clone();
    unsorted.payload.source.pages.reverse();
    let mut duplicate = envelope.clone();
    duplicate.payload.source.pages[1] = duplicate.payload.source.pages[0].clone();
    for (mut document, kind) in [
        (unsorted, ErrorKind::UnsortedSet),
        (duplicate, ErrorKind::DuplicateMember),
    ] {
        assert_eq!(
            std::mem::discriminant(&evidence(document.payload.clone()).unwrap_err().kind),
            std::mem::discriminant(&kind)
        );
        document.payload_digest = amiss_wire::digest::hb(
            EVIDENCE_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let error = parse_evidence(&serde_json::to_vec(&document).unwrap()).unwrap_err();
        assert_eq!(error.path, "$.payload.source.pages");
        assert_eq!(
            std::mem::discriminant(&error.kind),
            std::mem::discriminant(&kind)
        );
    }

    let text = serde_json::to_string(&envelope).unwrap();
    for (inventory, path) in [
        (
            serde_json::to_string(&envelope.payload.source).unwrap(),
            "$.payload.source",
        ),
        (
            serde_json::to_string(&envelope.payload.target).unwrap(),
            "$.payload.target",
        ),
    ] {
        assert_eq!(text.matches(&inventory).count(), 1);
        for (member, replacement, suffix) in [
            (r#""complete":true"#, r#""complete":"true""#, ".complete"),
            (r#""product":null,"#, "", ""),
        ] {
            assert_eq!(inventory.matches(member).count(), 1);
            let changed = text.replacen(&inventory, &inventory.replacen(member, replacement, 1), 1);
            assert_ne!(changed, text);
            let error = parse_evidence(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, format!("{path}{suffix}"));
            assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
        }
    }
}
