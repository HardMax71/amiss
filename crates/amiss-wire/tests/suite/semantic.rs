#![expect(
    clippy::unwrap_used,
    reason = "integration assertions over values constructed in the same test"
)]

use sha2::Digest as _;
use std::borrow::Cow;

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;
use amiss_wire::semantic::{
    PAYLOAD_SCHEMA, PayloadSchema, SEMANTIC_EVIDENCE_BYTES, SemanticEvidence,
    SemanticEvidenceTemplate, SemanticProducer, SemanticProducerKind, SemanticSubject,
    TemplateSchema, bind_template, envelope, observation::Observation, parse, record, template,
    write,
};

const A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn observation(name: &str) -> Observation {
    Observation::Record(record::Observation {
        kind: record::ObservationKind::Current,
        name: name.parse().unwrap(),
        records: Vec::new(),
    })
}

fn evidence(observations: Vec<Observation>) -> SemanticEvidence<'static> {
    SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest: A.parse().unwrap(),
            source_report_payload_digest: Nullable::Value(B.parse().unwrap()),
        },
        producer: SemanticProducer {
            kind: SemanticProducerKind::RecordSet,
            identity: "test-public-api".parse().unwrap(),
            version: "1".to_owned(),
            context_digest: B.parse().unwrap(),
            input_digest: C.parse().unwrap(),
        },
        complete: true,
        observations: observations.into_iter().map(Cow::Owned).collect(),
    }
}

fn evidence_template(observations: Vec<Observation>) -> SemanticEvidenceTemplate<'static> {
    SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: evidence(Vec::new()).producer,
        complete: true,
        observations: observations.into_iter().map(Cow::Owned).collect(),
    }
}

#[test]
fn construction_sorts_observations_and_binds_the_payload() {
    let a = observation("a");
    let z = observation("z");
    let document = envelope(evidence(vec![z.clone(), a.clone()])).unwrap();
    let mut bytes = Vec::new();
    write(&document, &mut bytes).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed, document);
    assert_eq!(parsed.payload.observations, [Cow::Owned(a), Cow::Owned(z)]);
    assert_eq!(
        parsed.payload_digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&parsed.payload).unwrap())
                .finalize()
                .0
        )
    );
}

#[test]
fn typed_templates_borrow_observations_through_sorting_and_binding() {
    let input = evidence_template(vec![observation("z"), observation("a")]);
    let shared = input.clone();
    assert!(std::sync::Arc::ptr_eq(
        &input.observations,
        &shared.observations
    ));
    let first = bind_template(&input, A.parse().unwrap()).unwrap();
    let second = bind_template(&input, B.parse().unwrap()).unwrap();
    for document in [&first, &second] {
        for (bound, original) in document
            .payload
            .observations
            .iter()
            .zip(input.observations.iter().rev())
        {
            assert!(matches!(bound, Cow::Borrowed(_)));
            assert!(std::ptr::eq(bound.as_ref(), original.as_ref()));
        }
        let mut bytes = Vec::new();
        write(document, &mut bytes).unwrap();
        assert_eq!(parse(&bytes).unwrap(), *document);
    }
    assert_eq!(input.observations[0].as_ref(), &observation("z"));
    assert_eq!(first.payload.observations, second.payload.observations);
    assert_ne!(first.payload_digest, second.payload_digest);
    assert_eq!(
        serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(
            &template(input.clone()).unwrap()
        )
        .unwrap()
        .observations
        .as_ref(),
        first.payload.observations
    );
}

#[test]
fn candidate_free_templates_bind_only_when_the_candidate_is_known() {
    let row = observation("rust/public-api");
    let input = evidence_template(vec![row.clone()]);
    let parsed = bind_template(&input, A.parse().unwrap()).unwrap();
    assert_eq!(
        parsed.payload.subject.candidate_identity_digest,
        A.parse().unwrap()
    );
    assert_eq!(
        parsed.payload.subject.source_report_payload_digest,
        Nullable::Null
    );
    assert_eq!(parsed.payload.observations, [Cow::Owned(row)]);
}

#[test]
fn strict_templates_have_no_candidate_or_report_binding_surface() {
    let input = evidence_template(Vec::new());
    let valid = template(input.clone()).unwrap();
    assert_eq!(
        serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(&valid).unwrap(),
        input
    );
    let valid = String::from_utf8(valid).unwrap();
    for field in ["candidate_identity_digest", "source_report_payload_digest"] {
        let invalid = valid.replacen('{', &format!(r#"{{"{field}":"{A}","#), 1);
        assert!(
            serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(invalid.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("unknown field")
        );
    }
}

#[test]
fn template_observations_must_already_be_canonical_sets() {
    let mut input = evidence_template(vec![observation("a"), observation("z")]);
    let parsed: SemanticEvidenceTemplate<'static> =
        serde_json::from_slice(&serde_json_canonicalizer::to_vec(&input).unwrap()).unwrap();
    assert!(parsed.validate().is_ok());
    std::sync::Arc::make_mut(&mut input.observations).reverse();
    assert_eq!(
        serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(
            &serde_json_canonicalizer::to_vec(&input).unwrap()
        )
        .unwrap()
        .validate()
        .unwrap_err()
        .kind,
        ErrorKind::UnsortedSet
    );
}

#[test]
fn duplicate_observations_are_refused() {
    let rows = vec![observation("a"); 2];
    assert_eq!(
        envelope(evidence(rows.clone())).unwrap_err().kind,
        ErrorKind::DuplicateMember
    );
    assert_eq!(
        template(evidence_template(rows.clone())).unwrap_err().kind,
        ErrorKind::DuplicateMember
    );
    assert_eq!(
        serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(
            &serde_json_canonicalizer::to_vec(&evidence_template(rows)).unwrap()
        )
        .unwrap()
        .validate()
        .unwrap_err()
        .kind,
        ErrorKind::DuplicateMember
    );
}

#[test]
fn semantic_readers_refuse_unknown_shapes_even_with_correct_payload_digests() {
    let row = observation("rust/api");
    let original = String::from_utf8(serde_json_canonicalizer::to_vec(&row).unwrap()).unwrap();
    for invalid in [
        "null",
        "[]",
        r#"["record-set","rust/api",[]]"#,
        r#"["site-route","/guide","guide.md",[]]"#,
        "{}",
        r#"{"kind":null}"#,
        r#"{"kind":1}"#,
        r#"{"kind":"../bad"}"#,
        r#"{"arbitrary":{"new":[true,null]},"kind":"future-fact"}"#,
        r#"{"extra":true,"kind":"record-set","name":"rust/api","records":[]}"#,
    ] {
        let payload = String::from_utf8(
            serde_json_canonicalizer::to_vec(&evidence(vec![row.clone()])).unwrap(),
        )
        .unwrap()
        .replace(&original, invalid);
        let digest = amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(payload.as_bytes())
                .finalize()
                .0,
        );
        let envelope = format!(
            r#"{{"schema":"amiss/semantic-evidence-envelope","payload":{payload},"payload_digest":"{digest}"}}"#
        );
        assert!(parse(envelope.as_bytes()).is_err(), "{invalid}");
        let template = String::from_utf8(template(evidence_template(vec![row.clone()])).unwrap())
            .unwrap()
            .replace(&original, invalid);
        assert!(
            serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(template.as_bytes())
                .is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn semantic_readers_enforce_strict_json_before_decoding_observations() {
    let row = observation("a");
    let document = envelope(evidence(vec![row.clone()])).unwrap();
    let mut bytes = Vec::new();
    write(&document, &mut bytes).unwrap();
    let original = String::from_utf8(serde_json_canonicalizer::to_vec(&row).unwrap()).unwrap();
    let nested = format!("{}null{}", "[".repeat(511), "]".repeat(511));
    assert!(amiss_wire::de::JsonProfile::validate(nested.as_bytes()).is_ok());
    for invalid in [
        "9007199254740992".to_owned(),
        nested,
        r#"{"kind":"record-set","kind":"record-set"}"#.to_owned(),
    ] {
        let malformed = String::from_utf8(bytes.clone())
            .unwrap()
            .replace(&original, &invalid);
        let error = parse(malformed.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$");
        assert_eq!(
            error,
            amiss_wire::de::JsonProfile::validate(malformed.as_bytes()).unwrap_err()
        );
    }
}

#[test]
fn serialized_semantic_bytes_preserve_unicode_and_escaping() {
    let row = Observation::Record(record::Observation {
        kind: record::ObservationKind::Current,
        name: "rust/api".parse().unwrap(),
        records: vec![record::Record {
            key: "\u{e000}\u{10000}".to_owned(),
            value: "quote\" slash/ backslash\\ newline\n nul\0 é".to_owned(),
        }],
    });
    let expected = br#"quote\" slash/ backslash\\ newline\n nul\u0000 "#;
    let document = envelope(evidence(vec![row.clone()])).unwrap();
    let mut bytes = Vec::new();
    write(&document, &mut bytes).unwrap();
    for bytes in [
        template(evidence_template(vec![row.clone()])).unwrap(),
        bytes,
    ] {
        assert!(
            bytes
                .windows(expected.len())
                .any(|window| window == expected)
        );
        assert!(!bytes.ends_with(b"\n"));
    }
    let parsed = serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(
        &template(evidence_template(vec![row.clone()])).unwrap(),
    )
    .unwrap();
    assert_eq!(parsed.observations.as_ref(), [Cow::Owned(row)]);
}

#[test]
fn serialized_semantic_bytes_enforce_the_complete_document_ceiling() {
    let mut records = record::Observation {
        kind: record::ObservationKind::Current,
        name: "rust/api".parse().unwrap(),
        records: vec![record::Record {
            key: "a".to_owned(),
            value: String::new(),
        }],
    };
    let overhead = template(evidence_template(vec![Observation::Record(
        records.clone(),
    )]))
    .unwrap()
    .len();
    let limit = usize::try_from(SEMANTIC_EVIDENCE_BYTES).unwrap();
    records.records[0].value = "x".repeat(limit - overhead);
    let bytes = template(evidence_template(vec![Observation::Record(
        records.clone(),
    )]))
    .unwrap();
    assert_eq!(bytes.len(), limit);
    assert!(serde_json::from_slice::<SemanticEvidenceTemplate<'static>>(&bytes).is_ok());
    let document = envelope(evidence(vec![Observation::Record(records.clone())])).unwrap();
    assert_eq!(
        write(&document, std::io::sink()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    records.records[0].value.push('x');
    let error = template(evidence_template(vec![Observation::Record(records)])).unwrap_err();
    assert_eq!(error.path, "$");
    assert_eq!(error.kind, ErrorKind::LimitExceeded);
}

#[test]
fn incomplete_pre_report_evidence_round_trips_without_claiming_absence() {
    let mut input = evidence(Vec::new());
    input.subject.source_report_payload_digest = Nullable::Null;
    input.complete = false;
    let document = envelope(input).unwrap();
    let mut bytes = Vec::new();
    write(&document, &mut bytes).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(
        parsed.payload.subject.source_report_payload_digest,
        Nullable::Null
    );
    assert!(!parsed.payload.complete);
}

#[test]
fn producer_versions_and_input_bytes_are_bounded_before_parsing() {
    let mut input = evidence(Vec::new());
    input.producer.version = "bad version".to_owned();
    assert_eq!(envelope(input).unwrap_err().kind, ErrorKind::InvalidValue);
    let oversized = vec![b' '; usize::try_from(SEMANTIC_EVIDENCE_BYTES).unwrap() + 1];
    assert_eq!(
        parse(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn tampered_and_unsorted_payloads_are_refused() {
    let mut document = envelope(evidence(vec![observation("a"), observation("z")])).unwrap();
    document.payload.observations.reverse();
    assert_eq!(
        parse(&serde_json_canonicalizer::to_vec(&document).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::DigestMismatch
    );
    document.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&document.payload).unwrap())
            .finalize()
            .0,
    );
    assert_eq!(
        parse(&serde_json_canonicalizer::to_vec(&document).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::UnsortedSet
    );
}

#[test]
fn semantic_templates_reject_positional_producers_and_record_rows() {
    let mut record = observation("rust/api");
    if let Observation::Record(record) = &mut record {
        record.records.push(record::Record {
            key: "entry".to_owned(),
            value: "value".to_owned(),
        });
    }
    let original = serde_json::to_value(evidence_template(vec![record])).unwrap();
    let mut producer_array = original.clone();
    let producer = &original["producer"];
    producer_array["producer"] = serde_json::json!([
        producer["kind"],
        producer["identity"],
        producer["version"],
        producer["context_digest"],
        producer["input_digest"]
    ]);
    let bytes = serde_json::to_vec(&producer_array).unwrap();
    let error = serde_path_to_error::deserialize::<_, SemanticEvidenceTemplate<'static>>(
        &mut serde_json::Deserializer::from_slice(&bytes),
    )
    .unwrap_err();
    assert!(error.inner().is_data());
    assert_eq!(error.path().to_string(), "producer");
    let mut record_array = original;
    record_array["observations"][0]["records"][0] = serde_json::json!(["entry", "value"]);
    let bytes = serde_json::to_vec(&record_array).unwrap();
    let error = serde_path_to_error::deserialize::<_, SemanticEvidenceTemplate<'static>>(
        &mut serde_json::Deserializer::from_slice(&bytes),
    )
    .unwrap_err();
    assert!(error.inner().is_data());
    assert_eq!(error.path().to_string(), "observations[0]");
    let input = serde_json::json!({
        "schema": "amiss/record-set-input", "producer_identity": "test-public-api",
        "context_digest": B, "input_digest": C, "complete": true,
        "name": "rust/api", "records": [["entry", "value"]]
    });
    let error = record::parse_input(&serde_json::to_vec(&input).unwrap()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::WrongType);
    assert_eq!(error.path, "$.records[0]");
}

#[test]
fn received_semantic_payloads_keep_shape_digest_and_contract_error_precedence() {
    let original = envelope(evidence(vec![observation("rust/api")])).unwrap();
    let mut document = serde_json::to_value(&original).unwrap();
    document["payload_digest"] = serde_json::json!(C);
    document["payload"]["producer"]["version"] = serde_json::json!(1);
    let shape_error = parse(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(shape_error.path, "$.payload.producer.version");
    assert_eq!(shape_error.kind, ErrorKind::WrongType);
    document["payload"]["producer"]["version"] = serde_json::json!("");
    let digest_error = parse(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(digest_error.path, "$.payload_digest");
    assert_eq!(digest_error.kind, ErrorKind::DigestMismatch);
    document["payload_digest"] = serde_json::json!(amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&document["payload"]).unwrap())
            .finalize()
            .0
    ));
    let contract_error = parse(&serde_json::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(contract_error.path, "$.payload.producer.version");
    assert_eq!(contract_error.kind, ErrorKind::InvalidValue);
    let mut positional = serde_json::to_value(original).unwrap();
    let payload = &positional["payload"];
    positional["payload"] = serde_json::json!([
        payload["schema"],
        payload["subject"],
        payload["producer"],
        payload["complete"],
        payload["observations"]
    ]);
    let bytes = serde_json::to_vec(&positional).unwrap();
    assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::WrongType);
    assert!(
        serde_json::from_slice::<amiss_wire::semantic::SemanticEvidenceEnvelope<'static>>(&bytes)
            .is_err()
    );
}
