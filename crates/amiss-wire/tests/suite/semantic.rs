#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration assertions over values constructed in the same test"
)]

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{ErrorKind as JsonErrorKind, Value as WireValue, canonical};
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{
    PAYLOAD_SCHEMA, PayloadSchema, SEMANTIC_EVIDENCE_BYTES, SemanticEvidence,
    SemanticEvidenceTemplate, SemanticProducer, SemanticSubject, TEMPLATE_SCHEMA, TemplateSchema,
    bind_template, envelope, parse, parse_template, template,
};

const A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn digest(raw: &str) -> Digest {
    Digest::from_wire(raw).unwrap()
}

fn id(raw: &str) -> ArtifactId {
    ArtifactId::new(raw.to_owned()).unwrap()
}

fn observation(rows: Vec<(&str, serde_json::Value)>) -> serde_json::Value {
    serde_json::Value::Object(
        rows.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn evidence(observations: Vec<serde_json::Value>) -> SemanticEvidence {
    SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest: digest(A),
            source_report_payload_digest: Nullable::Value(digest(B)),
        },
        producer: SemanticProducer {
            kind: id("sphinx-inventory"),
            identity: id("amiss-intersphinx"),
            version: "0.1.0".to_owned(),
            context_digest: digest(C),
            input_digest: digest(C),
        },
        complete: true,
        observations,
    }
}

fn template_producer() -> SemanticProducer {
    SemanticProducer {
        kind: id("record-set"),
        identity: id("test-public-api"),
        version: "1".to_owned(),
        context_digest: digest(B),
        input_digest: digest(C),
    }
}

fn evidence_template(observations: Vec<serde_json::Value>) -> SemanticEvidenceTemplate {
    SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: template_producer(),
        complete: true,
        observations: observations.into(),
    }
}

#[test]
fn real_inventory_and_site_shapes_share_an_envelope_without_sharing_vocabularies() {
    let inventory = observation(vec![
        ("kind", serde_json::json!("sphinx-reference")),
        ("inventory", serde_json::json!("python")),
        ("domain", serde_json::json!("std")),
        ("role", serde_json::json!("label")),
        ("name", serde_json::json!("context-managers")),
        (
            "uri",
            serde_json::json!("reference/datamodel.html#context-managers"),
        ),
    ]);
    let route = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/external-assessment.html")),
        (
            "source",
            serde_json::json!("docs/src/external-assessment.md"),
        ),
        ("anchors", serde_json::json!(["the-external-assessment"])),
    ]);

    let first = parse(&envelope(evidence(vec![inventory])).unwrap()).unwrap();
    let mut site = evidence(vec![route]);
    site.producer.kind = id("site-build");
    site.producer.identity = id("amiss-site-output");
    let second = parse(&envelope(site).unwrap()).unwrap();

    assert_eq!(
        first.payload.observations[0]
            .get("kind")
            .and_then(serde_json::Value::as_str),
        Some("sphinx-reference")
    );
    assert_eq!(
        second.payload.observations[0]
            .get("kind")
            .and_then(serde_json::Value::as_str),
        Some("site-route")
    );
}

#[test]
fn construction_sorts_observations_and_binds_the_payload() {
    let a = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/a")),
    ]);
    let z = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/z")),
    ]);
    let bytes = envelope(evidence(vec![z, a])).unwrap();
    let parsed = parse(&bytes).unwrap();
    let value = amiss_wire::json::parse(&bytes).unwrap();
    assert_eq!(parsed.payload.observations[0]["route"], "/a");
    assert_eq!(
        parsed.payload_digest,
        hj(PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
}

#[test]
fn typed_templates_borrow_nonclone_observations_through_sorting_and_binding() {
    #[derive(serde::Serialize)]
    struct Fact {
        kind: ArtifactId,
        value: &'static str,
    }

    let input = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: template_producer(),
        complete: true,
        observations: vec![
            Fact {
                kind: id("future-fact"),
                value: "z",
            },
            Fact {
                kind: id("future-fact"),
                value: "a",
            },
        ]
        .into(),
    };
    let first = bind_template(&input, digest(A)).unwrap();
    let second = bind_template(&input, digest(B)).unwrap();
    let first = parse(&first).unwrap();
    let second = parse(&second).unwrap();
    assert_eq!(input.observations[0].value, "z");
    assert_eq!(first.payload.observations[0]["value"], "a");
    assert_eq!(first.payload.observations, second.payload.observations);
    assert_ne!(first.payload_digest, second.payload_digest);
    let written = template(input).unwrap();
    assert_eq!(
        parse_template(&written).unwrap().observations.as_ref(),
        first.payload.observations
    );
}

#[test]
fn observation_headers_are_deserialized_without_closing_unknown_fact_fields() {
    for row in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!({ "kind": null }),
        serde_json::json!({ "kind": 1 }),
        serde_json::json!({ "kind": "../bad" }),
    ] {
        assert!(envelope(evidence(vec![row.clone()])).is_err(), "{row}");
    }
    let row = serde_json::json!({ "kind": "future-fact", "arbitrary": { "new": [true, null] } });
    assert_eq!(
        parse(&envelope(evidence(vec![row.clone()])).unwrap())
            .unwrap()
            .payload
            .observations,
        vec![row]
    );
}

#[test]
fn candidate_free_templates_bind_only_when_the_candidate_is_known() {
    let row = observation(vec![
        ("kind", serde_json::json!("record-set")),
        ("name", serde_json::json!("rust/public-api")),
        ("records", serde_json::json!([])),
    ]);
    let input = evidence_template(vec![row.clone()]);
    let bytes = bind_template(&input, digest(A)).unwrap();
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.payload.subject.candidate_identity_digest, digest(A));
    assert_eq!(
        parsed.payload.subject.source_report_payload_digest,
        Nullable::Null
    );
    assert_eq!(parsed.payload.observations, vec![row]);
}

#[test]
fn strict_templates_have_no_candidate_or_report_binding_surface() {
    let valid = WireValue::object(vec![
        ("schema".to_owned(), WireValue::string(TEMPLATE_SCHEMA)),
        (
            "producer".to_owned(),
            WireValue::object(vec![
                ("kind".to_owned(), WireValue::string("record-set")),
                ("identity".to_owned(), WireValue::string("test-public-api")),
                ("version".to_owned(), WireValue::string("1")),
                ("context_digest".to_owned(), WireValue::string(B)),
                ("input_digest".to_owned(), WireValue::string(C)),
            ]),
        ),
        ("complete".to_owned(), WireValue::Bool(true)),
        ("observations".to_owned(), WireValue::array(Vec::new())),
    ]);
    assert_eq!(
        parse_template(&canonical(&valid)).unwrap(),
        evidence_template(Vec::new())
    );

    for field in ["candidate_identity_digest", "source_report_payload_digest"] {
        let WireValue::Object(members) = &valid else {
            panic!("the fixture is an object")
        };
        let mut members = members.as_ref().to_vec();
        members.push((field.to_owned(), WireValue::string(A)));
        let invalid = WireValue::object(members);
        let error = parse_template(&canonical(&invalid)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::UnknownField);
    }
}

#[test]
fn template_observations_must_already_be_canonical_sets() {
    let a = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/a")),
    ]);
    let z = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/z")),
    ]);
    let input = evidence_template(vec![a, z]);
    let bytes = bind_template(&input, digest(A)).unwrap();
    let mut value = amiss_wire::json::parse(&bytes).unwrap();
    let payload = member_mut(&mut value, "payload");
    let observations = member_mut(payload, "observations").clone();
    let producer = member_mut(payload, "producer").clone();
    let template_value = WireValue::object(vec![
        ("schema".to_owned(), WireValue::string(TEMPLATE_SCHEMA)),
        ("producer".to_owned(), producer),
        ("complete".to_owned(), WireValue::Bool(true)),
        ("observations".to_owned(), observations),
    ]);
    let mut reversed = template_value.clone();
    reverse_template_observations(&mut reversed);
    assert_eq!(
        parse_template(&canonical(&reversed)).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );
    assert!(parse_template(&canonical(&template_value)).is_ok());
}

#[test]
fn duplicate_observations_are_refused() {
    let row = observation(vec![("kind", serde_json::json!("site-route"))]);
    let error = envelope(evidence(vec![row.clone(), row])).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DuplicateMember);
}

#[test]
fn construction_refuses_observation_values_outside_strict_json() {
    let unsafe_integer = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("count", serde_json::json!(9_007_199_254_740_992_i64)),
    ]);
    let mut deep = serde_json::Value::Null;
    for _ in 0..513 {
        deep = serde_json::Value::Array(vec![deep]);
    }
    let excessive_depth = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("value", deep),
    ]);

    for (invalid, defect) in [
        (unsafe_integer, JsonErrorKind::IntegerOutOfRange),
        (excessive_depth, JsonErrorKind::DepthLimit),
    ] {
        let error = envelope(evidence(vec![invalid])).unwrap_err();
        assert_eq!(error.path, "$.payload.observations[0]");
        assert!(matches!(error.kind, ErrorKind::Json(error) if error.kind == defect));
    }
}

#[test]
fn serialized_semantic_bytes_preserve_unicode_order_and_escaping() {
    let row = serde_json::json!({
        "kind": "future-fact",
        "\u{e000}": "last key",
        "\u{10000}": "earlier UTF-16 key",
        "data": "quote\" slash/ backslash\\ newline\n nul\0 é",
    });
    for bytes in [
        template(evidence_template(vec![row.clone()])).unwrap(),
        envelope(evidence(vec![row])).unwrap(),
    ] {
        let value = amiss_wire::json::parse(&bytes).unwrap();
        assert_eq!(bytes, canonical(&value));
        assert!(!bytes.ends_with(b"\n"));
    }
}

#[test]
fn serialized_semantic_bytes_check_depth_after_wrapping_observations() {
    let mut nested = serde_json::Value::Null;
    for _ in 0..511 {
        nested = serde_json::Value::Array(vec![nested]);
    }
    let row = serde_json::json!({ "kind": "future-fact", "data": nested });
    let observation_bytes = serde_json_canonicalizer::to_vec(&row).unwrap();
    assert!(amiss_wire::json::parse(&observation_bytes).is_ok());
    for error in [
        template(evidence_template(vec![row.clone()])).unwrap_err(),
        envelope(evidence(vec![row])).unwrap_err(),
    ] {
        assert_eq!(error.path, "$");
        assert!(
            matches!(error.kind, ErrorKind::Json(error) if error.kind == JsonErrorKind::DepthLimit)
        );
    }
}

#[test]
fn serialized_semantic_bytes_enforce_the_complete_document_ceiling() {
    let mut row = serde_json::json!({ "kind": "future-fact", "data": "" });
    let overhead = template(evidence_template(vec![row.clone()]))
        .unwrap()
        .len();
    let limit = usize::try_from(SEMANTIC_EVIDENCE_BYTES).unwrap();
    row["data"] = serde_json::Value::String("x".repeat(limit - overhead));
    let bytes = template(evidence_template(vec![row.clone()])).unwrap();
    assert_eq!(bytes.len(), limit);
    assert!(parse_template(&bytes).is_ok());
    assert_eq!(
        envelope(evidence(vec![row.clone()])).unwrap_err().kind,
        ErrorKind::LimitExceeded,
        "candidate binding adds bytes beyond the template ceiling"
    );
    row["data"] = serde_json::Value::String("x".repeat(limit - overhead + 1));
    let error = template(evidence_template(vec![row])).unwrap_err();
    assert_eq!(error.path, "$");
    assert_eq!(error.kind, ErrorKind::LimitExceeded);
}

#[test]
fn incomplete_pre_report_evidence_round_trips_without_claiming_absence() {
    let mut input = evidence(Vec::new());
    input.subject.source_report_payload_digest = Nullable::Null;
    input.complete = false;
    let parsed = parse(&envelope(input).unwrap()).unwrap();
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
    assert_eq!(
        parse_template(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn unknown_observation_kinds_remain_inert_data() {
    let row = observation(vec![
        ("kind", serde_json::json!("future-producer-fact")),
        ("answer", serde_json::json!(42)),
    ]);
    let parsed = parse(&envelope(evidence(vec![row])).unwrap()).unwrap();
    assert_eq!(
        parsed.payload.observations[0]
            .get("kind")
            .and_then(serde_json::Value::as_str),
        Some("future-producer-fact")
    );
}

#[test]
fn tampered_and_unsorted_payloads_are_refused() {
    let a = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/a")),
    ]);
    let z = observation(vec![
        ("kind", serde_json::json!("site-route")),
        ("route", serde_json::json!("/z")),
    ]);
    let bytes = envelope(evidence(vec![a, z])).unwrap();
    let mut value = amiss_wire::json::parse(&bytes).unwrap();
    reverse_observations(&mut value);
    assert_eq!(
        parse(&canonical(&value)).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    rebind_payload(&mut value);
    assert_eq!(
        parse(&canonical(&value)).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );
}

fn reverse_observations(value: &mut WireValue) {
    let WireValue::Array(observations) = member_mut(member_mut(value, "payload"), "observations")
    else {
        panic!("observations are an array")
    };
    observations.reverse();
}

fn reverse_template_observations(value: &mut WireValue) {
    let WireValue::Array(observations) = member_mut(value, "observations") else {
        panic!("observations are an array")
    };
    observations.reverse();
}

fn rebind_payload(value: &mut WireValue) {
    let payload_digest = hj(PAYLOAD_SCHEMA, value.member("payload").unwrap());
    *member_mut(value, "payload_digest") = WireValue::string(payload_digest.to_string());
}

fn member_mut<'a>(value: &'a mut WireValue, name: &str) -> &'a mut WireValue {
    let WireValue::Object(members) = value else {
        panic!("member parent is an object")
    };
    members
        .iter_mut()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value)
        .expect("member exists")
}
