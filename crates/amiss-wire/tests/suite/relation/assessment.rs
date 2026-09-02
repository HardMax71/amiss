use crate::relation_fixture::{digest, identity, projected, relation_contract};

use std::{fs, path::Path};

use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json;
use amiss_wire::relation::{
    ASSESSMENT_PAYLOAD_SCHEMA, RelationAssessmentEnvelope, RelationEvidence,
    RelationEvidenceEnvelope, RelationPlanEnvelope, RelationProjectionSlot, RelationReason,
    RelationVerdict, assess, evidence, parse_assessment, parse_evidence, parse_plan, plan,
};

fn plan_envelope() -> RelationPlanEnvelope {
    parse_plan(&json::canonical(&plan(&relation_contract().plan).unwrap())).unwrap()
}

fn evidence_envelope(input: &RelationEvidence) -> RelationEvidenceEnvelope {
    parse_evidence(&json::canonical(&evidence(input).unwrap())).unwrap()
}

fn assessed(
    plan: &RelationPlanEnvelope,
    evidence: Option<&RelationEvidenceEnvelope>,
) -> RelationAssessmentEnvelope {
    parse_assessment(&json::canonical(
        &assess(plan, evidence, "0.26.0", digest('a')).unwrap(),
    ))
    .unwrap()
}

#[test]
fn complete_projection_pairs_classify_all_four_equality_transitions() {
    let plan = plan_envelope();
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
        let evidence = evidence_envelope(&input);
        let assessment = assessed(&plan, Some(&evidence));
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
    let plan = plan_envelope();
    let mut input = relation_contract().evidence;
    input.plan_payload_digest = plan.payload_digest;
    input.subjects[1].candidate = input.subjects[0].candidate;
    let mut candidate = projected('a', 1_024);
    candidate.value_bytes = candidate.value_bytes.saturating_add(1);
    input.subjects[1].candidate = RelationProjectionSlot::Projected(candidate);

    let evidence = evidence_envelope(&input);
    assert_eq!(
        assessed(&plan, Some(&evidence)).payload.verdict,
        RelationVerdict::IntroducedDrift
    );
}

#[test]
fn absent_unbound_misrouted_and_partial_evidence_stays_unproven() {
    let plan = plan_envelope();

    let mut unbound = relation_contract().evidence;
    unbound.plan_payload_digest = digest('9');
    let unbound = evidence_envelope(&unbound);

    let mut misrouted = relation_contract().evidence;
    misrouted.plan_payload_digest = plan.payload_digest;
    misrouted.subjects[0].role = identity("manual");
    let misrouted = evidence_envelope(&misrouted);

    let mut partial = relation_contract().evidence;
    partial.plan_payload_digest = plan.payload_digest;
    partial.subjects[1].base = RelationProjectionSlot::Unproven;
    let partial = evidence_envelope(&partial);

    for (evidence, expected) in [
        (None, RelationReason::EvidenceAbsent),
        (Some(unbound), RelationReason::EvidenceUnbound),
        (Some(misrouted), RelationReason::RoleMismatch),
        (Some(partial), RelationReason::ProjectionUnproven),
    ] {
        let assessment = assessed(&plan, evidence.as_ref());
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
    let mut broken_plan = plan_envelope();
    broken_plan.payload_digest = digest('f');
    let error = assess(&broken_plan, None, "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.plan.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let plan = plan_envelope();
    let mut input = relation_contract().evidence;
    input.plan_payload_digest = plan.payload_digest;
    let mut broken_evidence = evidence_envelope(&input);
    broken_evidence.payload_digest = digest('f');
    let error = assess(&plan, Some(&broken_evidence), "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.evidence.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let evidence = evidence_envelope(&input);
    let value = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let inconsistent = String::from_utf8(json::canonical(&value))
        .unwrap()
        .replace("\"introduced-drift\"", "\"unproven\"");
    let inconsistent_value = json::parse(inconsistent.as_bytes()).unwrap();
    let rebound = inconsistent.replace(
        recorded,
        &hj(
            ASSESSMENT_PAYLOAD_SCHEMA,
            inconsistent_value.member("payload").unwrap(),
        )
        .to_string(),
    );
    let error = parse_assessment(rebound.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert_eq!(error.kind, ErrorKind::Inconsistent);
}

#[test]
fn nullable_assessment_fields_are_required() {
    let value = assess(&plan_envelope(), None, "0.26.0", digest('a')).unwrap();
    let bytes = json::canonical(&value);

    let mut missing_reason: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        missing_reason["payload"]
            .as_object_mut()
            .unwrap()
            .remove("reason")
            .is_some()
    );
    let payload = serde_json_canonicalizer::to_vec(&missing_reason["payload"]).unwrap();
    missing_reason["payload_digest"] =
        serde_json::json!(hb(ASSESSMENT_PAYLOAD_SCHEMA, &payload).to_string());
    assert_eq!(
        parse_assessment(&serde_json_canonicalizer::to_vec(&missing_reason).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );

    let mut missing_evidence_digest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        missing_evidence_digest["payload"]["subject"]
            .as_object_mut()
            .unwrap()
            .remove("evidence_payload_digest")
            .is_some()
    );
    let payload = serde_json_canonicalizer::to_vec(&missing_evidence_digest["payload"]).unwrap();
    missing_evidence_digest["payload_digest"] =
        serde_json::json!(hb(ASSESSMENT_PAYLOAD_SCHEMA, &payload).to_string());
    assert_eq!(
        parse_assessment(&serde_json_canonicalizer::to_vec(&missing_evidence_digest).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
}

#[test]
fn the_published_assessment_replays_from_its_plan_and_evidence() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let plan = parse_plan(&fs::read(examples.join("relation-plan.json")).unwrap()).unwrap();
    let evidence =
        parse_evidence(&fs::read(examples.join("relation-evidence.json")).unwrap()).unwrap();
    let published_bytes = fs::read(examples.join("relation-assessment.json")).unwrap();
    let published = parse_assessment(&published_bytes).unwrap();
    let replayed = assess(
        &plan,
        Some(&evidence),
        &published.payload.engine.engine_version,
        published.payload.engine.engine_digest,
    )
    .unwrap();

    assert_eq!(
        json::canonical(&replayed),
        json::canonical(&json::parse(&published_bytes).unwrap())
    );
}
