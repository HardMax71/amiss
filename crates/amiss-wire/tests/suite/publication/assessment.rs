use super::evidence::publication_evidence;
use super::{digest, oid, publication_plan};

use std::{fs, path::Path};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json;
use amiss_wire::model::ObjectFormat;
use amiss_wire::publication::{
    ASSESSMENT_PAYLOAD_SCHEMA, PublicationReason, PublicationVerdict, assess, evidence,
    parse_assessment, parse_evidence, parse_plan, plan,
};

fn plan_envelope() -> amiss_wire::publication::PublicationPlanEnvelope {
    let value = plan(&publication_plan()).unwrap();
    parse_plan(&json::canonical(&value)).unwrap()
}

fn evidence_envelope(
    evidence_value: &amiss_wire::publication::PublicationEvidence,
) -> amiss_wire::publication::PublicationEvidenceEnvelope {
    let value = evidence(evidence_value).unwrap();
    parse_evidence(&json::canonical(&value)).unwrap()
}

fn assessed(
    plan: &amiss_wire::publication::PublicationPlanEnvelope,
    evidence: Option<&amiss_wire::publication::PublicationEvidenceEnvelope>,
) -> amiss_wire::publication::PublicationAssessmentEnvelope {
    let value = assess(plan, evidence, "0.26.0", digest('a')).unwrap();
    parse_assessment(&json::canonical(&value)).unwrap()
}

#[test]
fn exact_provider_facts_match_the_publication_plan() {
    let plan = plan_envelope();
    let evidence = evidence_envelope(&publication_evidence());
    let assessment = assessed(&plan, Some(&evidence));

    assert_eq!(assessment.payload.verdict, PublicationVerdict::Matched);
    assert_eq!(assessment.payload.reasons, Vec::new());
    assert_eq!(
        assessment.payload.report_payload_digest,
        plan.payload.report_payload_digest
    );
    assert_eq!(assessment.payload.plan_payload_digest, plan.payload_digest);
    assert_eq!(
        assessment.payload.evidence_payload_digest,
        Some(evidence.payload_digest)
    );
}

#[test]
fn absent_unbound_and_foreign_producers_stay_unproven() {
    let plan = plan_envelope();
    let absent = assessed(&plan, None);
    assert_eq!(absent.payload.verdict, PublicationVerdict::Unproven);
    assert_eq!(
        absent.payload.reasons,
        vec![PublicationReason::EvidenceAbsent]
    );
    assert_eq!(absent.payload.evidence_payload_digest, None);

    let mut unbound = publication_evidence();
    unbound.plan_payload_digest = digest('f');
    let unbound = evidence_envelope(&unbound);
    let unbound_assessment = assessed(&plan, Some(&unbound));
    assert_eq!(
        unbound_assessment.payload.reasons,
        vec![PublicationReason::EvidenceUnbound]
    );

    let mut foreign = publication_evidence();
    foreign.producer.context_digest = digest('e');
    foreign.product.digest = digest('d');
    let foreign = evidence_envelope(&foreign);
    let foreign_assessment = assessed(&plan, Some(&foreign));
    assert_eq!(
        foreign_assessment.payload.reasons,
        vec![PublicationReason::ProducerMismatch]
    );
}

#[test]
fn bound_disagreements_are_one_sorted_refutation() {
    let plan = plan_envelope();
    let mut mismatched = publication_evidence();
    mismatched.docs.commit = oid('c', ObjectFormat::Sha1);
    mismatched.target.canonical_url = "https://preview.example.com/widget/".to_owned();
    mismatched.site.input_digest = digest('d');
    mismatched.product.digest = digest('e');
    let evidence = evidence_envelope(&mismatched);
    let assessment = assessed(&plan, Some(&evidence));

    assert_eq!(assessment.payload.verdict, PublicationVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            PublicationReason::DocsMismatch,
            PublicationReason::TargetMismatch,
            PublicationReason::SiteMismatch,
            PublicationReason::ProductMismatch,
        ]
    );
}

#[test]
fn assessment_rejects_mutated_envelopes_and_inconsistent_verdicts() {
    let mut plan = plan_envelope();
    plan.payload_digest = digest('f');
    let error = assess(&plan, None, "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.plan.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let valid_plan = plan_envelope();
    let value = assess(&valid_plan, None, "0.26.0", digest('a')).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let inconsistent = String::from_utf8(json::canonical(&value))
        .unwrap()
        .replace("\"unproven\"", "\"matched\"");
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

    let mut mismatched = publication_evidence();
    mismatched.docs.commit = oid('c', ObjectFormat::Sha1);
    mismatched.target.canonical_url = "https://preview.example.com/widget/".to_owned();
    let evidence = evidence_envelope(&mismatched);
    let value = assess(&valid_plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let unsorted = String::from_utf8(json::canonical(&value)).unwrap().replace(
        "[\"docs-mismatch\",\"target-mismatch\"]",
        "[\"target-mismatch\",\"docs-mismatch\"]",
    );
    let unsorted_value = json::parse(unsorted.as_bytes()).unwrap();
    let rebound = unsorted.replace(
        recorded,
        &hj(
            ASSESSMENT_PAYLOAD_SCHEMA,
            unsorted_value.member("payload").unwrap(),
        )
        .to_string(),
    );
    let error = parse_assessment(rebound.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.reasons");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);
}

#[test]
fn the_published_assessment_replays_from_its_plan_and_evidence() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let plan = parse_plan(&fs::read(examples.join("publication-plan.json")).unwrap()).unwrap();
    let evidence =
        parse_evidence(&fs::read(examples.join("publication-evidence.json")).unwrap()).unwrap();
    let published_bytes = fs::read(examples.join("publication-assessment.json")).unwrap();
    let published = parse_assessment(&published_bytes).unwrap();
    let replayed = assess(
        &plan,
        Some(&evidence),
        &published.payload.engine_version,
        published.payload.engine_digest,
    )
    .unwrap();

    assert_eq!(
        json::canonical(&replayed),
        json::canonical(&json::parse(&published_bytes).unwrap())
    );
}
