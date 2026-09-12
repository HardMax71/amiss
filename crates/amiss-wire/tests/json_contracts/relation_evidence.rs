use amiss_wire::{
    de::ErrorKind,
    digest::sha256,
    json::MAX_SAFE_INTEGER,
    relation::{
        self, RELATION_DOCUMENT_BYTES, RelationProjectedValue, RelationProjectionSlot,
        RelationVerdict,
    },
};

use super::input::assert_object_required;

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/relation-evidence.json");

#[test]
fn projected_counts_keep_the_safe_integer_contract_through_serde() {
    let mut value = RelationProjectedValue {
        value_digest: sha256(b"projected value"),
        value_bytes: 1,
    };
    let text = serde_json::to_string(&value).unwrap();
    let maximum = MAX_SAFE_INTEGER.unsigned_abs();
    for value_bytes in [0, 1, maximum] {
        value.value_bytes = value_bytes;
        let encoded = serde_json::to_vec(&value).unwrap();
        let slot = RelationProjectionSlot::Projected(value);
        assert_eq!(serde_json::to_vec(&slot).unwrap(), encoded);
        assert_eq!(
            serde_json::from_slice::<RelationProjectedValue>(&encoded).unwrap(),
            value
        );
        assert_eq!(
            serde_json::from_slice::<RelationProjectionSlot>(&encoded).unwrap(),
            slot
        );
    }
    for value_bytes in [maximum + 1, u64::MAX] {
        value.value_bytes = value_bytes;
        let invalid = text.replace(
            "\"value_bytes\":1",
            &format!("\"value_bytes\":{value_bytes}"),
        );
        assert_ne!(invalid, text);
        assert_eq!(
            [
                serde_json::from_str::<RelationProjectedValue>(&invalid).is_err(),
                serde_json::from_str::<RelationProjectionSlot>(&invalid).is_err(),
                serde_json::to_vec(&value).is_err(),
                serde_json::to_vec(&RelationProjectionSlot::Projected(value)).is_err(),
            ],
            [true; 4],
            "{value_bytes}"
        );
    }
    for invalid in ["-0", "-1", "1.0", "1e0", "null", "true", "\"1\""] {
        let changed = text.replace("\"value_bytes\":1", &format!("\"value_bytes\":{invalid}"));
        assert_ne!(changed, text);
        assert!(serde_json::from_str::<RelationProjectedValue>(&changed).is_err());
        assert!(serde_json::from_str::<RelationProjectionSlot>(&changed).is_err());
    }
}

#[test]
fn owned_evidence_preserves_roles_safe_counts_and_bounded_output() {
    let plan = relation::parse_plan(include_bytes!(
        "../../../../spec/examples/relation-plan.json"
    ))
    .unwrap();
    let mut input = relation::parse_evidence(EVIDENCE).unwrap().payload;
    let pointers = input
        .subjects
        .each_ref()
        .map(|subject| subject.role.as_str().as_ptr());
    let maximum = MAX_SAFE_INTEGER.unsigned_abs();
    let value = RelationProjectedValue {
        value_digest: sha256(b"projected value"),
        value_bytes: maximum,
    };
    for subject in &mut input.subjects {
        subject.base = RelationProjectionSlot::Projected(value);
        subject.candidate = RelationProjectionSlot::Projected(value);
    }
    let envelope = relation::evidence(input).unwrap();
    assert_eq!(
        pointers,
        envelope
            .payload
            .subjects
            .each_ref()
            .map(|subject| subject.role.as_str().as_ptr())
    );
    assert!(envelope.payload.subjects.iter().all(|subject| {
        [subject.base, subject.candidate] == [RelationProjectionSlot::Projected(value); 2]
    }));
    let assessment = relation::assess(&plan, Some(&envelope), "1", sha256(b"engine")).unwrap();
    assert_eq!(assessment.payload.verdict, RelationVerdict::Aligned);
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(relation::parse_evidence(&bytes).unwrap(), envelope);
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
    input.subjects[0].base = RelationProjectionSlot::Projected(RelationProjectedValue {
        value_bytes: 0,
        ..value
    });
    assert_eq!(relation::evidence(input.clone()).unwrap().payload, input);
    for value_bytes in [maximum + 1, u64::MAX] {
        input.subjects[0].base = RelationProjectionSlot::Projected(RelationProjectedValue {
            value_bytes,
            ..value
        });
        let error = relation::evidence(input.clone()).unwrap_err();
        assert_eq!(error.path, "$.payload.subjects[0].base.value_bytes");
        assert!(matches!(error.kind, ErrorKind::InvalidValue));
    }
}

#[test]
fn evidence_requires_object_shapes_and_both_schema_tags() -> Result<(), Box<dyn std::error::Error>>
{
    let document = relation::parse_evidence(EVIDENCE)?;
    let input = (&document, relation::parse_evidence);
    let payload = &document.payload;
    let subject = &payload.subjects[1];
    let RelationProjectionSlot::Projected(value) = &subject.candidate else {
        panic!("the committed example includes the changed projection");
    };
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
            payload.plan_payload_digest,
            &payload.subjects,
        ),
    )?;
    assert_object_required(
        input,
        subject,
        (&subject.role, subject.base, subject.candidate),
    )?;
    assert_object_required(input, value, (value.value_digest, value.value_bytes))?;
    let text = serde_json::to_string(&document)?;
    for (tag, path) in [
        (serde_json::to_string(&document.schema)?, "$.schema"),
        (serde_json::to_string(&payload.schema)?, "$.payload.schema"),
    ] {
        for invalid in ["null", "false", r#""unknown""#] {
            let changed = text.replacen(&tag, invalid, 1);
            assert_ne!(changed, text);
            let error = relation::parse_evidence(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
        }
        let missing = text.replacen(&format!("\"schema\":{tag},"), "", 1);
        assert_ne!(missing, text);
        let error = relation::parse_evidence(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path.rsplit_once('.').unwrap().0);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
    Ok(())
}

#[test]
fn all_nullable_projection_slots_remain_required_in_the_model_and_reader() {
    let mut input = relation::parse_evidence(EVIDENCE).unwrap().payload;
    for subject in &mut input.subjects {
        subject.base = RelationProjectionSlot::Unproven;
        subject.candidate = RelationProjectionSlot::Unproven;
    }
    let envelope = relation::evidence(input).unwrap();
    let text = String::from_utf8(serde_json_canonicalizer::to_vec(&envelope).unwrap()).unwrap();
    assert_eq!(relation::parse_evidence(text.as_bytes()).unwrap(), envelope);
    for (index, subject) in envelope.payload.subjects.iter().enumerate() {
        let object = String::from_utf8(serde_json_canonicalizer::to_vec(subject).unwrap()).unwrap();
        for field in ["base", "candidate"] {
            let missing = object.replacen(&format!("\"{field}\":null,"), "", 1);
            assert_ne!(missing, object);
            let changed = text.replacen(&object, &missing, 1);
            assert_ne!(changed, text);
            assert!(serde_json::from_str::<relation::RelationEvidenceEnvelope>(&changed).is_err());
            let error = relation::parse_evidence(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, format!("$.payload.subjects[{index}]"));
            assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
        }
    }
}
