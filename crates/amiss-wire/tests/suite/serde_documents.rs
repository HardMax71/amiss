#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests mutate known-valid published document shapes"
)]

use amiss_wire::codec::{self, Document, Envelope};
use amiss_wire::de::Error;
use amiss_wire::{locale, publication, relation};
use garde::Validate;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

struct Example {
    bytes: &'static [u8],
    parse: fn(&[u8]) -> Result<(), Error>,
}

fn parse<T: Document + Serialize + DeserializeOwned + Validate<Context = ()>>(
    bytes: &[u8],
) -> Result<(), Error> {
    Envelope::<T>::parse(bytes).map(|_envelope| ())
}

fn examples() -> [Example; 9] {
    [
        Example {
            bytes: include_bytes!("../../../../spec/examples/relation-plan.json"),
            parse: parse::<relation::RelationPlan>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/relation-evidence.json"),
            parse: parse::<relation::RelationEvidence>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/relation-assessment.json"),
            parse: parse::<relation::RelationAssessment>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/publication-plan.json"),
            parse: parse::<publication::PublicationPlan>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/publication-evidence.json"),
            parse: parse::<publication::PublicationEvidence>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/publication-assessment.json"),
            parse: parse::<publication::PublicationAssessment>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/locale-coverage-plan.json"),
            parse: parse::<locale::LocaleCoveragePlan>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/locale-coverage-evidence.json"),
            parse: parse::<locale::LocaleCoverageEvidence>,
        },
        Example {
            bytes: include_bytes!("../../../../spec/examples/locale-coverage-assessment.json"),
            parse: parse::<locale::LocaleCoverageAssessment>,
        },
    ]
}

fn rebound(document: &mut Value) -> Vec<u8> {
    let domain = document["payload"]["schema"].as_str().unwrap();
    let digest = codec::digest(domain, &document["payload"]).unwrap();
    document["payload_digest"] = Value::String(digest.to_string());
    codec::canonical(document).unwrap()
}

#[test]
fn every_document_requires_its_envelope_and_payload_members() {
    for example in examples() {
        (example.parse)(example.bytes).unwrap();
        let document: Value = serde_json::from_slice(example.bytes).unwrap();
        for field in ["schema", "payload", "payload_digest"] {
            let mut missing = document.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                (example.parse)(&codec::canonical(&missing).unwrap()).is_err(),
                "{field}"
            );
        }
        for field in document["payload"].as_object().unwrap().keys() {
            let mut missing = document.clone();
            missing["payload"].as_object_mut().unwrap().remove(field);
            let bytes = if field == "schema" {
                codec::canonical(&missing).unwrap()
            } else {
                rebound(&mut missing)
            };
            assert!((example.parse)(&bytes).is_err(), "payload.{field}");
        }
        let mut open = document;
        open["payload"]["unknown"] = Value::Null;
        assert!((example.parse)(&rebound(&mut open)).is_err());
    }
}

#[test]
fn nullable_bindings_and_locale_products_are_required_members() {
    for example in examples() {
        let document: Value = serde_json::from_slice(example.bytes).unwrap();
        for pointer in [
            "/payload/subject/evidence_payload_digest",
            "/payload/product",
            "/payload/scope/version",
            "/payload/source/product",
            "/payload/target/product",
            "/payload/target/pages/0/origin/based_on_source_digest",
            "/payload/subjects/0/base",
            "/payload/subjects/0/candidate",
            "/payload/reason",
        ] {
            if document.pointer(pointer).is_none() {
                continue;
            }
            let (parent, field) = pointer.rsplit_once('/').unwrap();
            let mut missing = document.clone();
            missing
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                (example.parse)(&rebound(&mut missing)).is_err(),
                "{pointer}"
            );
        }
    }
}

#[test]
fn unit_variants_refuse_fields_belonging_to_other_variants() {
    let mut document: Value = serde_json::from_slice(examples()[6].bytes).unwrap();
    document["payload"]["policy"]["required"] =
        serde_json::json!({"mode":"all-source", "keys":null});
    assert!(locale::parse_plan(&rebound(&mut document)).is_err());
    document["payload"]["policy"]["required"] = serde_json::json!({"mode":"all-source", "keys":[]});
    assert!(locale::parse_plan(&rebound(&mut document)).is_err());
}

#[test]
fn locale_evidence_admits_the_page_ceiling_and_refuses_one_more_page() {
    let mut evidence = Envelope::<locale::LocaleCoverageEvidence>::parse(examples()[7].bytes)
        .unwrap()
        .payload;
    let digest = amiss_wire::digest::sha256(b"page resource");
    evidence.source.pages = (0..locale::PAGE_ITEMS_LIMIT)
        .map(|index| (format!("page-{index:06}"), digest))
        .collect();
    evidence.target.pages.clear();
    let envelope = Envelope::seal(evidence).unwrap();
    let bytes = codec::canonical(&envelope).unwrap();
    let mut parsed = locale::parse_evidence(&bytes).unwrap();
    assert_eq!(parsed.payload.source.pages.len(), locale::PAGE_ITEMS_LIMIT);
    parsed.payload.target.pages.insert(
        "additional-page".to_owned(),
        locale::LocaleTargetPage {
            resource_digest: digest,
            origin: locale::LocaleTargetOrigin::TargetResource {
                based_on_source_digest: None,
            },
        },
    );
    assert_eq!(
        Envelope::seal(parsed.payload).unwrap_err().kind,
        amiss_wire::de::ErrorKind::LimitExceeded
    );
    let mut excessive: Value = serde_json::from_slice(&bytes).unwrap();
    excessive["payload"]["source"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"key":"z-final-page", "resource_digest": digest}));
    assert_eq!(
        locale::parse_evidence(&rebound(&mut excessive))
            .unwrap_err()
            .kind,
        amiss_wire::de::ErrorKind::LimitExceeded
    );
}

#[test]
fn locale_assessment_admits_the_combined_ceiling_and_refuses_one_more_result() {
    let mut assessment = Envelope::<locale::LocaleCoverageAssessment>::parse(examples()[8].bytes)
        .unwrap()
        .payload;
    let keys: Vec<String> = (0..locale::PAGE_ITEMS_LIMIT)
        .map(|index| format!("page-{index:06}"))
        .collect();
    assessment.verdict = locale::LocaleCoverageVerdict::Refuted;
    assessment.reasons = vec![
        locale::LocaleCoverageReason::SourceMissing,
        locale::LocaleCoverageReason::TargetMissing,
    ];
    assessment.product = None;
    assessment.coverage = locale::LocaleCoverageResult {
        complete: true,
        source_missing: keys.clone(),
        target_missing: keys,
        target_orphaned: Vec::new(),
        fallbacks: Vec::new(),
        lineage: Vec::new(),
    };
    let envelope = Envelope::seal(assessment).unwrap();
    let bytes = codec::canonical(&envelope).unwrap();
    let mut parsed = locale::parse_assessment(&bytes).unwrap();
    assert_eq!(
        parsed.payload.coverage.source_missing.len() + parsed.payload.coverage.target_missing.len(),
        locale::ASSESSMENT_PAGE_ITEMS_LIMIT
    );
    parsed
        .payload
        .coverage
        .lineage
        .push(locale::LocaleLineageResult {
            key: "additional-page".to_owned(),
            status: locale::LocaleLineageStatus::Current,
        });
    assert_eq!(
        Envelope::seal(parsed.payload).unwrap_err().kind,
        amiss_wire::de::ErrorKind::LimitExceeded
    );
}

#[test]
fn tagged_locale_records_refuse_positional_sequences() {
    for (pointer, replacement) in [
        (
            "/payload/policy/required",
            serde_json::json!(["all-source"]),
        ),
        (
            "/payload/policy/required",
            serde_json::json!(["named", ["guide/getting-started"]]),
        ),
        (
            "/payload/policy/fallbacks/0/pages",
            serde_json::json!(["all-source"]),
        ),
    ] {
        let mut plan: Value = serde_json::from_slice(examples()[6].bytes).unwrap();
        *plan.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            locale::parse_plan(&rebound(&mut plan)).is_err(),
            "{pointer}"
        );
    }
    for (index, replacement) in [
        (
            0,
            serde_json::json!(["target-resource", format!("sha256:{}", "6".repeat(64))]),
        ),
        (
            1,
            serde_json::json!([
                "fallback",
                "source-copy",
                format!("sha256:{}", "7".repeat(64))
            ]),
        ),
    ] {
        let mut evidence: Value = serde_json::from_slice(examples()[7].bytes).unwrap();
        evidence["payload"]["target"]["pages"][index]["origin"] = replacement;
        assert!(
            locale::parse_evidence(&rebound(&mut evidence)).is_err(),
            "page {index}"
        );
    }
}
