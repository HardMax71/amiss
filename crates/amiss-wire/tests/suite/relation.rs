#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid relation identities and inspect exact refusals"
)]

use std::{fs, path::Path};

use amiss_wire::controls::{
    BlobLineSelection, NamedRegionSelection, ProjectionKind, ProjectionSource, RecordSetSelection,
    RecordValueSelection, TreePathSelection,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json;
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
    let bytes = plan(&expected).unwrap();
    let value = json::parse(&bytes).unwrap();
    let parsed = parse_plan(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(PLAN_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    let example_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-plan.json"),
    )
    .unwrap();
    let example = parse_plan(&example_bytes).unwrap();
    assert_eq!(
        plan(&example.payload).unwrap(),
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn relation_plan_requires_two_sorted_distinct_subjects_and_a_known_trigger() {
    let mut unsorted = relation_contract().plan;
    unsorted.subjects.reverse();
    let error = plan(&unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated_role = relation_contract().plan;
    repeated_role.subjects[1].role = repeated_role.subjects[0].role.clone();
    let error = plan(&repeated_role).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut repeated_repository = relation_contract().plan;
    repeated_repository.subjects[1].repository = repeated_repository.subjects[0].repository.clone();
    let error = plan(&repeated_repository).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);

    let mut foreign_trigger = relation_contract().plan;
    foreign_trigger.trigger_role = identity("release");
    let error = plan(&foreign_trigger).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn relation_plan_refuses_mixed_objects_and_incompatible_sources() {
    let mut mixed = relation_contract().plan;
    mixed.subjects[0].candidate.tree = oid('f', ObjectFormat::Sha256);
    let error = plan(&mixed).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].candidate.tree_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut incompatible = relation_contract().plan;
    incompatible.projection = ProjectionKind::CodeTextV1;
    let error = plan(&incompatible).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].source");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
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
        assert_eq!(parse_plan(&plan(&input).unwrap()).unwrap().payload, input);
    }
}

#[test]
fn relation_plan_refuses_repository_values_that_bypass_construction() {
    let value = plan(&relation_contract().plan).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&value).unwrap();
    document["payload"]["subjects"][0]["repository"]["host"] = serde_json::json!("invalid/host");
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::json!(hb(PLAN_PAYLOAD_SCHEMA, &payload).to_string());

    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].repository");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn relation_evidence_round_trips_four_independent_slots() {
    let expected = relation_contract().evidence;
    let bytes = evidence(&expected).unwrap();
    let value = json::parse(&bytes).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(EVIDENCE_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
}

#[test]
fn every_relation_projection_slot_can_remain_unproven_independently() {
    let mut partial = relation_contract().evidence;
    partial.subjects[0].base = RelationProjectionSlot::Unproven;
    partial.subjects[1].candidate = RelationProjectionSlot::Unproven;
    partial.subjects[1].base = RelationProjectionSlot::Projected(projected('a', 0));

    let parsed = parse_evidence(&evidence(&partial).unwrap()).unwrap();
    assert_eq!(parsed.payload, partial);

    for subject in &mut partial.subjects {
        subject.base = RelationProjectionSlot::Unproven;
        subject.candidate = RelationProjectionSlot::Unproven;
    }
    assert_eq!(
        parse_evidence(&evidence(&partial).unwrap())
            .unwrap()
            .payload,
        partial
    );
}

#[test]
fn nullable_projection_slots_are_still_required_fields() {
    let bytes = evidence(&relation_contract().evidence).unwrap();
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

    let error = parse_evidence(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn relation_evidence_refuses_role_and_value_shape_drift() {
    let mut unsorted = relation_contract().evidence;
    unsorted.subjects.reverse();
    let error = evidence(&unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated = relation_contract().evidence;
    repeated.subjects[1].role = repeated.subjects[0].role.clone();
    let error = evidence(&repeated).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut unsafe_bytes = relation_contract().evidence;
    unsafe_bytes.subjects[0].base = RelationProjectionSlot::Projected(projected('a', u64::MAX));
    let error = evidence(&unsafe_bytes).unwrap_err();
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
            bytes: plan(&relation_contract().plan).unwrap(),
            payload_schema: PLAN_PAYLOAD_SCHEMA,
            first_payload_field: "report_payload_digest",
            parse: |bytes| parse_plan(bytes).map(|_envelope| ()),
            open_error: ("$", ErrorKind::InvalidValue),
        },
        Document {
            bytes: evidence(&relation_contract().evidence).unwrap(),
            payload_schema: EVIDENCE_PAYLOAD_SCHEMA,
            first_payload_field: "plan_payload_digest",
            parse: |bytes| parse_evidence(bytes).map(|_envelope| ()),
            open_error: ("$", ErrorKind::InvalidValue),
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
        let value = json::parse(&bytes).unwrap();
        let recorded = value.text("payload_digest").unwrap();
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
        let open_value = json::parse(open.as_bytes()).unwrap();
        let rebound = open.replace(
            recorded,
            &hj(payload_schema, open_value.member("payload").unwrap()).to_string(),
        );
        let error = parse(rebound.as_bytes()).unwrap_err();
        assert_eq!((error.path.as_str(), error.kind), open_error);

        let oversized = vec![b' '; usize::try_from(RELATION_DOCUMENT_BYTES).unwrap() + 1];
        let error = parse(&oversized).unwrap_err();
        assert_eq!(error.path, "$");
        assert_eq!(error.kind, ErrorKind::LimitExceeded);
    }
}
