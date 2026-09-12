use amiss_wire::controls::{
    BlobLineSelection, NamedRegionSelection, ProjectionKind, ProjectionSource, RecordSetSelection,
    RecordValueSelection, TreePathSelection,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ObjectFormat, RepoPathText};
use amiss_wire::relation::{
    EVIDENCE_PAYLOAD_SCHEMA, PLAN_PAYLOAD_SCHEMA, RELATION_DOCUMENT_BYTES, RelationProjectionSlot,
    evidence, parse_evidence, parse_plan, plan,
};

use crate::relation_fixture::{digest, identity, oid, projected, relation_contract};

mod assessment;

#[test]
fn relation_plan_round_trips_all_four_exact_snapshots_and_example() {
    let expected = relation_contract().plan;
    let envelope = plan(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    let parsed = parse_plan(&bytes).unwrap();

    assert_eq!(parsed, envelope);
    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hb(
            PLAN_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );
    assert_eq!(serde_json_canonicalizer::to_vec(&parsed).unwrap(), bytes);

    let example_bytes = include_bytes!("../../../../spec/examples/relation-plan.json");
    let example = parse_plan(example_bytes).unwrap();
    assert_eq!(plan(example.payload.clone()).unwrap(), example);
    let mut source = serde_json::Deserializer::from_slice(example_bytes);
    assert_eq!(
        serde_json_canonicalizer::to_vec(&example).unwrap(),
        serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(&mut source)).unwrap()
    );
    source.end().unwrap();
}

#[test]
fn relation_plan_requires_two_sorted_distinct_subjects_and_a_known_trigger() {
    let mut unsorted = relation_contract().plan;
    unsorted.subjects.reverse();
    let error = plan(unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert!(matches!(error.kind, ErrorKind::UnsortedSet));

    let mut repeated_role = relation_contract().plan;
    repeated_role.subjects[1].role = repeated_role.subjects[0].role.clone();
    let error = plan(repeated_role).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert!(matches!(error.kind, ErrorKind::DuplicateMember));

    let mut repeated_repository = relation_contract().plan;
    repeated_repository.subjects[1].repository = repeated_repository.subjects[0].repository.clone();
    let error = plan(repeated_repository).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert!(matches!(error.kind, ErrorKind::Inconsistent));

    let mut foreign_trigger = relation_contract().plan;
    foreign_trigger.trigger_role = identity("release");
    let error = plan(foreign_trigger).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert!(matches!(error.kind, ErrorKind::Inconsistent));
}

#[test]
fn relation_plan_refuses_mixed_objects_and_incompatible_sources() {
    let mut mixed = relation_contract().plan;
    mixed.subjects[0].candidate.tree = oid('f', ObjectFormat::Sha256);
    let error = plan(mixed).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].candidate.tree_oid");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));

    let mut incompatible = relation_contract().plan;
    incompatible.projection = ProjectionKind::CodeTextV1;
    let error = plan(incompatible).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].source");
    assert!(matches!(error.kind, ErrorKind::Inconsistent));
}

#[test]
fn relation_plan_preserves_every_projection_source_shape() {
    let path = RepoPathText::new("reference/api.md".to_owned()).unwrap();
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
    ];

    for (projection, source) in cases {
        let mut input = relation_contract().plan;
        input.projection = projection;
        for subject in &mut input.subjects {
            subject.source = source.clone();
        }
        let envelope = plan(input.clone()).unwrap();
        let mut bytes = Vec::new();
        amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
        assert_eq!(parse_plan(&bytes).unwrap(), envelope);
        assert_eq!(envelope.payload, input);
    }
}

#[test]
fn relation_plan_refuses_repository_values_that_bypass_construction() {
    let mut document = plan(relation_contract().plan).unwrap();
    document.payload.subjects[0].repository =
        serde_json::from_str(r#"{"host":"invalid/host","owner":"acme","name":"handbook"}"#)
            .unwrap();
    document.payload_digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
    );

    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].repository");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}

#[test]
fn relation_evidence_round_trips_four_independent_slots() {
    let expected = relation_contract().evidence;
    let envelope = evidence(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed, envelope);
    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hb(
            EVIDENCE_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );
}

#[test]
fn every_relation_projection_slot_can_remain_unproven_independently() {
    let mut partial = relation_contract().evidence;
    partial.subjects[0].base = RelationProjectionSlot::Unproven;
    partial.subjects[1].candidate = RelationProjectionSlot::Unproven;
    partial.subjects[1].base = RelationProjectionSlot::Projected(projected('a', 0));

    let envelope = evidence(partial.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(parse_evidence(&bytes).unwrap(), envelope);
    assert_eq!(envelope.payload, partial);

    for subject in &mut partial.subjects {
        subject.base = RelationProjectionSlot::Unproven;
        subject.candidate = RelationProjectionSlot::Unproven;
    }
    let envelope = evidence(partial.clone()).unwrap();
    bytes.clear();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(parse_evidence(&bytes).unwrap(), envelope);
    assert_eq!(envelope.payload, partial);
}

#[test]
fn relation_evidence_refuses_role_and_value_shape_drift() {
    let mut unsorted = relation_contract().evidence;
    unsorted.subjects.reverse();
    let error = evidence(unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert!(matches!(error.kind, ErrorKind::UnsortedSet));

    let mut repeated = relation_contract().evidence;
    repeated.subjects[1].role = repeated.subjects[0].role.clone();
    let error = evidence(repeated).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert!(matches!(error.kind, ErrorKind::DuplicateMember));

    let mut unsafe_bytes = relation_contract().evidence;
    unsafe_bytes.subjects[0].base = RelationProjectionSlot::Projected(projected('a', u64::MAX));
    let error = evidence(unsafe_bytes).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].base.value_bytes");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}

#[test]
fn relation_documents_refuse_tampering_open_shapes_and_oversized_input() {
    struct Document {
        bytes: &'static [u8],
        payload_digest: Digest,
        first_payload_field: &'static str,
        parse: fn(&[u8]) -> Result<(), amiss_wire::de::Error>,
    }

    let plan_bytes = include_bytes!("../../../../spec/examples/relation-plan.json");
    let evidence_bytes = include_bytes!("../../../../spec/examples/relation-evidence.json");
    let documents = [
        Document {
            bytes: plan_bytes,
            payload_digest: parse_plan(plan_bytes).unwrap().payload_digest,
            first_payload_field: "report_payload_digest",
            parse: |bytes| parse_plan(bytes).map(|_envelope| ()),
        },
        Document {
            bytes: evidence_bytes,
            payload_digest: parse_evidence(evidence_bytes).unwrap().payload_digest,
            first_payload_field: "plan_payload_digest",
            parse: |bytes| parse_evidence(bytes).map(|_envelope| ()),
        },
    ];

    for Document {
        bytes,
        payload_digest,
        first_payload_field,
        parse,
    } in documents
    {
        let text = std::str::from_utf8(bytes).unwrap();
        let tampered = text.replace(&payload_digest.to_string(), &digest('f').to_string());
        assert_ne!(tampered, text);
        let error = parse(tampered.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.payload_digest");
        assert!(matches!(error.kind, ErrorKind::DigestMismatch));

        let open = text.replacen(
            &format!("\"{first_payload_field}\":"),
            &format!("\"unknown\":true,\"{first_payload_field}\":"),
            1,
        );
        assert_ne!(open, text);
        let error = parse(open.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.payload.unknown");
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));

        let oversized = vec![b' '; usize::try_from(RELATION_DOCUMENT_BYTES).unwrap() + 1];
        let error = parse(&oversized).unwrap_err();
        assert_eq!(error.path, "$");
        assert!(matches!(error.kind, ErrorKind::LimitExceeded));
    }
}
