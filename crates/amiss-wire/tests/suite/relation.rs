#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid relation identities and inspect exact refusals"
)]

use sha2::Digest as _;
use std::{fs, path::Path};

use amiss_wire::controls::{
    BlobLineSelection, BlobSelection, KeyFormat, KeyValueSelection, NamedRegionSelection,
    ProjectionKind, ProjectionSource, RecordSetSelection, RecordValueSelection, TreePathSelection,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::envelope::Payload as _;
use amiss_wire::model::ObjectFormat;
use amiss_wire::relation::{
    EVIDENCE_PAYLOAD_SCHEMA, PLAN_PAYLOAD_SCHEMA, RELATION_DOCUMENT_BYTES, RelationEvidence,
    RelationPlan, RelationProjectionSlot,
};
use amiss_wire::repo_path_text;

use crate::relation_fixture::{digest, identity, oid, projected, relation_contract};

mod assessment;

#[test]
fn relation_plan_round_trips_all_four_exact_snapshots_and_example() {
    let expected = relation_contract().plan;
    let bytes = expected.emit().unwrap();
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
    let parsed = RelationPlan::parse(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(parsed.validate(), Ok(()));
    assert_eq!(
        parsed.payload_digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(
                    serde_json_canonicalizer::to_vec(value.get("payload").unwrap()).unwrap()
                )
                .finalize()
                .0
        )
    );
    assert_eq!(
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        )
        .unwrap(),
        bytes
    );

    let example_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-plan.json"),
    )
    .unwrap();
    let example = RelationPlan::parse(&example_bytes).unwrap();
    assert_eq!(
        example.payload.emit().unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&example_bytes).unwrap()
        )
        .unwrap()
    );
}

#[test]
fn relation_plan_requires_two_sorted_distinct_subjects_and_a_known_trigger() {
    let mut unsorted = relation_contract().plan;
    unsorted.subjects.reverse();
    let error = unsorted.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated_role = relation_contract().plan;
    repeated_role.subjects[1].role = repeated_role.subjects[0].role.clone();
    let error = repeated_role.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut repeated_repository = relation_contract().plan;
    repeated_repository.subjects[1].repository = repeated_repository.subjects[0].repository.clone();
    let error = repeated_repository.emit().unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);

    let mut foreign_trigger = relation_contract().plan;
    foreign_trigger.trigger_role = identity("release");
    let error = foreign_trigger.emit().unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn relation_plan_refuses_mixed_objects_and_incompatible_sources() {
    let mut mixed = relation_contract().plan;
    mixed.subjects[0].candidate.tree = oid('f', ObjectFormat::Sha256);
    let error = mixed.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].candidate.tree_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut incompatible = relation_contract().plan;
    incompatible.projection = ProjectionKind::CodeTextV1;
    let error = incompatible.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].source");
    assert_eq!(error.kind, ErrorKind::Inconsistent);

    // Two sides' digests say nothing about one containing the other.
    let mut contained = relation_contract().plan;
    contained.projection = ProjectionKind::ContainsV1;
    for subject in &mut contained.subjects {
        subject.source = ProjectionSource::Blob(BlobSelection {
            path: repo_path_text!("reference/api.md"),
        });
    }
    assert!(matches!(
        contained.emit(),
        Err(error) if error.path == "$.payload.projection" && error.kind == ErrorKind::InvalidValue
    ));
}

#[test]
fn relation_plan_preserves_every_projection_source_shape() {
    let path = repo_path_text!("reference/api.md");
    let cases = [
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::BlobLines(BlobLineSelection {
                path: path.clone(),
                first_line: 1,
                last_line: 4,
            }),
        ),
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::NamedRegion(NamedRegionSelection {
                path: path.clone(),
                start_marker: "API start".to_owned(),
                end_marker: "API end".to_owned(),
            }),
        ),
        (
            ProjectionKind::DecimalCountV1,
            ProjectionSource::TreePaths(TreePathSelection {
                root: path,
                suffix: Some(".md".to_owned()),
                maximum_depth: 3,
            }),
        ),
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::RecordValue(RecordValueSelection {
                set: identity("rust/public-api"),
                key: "item".to_owned(),
            }),
        ),
        (
            ProjectionKind::SortedRowsV1,
            ProjectionSource::RecordSet(RecordSetSelection {
                set: identity("rust/public-api"),
            }),
        ),
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::Blob(BlobSelection {
                path: repo_path_text!("reference/help.txt"),
            }),
        ),
        (
            ProjectionKind::CodeTextV1,
            ProjectionSource::KeyValue(KeyValueSelection {
                path: repo_path_text!("Cargo.toml"),
                format: KeyFormat::Toml,
                key: vec!["package".to_owned(), "version".to_owned()],
            }),
        ),
    ];

    for (projection, source) in cases {
        let mut input = relation_contract().plan;
        input.projection = projection;
        for subject in &mut input.subjects {
            subject.source = source.clone();
        }
        assert_eq!(
            RelationPlan::parse(&input.emit().unwrap()).unwrap().payload,
            input
        );
    }
}

#[test]
fn relation_plan_refuses_repository_values_that_bypass_construction() {
    let value = relation_contract().plan.emit().unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&value).unwrap();
    document["payload"]["subjects"][0]["repository"]["host"] = serde_json::json!("invalid/host");
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::json!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(&payload)
                .finalize()
                .0
        )
        .to_string()
    );

    let error =
        RelationPlan::parse(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].repository");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn relation_evidence_round_trips_four_independent_slots() {
    let expected = relation_contract().evidence;
    let bytes = expected.emit().unwrap();
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
    let parsed = RelationEvidence::parse(&bytes).unwrap();

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
}

#[test]
fn every_relation_projection_slot_can_remain_unproven_independently() {
    let mut partial = relation_contract().evidence;
    partial.subjects[0].base = RelationProjectionSlot::Unproven;
    partial.subjects[1].candidate = RelationProjectionSlot::Unproven;
    partial.subjects[1].base = RelationProjectionSlot::Projected(projected('a', 0));

    let parsed = RelationEvidence::parse(&partial.emit().unwrap()).unwrap();
    assert_eq!(parsed.payload, partial);

    for subject in &mut partial.subjects {
        subject.base = RelationProjectionSlot::Unproven;
        subject.candidate = RelationProjectionSlot::Unproven;
    }
    assert_eq!(
        RelationEvidence::parse(&partial.emit().unwrap())
            .unwrap()
            .payload,
        partial
    );
}

#[test]
fn nullable_projection_slots_are_still_required_fields() {
    let bytes = relation_contract().evidence.emit().unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        document
            .pointer_mut("/payload/subjects/0")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("base")
            .is_some()
    );

    let error = RelationEvidence::parse(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::MissingField);
    assert_eq!(error.path, "$.payload.subjects[0].base");
}

#[test]
fn relation_evidence_refuses_role_and_value_shape_drift() {
    let mut unsorted = relation_contract().evidence;
    unsorted.subjects.reverse();
    let error = unsorted.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated = relation_contract().evidence;
    repeated.subjects[1].role = repeated.subjects[0].role.clone();
    let error = repeated.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut unsafe_bytes = relation_contract().evidence;
    unsafe_bytes.subjects[0].base = RelationProjectionSlot::Projected(projected('a', u64::MAX));
    let error = unsafe_bytes.emit().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].base.value_bytes");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn relation_documents_refuse_tampering_open_shapes_and_oversized_input() {
    struct Document {
        bytes: Vec<u8>,
        payload_schema: &'static str,
        first_payload_field: &'static str,
        parse: fn(&[u8]) -> Result<(), amiss_wire::de::Error>,
        open_error: (&'static str, ErrorKind),
    }

    let documents = [
        Document {
            bytes: relation_contract().plan.emit().unwrap(),
            payload_schema: PLAN_PAYLOAD_SCHEMA,
            first_payload_field: "report_payload_digest",
            parse: |bytes| RelationPlan::parse(bytes).map(|_envelope| ()),
            open_error: ("$.payload.unknown", ErrorKind::UnknownField),
        },
        Document {
            bytes: relation_contract().evidence.emit().unwrap(),
            payload_schema: EVIDENCE_PAYLOAD_SCHEMA,
            first_payload_field: "plan_payload_digest",
            parse: |bytes| RelationEvidence::parse(bytes).map(|_envelope| ()),
            open_error: ("$.payload.unknown", ErrorKind::UnknownField),
        },
    ];

    for Document {
        bytes,
        payload_schema,
        first_payload_field,
        parse,
        open_error,
    } in documents
    {
        let value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        let recorded = value
            .get("payload_digest")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        let tampered = String::from_utf8(bytes.clone())
            .unwrap()
            .replace(recorded, &digest('f').to_string());
        let error = parse(tampered.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.payload_digest");
        assert_eq!(error.kind, ErrorKind::DigestMismatch);

        let open = String::from_utf8(bytes.clone()).unwrap().replacen(
            &format!("\"{first_payload_field}\":"),
            &format!("\"unknown\":true,\"{first_payload_field}\":"),
            1,
        );
        let open_value = serde_json::from_slice::<serde_json::Value>(open.as_bytes()).unwrap();
        let rebound = open.replace(
            recorded,
            &amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(payload_schema)
                    .chain_update([0_u8])
                    .chain_update(
                        serde_json_canonicalizer::to_vec(open_value.get("payload").unwrap())
                            .unwrap(),
                    )
                    .finalize()
                    .0,
            )
            .to_string(),
        );
        let error = parse(rebound.as_bytes()).unwrap_err();
        assert_eq!((error.path.as_str(), error.kind), open_error);

        let oversized = vec![b' '; usize::try_from(RELATION_DOCUMENT_BYTES).unwrap() + 1];
        let error = parse(&oversized).unwrap_err();
        assert_eq!(error.path, "$");
        assert_eq!(error.kind, ErrorKind::LimitExceeded);
    }
}

#[test]
fn projection_slots_must_match_the_canonical_typed_payload_digest() {
    let mut document = serde_json::to_value(
        RelationEvidence::parse(&relation_contract().evidence.emit().unwrap()).unwrap(),
    )
    .unwrap();
    let projected = &document["payload"]["subjects"][0]["base"];
    let array = serde_json::json!([projected["value_digest"], projected["value_bytes"]]);
    document["payload"]["subjects"][0]["base"] = array;
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::json!(amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize()
            .0
    ));
    let error = RelationEvidence::parse(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
    assert_eq!(error.path, "$.payload_digest");
}

#[test]
fn typed_plan_validation_preserves_binding_and_domain_checks() {
    let original = RelationPlan::parse(&relation_contract().plan.emit().unwrap()).unwrap();
    let mut tampered = original.clone();
    tampered.payload_digest = digest('f');
    let error = tampered.validate().unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
    let mut invalid = original;
    invalid.payload.subjects.reverse();
    let error = invalid.validate().unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);
}
