use amiss_wire::{
    controls::{NamedRegionSelection, ProjectionKind, ProjectionSource, SOURCE_MARKER_BYTES},
    de::ErrorKind,
    relation::{self, RELATION_DOCUMENT_BYTES, RelationVerdict},
};

use super::input::assert_object_required;

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/relation-plan.json");

#[test]
fn owned_plan_moves_bounded_sources_and_writes_only_at_the_output_boundary() {
    let mut input = relation::parse_plan(PLAN).unwrap().payload;
    input.projection = ProjectionKind::CodeTextV1;
    let pointers = input.subjects.each_mut().map(|subject| {
        let selection = NamedRegionSelection {
            path: "reference/api.md".parse().unwrap(),
            start_marker: "a".repeat(SOURCE_MARKER_BYTES),
            end_marker: "b".repeat(SOURCE_MARKER_BYTES),
        };
        let pointers = [
            selection.start_marker.as_ptr(),
            selection.end_marker.as_ptr(),
        ];
        subject.source = ProjectionSource::NamedRegion(selection);
        pointers
    });
    let envelope = relation::plan(input).unwrap();
    assert_eq!(
        envelope.payload_digest,
        relation::plan_payload_digest(&envelope.payload).unwrap()
    );
    assert_eq!(
        pointers,
        envelope.payload.subjects.each_ref().map(|subject| {
            let ProjectionSource::NamedRegion(selection) = &subject.source else {
                panic!("the owned source keeps its selected variant");
            };
            [
                selection.start_marker.as_ptr(),
                selection.end_marker.as_ptr(),
            ]
        })
    );
    let assessment =
        relation::assess(&envelope, None, "1", amiss_wire::digest::sha256(b"engine")).unwrap();
    assert_eq!(assessment.payload.verdict, RelationVerdict::Unproven);
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(relation::parse_plan(&bytes).unwrap(), envelope);
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= RELATION_DOCUMENT_BYTES);
    amiss_wire::write_json(&envelope, std::io::sink(), exact).unwrap();
    assert!(matches!(
        amiss_wire::write_json(&envelope, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    ));

    let mut input = envelope.payload;
    let ProjectionSource::NamedRegion(selection) = &mut input.subjects[0].source else {
        panic!("the owned source keeps its selected variant");
    };
    selection.start_marker.push('a');
    let error = relation::plan_payload_digest(&input).unwrap_err();
    assert_eq!(error.path, "$.payload.subjects[0].source");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
    let parsed = relation::plan(input).unwrap_err();
    assert_eq!(parsed.path, error.path);
    assert_eq!(
        std::mem::discriminant(&parsed.kind),
        std::mem::discriminant(&error.kind)
    );
}

#[test]
fn complete_plan_requires_schema_tags_and_objects_at_every_nested_level()
-> Result<(), Box<dyn std::error::Error>> {
    let document = relation::parse_plan(PLAN)?;
    let input = (&document, relation::parse_plan);
    let payload = &document.payload;
    let relation = &payload.relation;
    let subject = &payload.subjects[1];
    let repository = &subject.repository;
    assert_object_required(
        input,
        &document,
        (document.schema, payload, document.payload_digest),
    )?;
    assert_object_required(
        input,
        payload,
        (
            payload.schema,
            payload.report_payload_digest,
            relation,
            &payload.coordination,
            &payload.trigger_role,
            payload.projection,
            &payload.subjects,
        ),
    )?;
    assert_object_required(
        input,
        relation,
        (&relation.identity, relation.context_digest),
    )?;
    assert_object_required(
        input,
        subject,
        (
            &subject.role,
            repository,
            &subject.target,
            subject.object_format,
            &subject.source,
            &subject.base,
            &subject.candidate,
        ),
    )?;
    assert_object_required(
        input,
        repository,
        (repository.host(), repository.name(), repository.owner()),
    )?;
    for snapshot in [&subject.base, &subject.candidate] {
        assert_object_required(input, snapshot, (&snapshot.commit, &snapshot.tree))?;
    }

    let text = serde_json::to_string(&document)?;
    for (tag, path) in [
        (serde_json::to_string(&document.schema)?, "$.schema"),
        (serde_json::to_string(&payload.schema)?, "$.payload.schema"),
    ] {
        for invalid in ["null", "false", r#""unknown""#] {
            let changed = text.replacen(&tag, invalid, 1);
            assert_ne!(changed, text);
            let error = relation::parse_plan(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
        }
        let missing = text.replacen(&format!("\"schema\":{tag},"), "", 1);
        assert_ne!(missing, text);
        let error = relation::parse_plan(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path.rsplit_once('.').unwrap().0);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
    Ok(())
}
