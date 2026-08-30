#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid relation identities and inspect exact refusals"
)]

use std::{fs, path::Path};

use amiss_wire::controls::{
    BlobLineSelection, ProjectionKind, ProjectionSource, RecordSetSelection, TreePathSelection,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json;
use amiss_wire::model::{
    ArtifactId, BranchRef, ObjectFormat, Oid, RepoPathText, RepositoryIdentity,
};
use amiss_wire::relation::{
    EVIDENCE_PAYLOAD_SCHEMA, PLAN_PAYLOAD_SCHEMA, RELATION_DOCUMENT_BYTES, RelationEvidence,
    RelationEvidenceSubject, RelationIdentity, RelationPlan, RelationProjectedValue,
    RelationSnapshot, RelationSubject, evidence, parse_evidence, parse_plan, plan,
};

mod assessment;

fn digest(digit: char) -> Digest {
    Digest::from_wire(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

fn identity(value: &str) -> ArtifactId {
    ArtifactId::new(value.to_owned()).unwrap()
}

fn oid(digit: char, object_format: ObjectFormat) -> Oid {
    let width = match object_format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    Oid::new(object_format, digit.to_string().repeat(width)).unwrap()
}

fn subject(
    role: &str,
    repository: &str,
    set: &str,
    object_format: ObjectFormat,
    digits: [char; 4],
) -> RelationSubject {
    RelationSubject {
        role: identity(role),
        repository: RepositoryIdentity::github("acme".to_owned(), repository.to_owned()).unwrap(),
        target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        source: ProjectionSource::RecordSet(RecordSetSelection { set: identity(set) }),
        base: RelationSnapshot {
            commit: oid(digits[0], object_format),
            tree: oid(digits[1], object_format),
        },
        candidate: RelationSnapshot {
            commit: oid(digits[2], object_format),
            tree: oid(digits[3], object_format),
        },
    }
}

fn relation_plan() -> RelationPlan {
    RelationPlan {
        report_payload_digest: digest('1'),
        relation: RelationIdentity {
            identity: identity("relation/public-api"),
            context_digest: digest('2'),
        },
        coordination: identity("workflow/release-42"),
        trigger_role: identity("source"),
        projection: ProjectionKind::SortedRowsV1,
        subjects: [
            subject(
                "documentation",
                "handbook",
                "docs/public-api",
                ObjectFormat::Sha1,
                ['a', 'b', 'a', 'b'],
            ),
            subject(
                "source",
                "service",
                "rust/public-api",
                ObjectFormat::Sha256,
                ['c', 'd', 'e', 'f'],
            ),
        ],
    }
}

fn projected(digit: char, value_bytes: u64) -> RelationProjectedValue {
    RelationProjectedValue {
        value_digest: digest(digit),
        value_bytes,
    }
}

fn relation_evidence() -> RelationEvidence {
    RelationEvidence {
        plan_payload_digest: digest('9'),
        subjects: [
            RelationEvidenceSubject {
                role: identity("documentation"),
                base: Some(projected('a', 1_024)),
                candidate: Some(projected('a', 1_024)),
            },
            RelationEvidenceSubject {
                role: identity("source"),
                base: Some(projected('a', 1_024)),
                candidate: Some(projected('b', 1_031)),
            },
        ],
    }
}

#[test]
fn relation_plan_round_trips_all_four_exact_snapshots_and_example() {
    let expected = relation_plan();
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-plan.json"),
    )
    .unwrap();
    let example = parse_plan(&example_bytes).unwrap();
    assert_eq!(
        json::canonical(&plan(&example.payload).unwrap()),
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn relation_plan_requires_two_sorted_distinct_subjects_and_a_known_trigger() {
    let mut unsorted = relation_plan();
    unsorted.subjects.reverse();
    let error = plan(&unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated_role = relation_plan();
    repeated_role.subjects[1].role = repeated_role.subjects[0].role.clone();
    let error = plan(&repeated_role).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut repeated_repository = relation_plan();
    repeated_repository.subjects[1].repository = repeated_repository.subjects[0].repository.clone();
    let error = plan(&repeated_repository).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);

    let mut foreign_trigger = relation_plan();
    foreign_trigger.trigger_role = identity("release");
    let error = plan(&foreign_trigger).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn relation_plan_refuses_mixed_objects_and_incompatible_sources() {
    let mut mixed = relation_plan();
    mixed.subjects[0].candidate.tree = oid('f', ObjectFormat::Sha256);
    let error = plan(&mixed).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].candidate.tree_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut incompatible = relation_plan();
    incompatible.projection = ProjectionKind::CodeTextV1;
    let error = plan(&incompatible).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].source");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn relation_plan_reuses_code_and_inventory_projection_sources() {
    let mut code = relation_plan();
    code.projection = ProjectionKind::CodeTextV1;
    for subject in &mut code.subjects {
        subject.source = ProjectionSource::BlobLines(BlobLineSelection {
            path: RepoPathText::new("api.txt".to_owned()).unwrap(),
            first_line: 1,
            last_line: 4,
        });
    }
    assert_eq!(
        parse_plan(&json::canonical(&plan(&code).unwrap()))
            .unwrap()
            .payload,
        code
    );

    let mut count = relation_plan();
    count.projection = ProjectionKind::DecimalCountV1;
    for subject in &mut count.subjects {
        subject.source = ProjectionSource::TreePaths(TreePathSelection {
            root: RepoPathText::new("reference".to_owned()).unwrap(),
            suffix: Some(".md".to_owned()),
            maximum_depth: 3,
        });
    }
    assert_eq!(
        parse_plan(&json::canonical(&plan(&count).unwrap()))
            .unwrap()
            .payload,
        count
    );
}

#[test]
fn relation_evidence_round_trips_four_independent_slots_and_example() {
    let expected = relation_evidence();
    let value = evidence(&expected).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(EVIDENCE_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );

    let example_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-evidence.json"),
    )
    .unwrap();
    let example = parse_evidence(&example_bytes).unwrap();
    assert_eq!(
        json::canonical(&evidence(&example.payload).unwrap()),
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn every_relation_projection_slot_can_remain_unproven_independently() {
    let mut partial = relation_evidence();
    partial.subjects[0].base = None;
    partial.subjects[1].candidate = None;
    partial.subjects[1].base = Some(projected('a', 0));

    let parsed = parse_evidence(&json::canonical(&evidence(&partial).unwrap())).unwrap();
    assert_eq!(parsed.payload, partial);

    for subject in &mut partial.subjects {
        subject.base = None;
        subject.candidate = None;
    }
    assert_eq!(
        parse_evidence(&json::canonical(&evidence(&partial).unwrap()))
            .unwrap()
            .payload,
        partial
    );
}

#[test]
fn relation_evidence_refuses_role_and_value_shape_drift() {
    let mut unsorted = relation_evidence();
    unsorted.subjects.reverse();
    let error = evidence(&unsorted).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);

    let mut repeated = relation_evidence();
    repeated.subjects[1].role = repeated.subjects[0].role.clone();
    let error = evidence(&repeated).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects");
    assert_eq!(error.kind, ErrorKind::DuplicateMember);

    let mut unsafe_bytes = relation_evidence();
    unsafe_bytes.subjects[0].base = Some(projected('a', u64::MAX));
    let error = evidence(&unsafe_bytes).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].base.value_bytes");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn relation_documents_refuse_tampering_open_shapes_and_oversized_input() {
    struct Document {
        value: json::Value,
        payload_schema: &'static str,
        first_payload_field: &'static str,
        parse: fn(&[u8]) -> Result<(), amiss_wire::de::Error>,
    }

    let documents = [
        Document {
            value: plan(&relation_plan()).unwrap(),
            payload_schema: PLAN_PAYLOAD_SCHEMA,
            first_payload_field: "report_payload_digest",
            parse: |bytes| parse_plan(bytes).map(|_envelope| ()),
        },
        Document {
            value: evidence(&relation_evidence()).unwrap(),
            payload_schema: EVIDENCE_PAYLOAD_SCHEMA,
            first_payload_field: "plan_payload_digest",
            parse: |bytes| parse_evidence(bytes).map(|_envelope| ()),
        },
    ];

    for Document {
        value,
        payload_schema,
        first_payload_field,
        parse,
    } in documents
    {
        let recorded = value.text("payload_digest").unwrap();
        let tampered = String::from_utf8(json::canonical(&value))
            .unwrap()
            .replace(recorded, &digest('f').to_string());
        let error = parse(tampered.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.payload_digest");
        assert_eq!(error.kind, ErrorKind::DigestMismatch);

        let open = String::from_utf8(json::canonical(&value))
            .unwrap()
            .replacen(
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
        assert_eq!(error.path, "$.payload.unknown");
        assert_eq!(error.kind, ErrorKind::UnknownField);

        let oversized = vec![b' '; usize::try_from(RELATION_DOCUMENT_BYTES).unwrap() + 1];
        let error = parse(&oversized).unwrap_err();
        assert_eq!(error.path, "$");
        assert_eq!(error.kind, ErrorKind::LimitExceeded);
    }
}
