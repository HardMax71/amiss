#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration assertions over values constructed in the same test"
)]

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{
    PAYLOAD_SCHEMA, SEMANTIC_EVIDENCE_BYTES, SemanticEvidence, envelope, parse,
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

fn observation(rows: Vec<(&str, Value)>) -> Value {
    Value::object(
        rows.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn evidence(observations: Vec<Value>) -> SemanticEvidence {
    SemanticEvidence {
        candidate_identity_digest: digest(A),
        source_report_payload_digest: Some(digest(B)),
        producer_kind: id("sphinx-inventory"),
        producer_identity: id("amiss-intersphinx"),
        producer_version: "0.1.0".to_owned(),
        input_digest: digest(C),
        complete: true,
        observations,
    }
}

#[test]
fn real_inventory_and_site_shapes_share_an_envelope_without_sharing_vocabularies() {
    let inventory = observation(vec![
        ("kind", Value::string("sphinx-reference")),
        ("inventory", Value::string("python")),
        ("domain", Value::string("std")),
        ("role", Value::string("label")),
        ("name", Value::string("context-managers")),
        (
            "uri",
            Value::string("reference/datamodel.html#context-managers"),
        ),
    ]);
    let route = observation(vec![
        ("kind", Value::string("site-route")),
        ("route", Value::string("/external-assessment.html")),
        ("source", Value::string("docs/src/external-assessment.md")),
        (
            "anchors",
            Value::array(vec![Value::string("the-external-assessment")]),
        ),
    ]);

    let first = parse(&canonical(&envelope(evidence(vec![inventory])).unwrap())).unwrap();
    let mut site = evidence(vec![route]);
    site.producer_kind = id("site-build");
    site.producer_identity = id("amiss-site-output");
    let second = parse(&canonical(&envelope(site).unwrap())).unwrap();

    assert_eq!(
        first.payload.observations[0].text("kind"),
        Some("sphinx-reference")
    );
    assert_eq!(
        second.payload.observations[0].text("kind"),
        Some("site-route")
    );
}

#[test]
fn construction_sorts_observations_and_binds_the_payload() {
    let a = observation(vec![
        ("kind", Value::string("site-route")),
        ("route", Value::string("/a")),
    ]);
    let z = observation(vec![
        ("kind", Value::string("site-route")),
        ("route", Value::string("/z")),
    ]);
    let value = envelope(evidence(vec![z, a])).unwrap();
    let parsed = parse(&canonical(&value)).unwrap();
    assert_eq!(parsed.payload.observations[0].text("route"), Some("/a"));
    assert_eq!(
        parsed.payload_digest,
        hj(PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
}

#[test]
fn duplicate_observations_are_refused() {
    let row = observation(vec![("kind", Value::string("site-route"))]);
    let error = envelope(evidence(vec![row.clone(), row])).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DuplicateMember);
}

#[test]
fn incomplete_pre_report_evidence_round_trips_without_claiming_absence() {
    let mut input = evidence(Vec::new());
    input.source_report_payload_digest = None;
    input.complete = false;
    let parsed = parse(&canonical(&envelope(input).unwrap())).unwrap();
    assert_eq!(parsed.payload.source_report_payload_digest, None);
    assert!(!parsed.payload.complete);
}

#[test]
fn producer_versions_and_input_bytes_are_bounded_before_parsing() {
    let mut input = evidence(Vec::new());
    input.producer_version = "bad version".to_owned();
    assert_eq!(envelope(input).unwrap_err().kind, ErrorKind::InvalidValue);

    let oversized = vec![b' '; usize::try_from(SEMANTIC_EVIDENCE_BYTES).unwrap() + 1];
    assert_eq!(
        parse(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn unknown_observation_kinds_remain_inert_data() {
    let row = observation(vec![
        ("kind", Value::string("future-producer-fact")),
        ("answer", Value::Integer(42)),
    ]);
    let parsed = parse(&canonical(&envelope(evidence(vec![row])).unwrap())).unwrap();
    assert_eq!(
        parsed.payload.observations[0].text("kind"),
        Some("future-producer-fact")
    );
}

#[test]
fn tampered_and_unsorted_payloads_are_refused() {
    let a = observation(vec![
        ("kind", Value::string("site-route")),
        ("route", Value::string("/a")),
    ]);
    let z = observation(vec![
        ("kind", Value::string("site-route")),
        ("route", Value::string("/z")),
    ]);
    let mut value = envelope(evidence(vec![a, z])).unwrap();
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

fn reverse_observations(value: &mut Value) {
    let Value::Array(observations) = member_mut(member_mut(value, "payload"), "observations")
    else {
        panic!("observations are an array")
    };
    observations.reverse();
}

fn rebind_payload(value: &mut Value) {
    let payload_digest = hj(PAYLOAD_SCHEMA, value.member("payload").unwrap());
    *member_mut(value, "payload_digest") = Value::string(payload_digest.to_string());
}

fn member_mut<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    let Value::Object(members) = value else {
        panic!("member parent is an object")
    };
    members
        .iter_mut()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value)
        .expect("member exists")
}
