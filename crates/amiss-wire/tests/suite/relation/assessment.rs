use crate::relation_fixture::{digest, identity, projected, relation_contract};

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hb;
use amiss_wire::relation::{
    ASSESSMENT_PAYLOAD_SCHEMA, RelationProjectionSlot, RelationReason, RelationVerdict, assess,
    evidence, parse_assessment, plan,
};

#[test]
fn complete_projection_pairs_classify_all_four_equality_transitions() {
    let plan = plan(relation_contract().plan).unwrap();
    let mut introduced = relation_contract().evidence;
    introduced.plan_payload_digest = plan.payload_digest;

    let mut aligned = introduced.clone();
    aligned.subjects[1].candidate = aligned.subjects[0].candidate;

    let mut pre_existing = introduced.clone();
    pre_existing.subjects[1].base = pre_existing.subjects[1].candidate;

    let mut resolved = pre_existing.clone();
    resolved.subjects[1].candidate = resolved.subjects[0].candidate;

    for (input, expected) in [
        (aligned, RelationVerdict::Aligned),
        (introduced, RelationVerdict::IntroducedDrift),
        (pre_existing, RelationVerdict::PreExistingDrift),
        (resolved, RelationVerdict::ResolvedDrift),
    ] {
        let evidence = evidence(input).unwrap();
        let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
        assert_eq!(assessment.payload.verdict, expected);
        assert_eq!(assessment.payload.reason, Nullable::Null);
        assert_eq!(
            assessment.payload.subject.evidence_payload_digest,
            Nullable::Value(evidence.payload_digest)
        );
    }
}

#[test]
fn digest_and_length_jointly_define_projected_value_equality() {
    let plan = plan(relation_contract().plan).unwrap();
    let mut input = relation_contract().evidence;
    input.plan_payload_digest = plan.payload_digest;
    input.subjects[1].candidate = input.subjects[0].candidate;
    let mut candidate = projected('a', 1_024);
    candidate.value_bytes = candidate.value_bytes.saturating_add(1);
    input.subjects[1].candidate = RelationProjectionSlot::Projected(candidate);

    let evidence = evidence(input).unwrap();
    assert_eq!(
        assess(&plan, Some(&evidence), "0.26.0", digest('a'))
            .unwrap()
            .payload
            .verdict,
        RelationVerdict::IntroducedDrift
    );
}

#[test]
fn absent_unbound_misrouted_and_partial_evidence_stays_unproven() {
    let plan = plan(relation_contract().plan).unwrap();

    let mut unbound = relation_contract().evidence;
    unbound.plan_payload_digest = digest('9');
    let unbound = evidence(unbound).unwrap();

    let mut misrouted = relation_contract().evidence;
    misrouted.plan_payload_digest = plan.payload_digest;
    misrouted.subjects[0].role = identity("manual");
    let misrouted = evidence(misrouted).unwrap();

    let mut partial = relation_contract().evidence;
    partial.plan_payload_digest = plan.payload_digest;
    partial.subjects[1].base = RelationProjectionSlot::Unproven;
    let partial = evidence(partial).unwrap();

    for (evidence, expected) in [
        (None, RelationReason::EvidenceAbsent),
        (Some(unbound), RelationReason::EvidenceUnbound),
        (Some(misrouted), RelationReason::RoleMismatch),
        (Some(partial), RelationReason::ProjectionUnproven),
    ] {
        let assessment = assess(&plan, evidence.as_ref(), "0.26.0", digest('a')).unwrap();
        assert_eq!(assessment.payload.verdict, RelationVerdict::Unproven);
        assert_eq!(assessment.payload.reason, Nullable::Value(expected));
        assert_eq!(
            assessment.payload.subject.evidence_payload_digest,
            evidence.as_ref().map_or(Nullable::Null, |value| {
                Nullable::Value(value.payload_digest)
            })
        );
    }
}

#[test]
fn assessment_rejects_mutated_inputs_and_inconsistent_output() {
    let mut broken_plan = plan(relation_contract().plan).unwrap();
    broken_plan.payload_digest = digest('f');
    let error = assess(&broken_plan, None, "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.plan.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let plan = plan(relation_contract().plan).unwrap();
    let mut input = relation_contract().evidence;
    input.plan_payload_digest = plan.payload_digest;
    let mut broken_evidence = evidence(input.clone()).unwrap();
    broken_evidence.payload_digest = digest('f');
    let error = assess(&plan, Some(&broken_evidence), "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.evidence.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let evidence = evidence(input).unwrap();
    let mut inconsistent = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    inconsistent.payload.verdict = RelationVerdict::Unproven;
    inconsistent.payload_digest = hb(
        ASSESSMENT_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&inconsistent.payload).unwrap(),
    );
    let error =
        parse_assessment(&serde_json_canonicalizer::to_vec(&inconsistent).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn nullable_assessment_fields_are_required() {
    let assessment = assess(
        &plan(relation_contract().plan).unwrap(),
        None,
        "0.26.0",
        digest('a'),
    )
    .unwrap();
    let text = String::from_utf8(serde_json_canonicalizer::to_vec(&assessment).unwrap()).unwrap();
    for (member, path) in [
        (r#""reason":"evidence-absent","#, "$.payload.reason"),
        (
            r#""evidence_payload_digest":null,"#,
            "$.payload.subject.evidence_payload_digest",
        ),
    ] {
        let missing = text.replacen(member, "", 1);
        assert_ne!(missing, text);
        let error = parse_assessment(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
}
