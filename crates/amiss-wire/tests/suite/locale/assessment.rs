#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "tests replay checked locale contracts and mutate their canonical JSON"
)]

use std::{fs, path::Path};

use super::evidence::{inventory, locale_evidence};
use super::{digest, locale_plan, oid};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json::{self, Value};
use amiss_wire::locale::{
    ASSESSMENT_PAYLOAD_SCHEMA, LocaleCoverageAssessmentEnvelope, LocaleCoverageEvidence,
    LocaleCoverageEvidenceEnvelope, LocaleCoverageReason, LocaleCoverageVerdict,
    LocalePageRequirement, assess, evidence, parse_assessment, parse_evidence, parse_plan, plan,
};

fn plan_envelope() -> amiss_wire::locale::LocaleCoveragePlanEnvelope {
    let value = plan(&locale_plan()).unwrap();
    parse_plan(&json::canonical(&value)).unwrap()
}

fn evidence_envelope(input: &LocaleCoverageEvidence) -> LocaleCoverageEvidenceEnvelope {
    let value = evidence(input).unwrap();
    parse_evidence(&json::canonical(&value)).unwrap()
}

fn assessed(
    plan: &amiss_wire::locale::LocaleCoveragePlanEnvelope,
    evidence: Option<&LocaleCoverageEvidenceEnvelope>,
) -> LocaleCoverageAssessmentEnvelope {
    let value = assess(plan, evidence, "0.26.0", digest('a')).unwrap();
    parse_assessment(&json::canonical(&value)).unwrap()
}

#[test]
fn complete_inventories_report_exact_missing_and_orphan_pages() {
    let plan = plan_envelope();
    let mut input = locale_evidence();
    input
        .target
        .pages
        .insert("legacy/removed".to_owned(), digest('b'));
    let evidence = evidence_envelope(&input);
    let assessment = assessed(&plan, Some(&evidence));

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            LocaleCoverageReason::TargetMissing,
            LocaleCoverageReason::TargetOrphaned,
        ]
    );
    assert!(assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.target_missing,
        vec!["reference/api"]
    );
    assert_eq!(
        assessment.payload.coverage.target_orphaned,
        vec!["legacy/removed"]
    );
    assert!(assessment.payload.coverage.source_missing.is_empty());
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
fn partial_inventories_only_report_absences_the_other_side_proves() {
    let mut all_source = locale_plan();
    all_source.policy.required = LocalePageRequirement::AllSource;
    let value = plan(&all_source).unwrap();
    let all_source = parse_plan(&json::canonical(&value)).unwrap();

    let mut partial_source = locale_evidence();
    partial_source.plan_payload_digest = all_source.payload_digest;
    partial_source.source.complete = false;
    let evidence = evidence_envelope(&partial_source);
    let assessment = assessed(&all_source, Some(&evidence));
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert!(!assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.target_missing,
        vec!["reference/api"]
    );
    assert!(assessment.payload.coverage.target_orphaned.is_empty());

    let plan = plan_envelope();
    let mut partial_target = locale_evidence();
    partial_target.target.complete = false;
    partial_target
        .target
        .pages
        .insert("legacy/removed".to_owned(), digest('b'));
    let evidence = evidence_envelope(&partial_target);
    let assessment = assessed(&plan, Some(&evidence));
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert!(!assessment.payload.coverage.complete);
    assert!(assessment.payload.coverage.target_missing.is_empty());
    assert_eq!(
        assessment.payload.coverage.target_orphaned,
        vec!["legacy/removed"]
    );

    let mut both_partial = locale_evidence();
    both_partial.source.complete = false;
    both_partial.target.complete = false;
    both_partial.source.pages.remove("reference/api");
    let evidence = evidence_envelope(&both_partial);
    let assessment = assessed(&plan, Some(&evidence));
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            LocaleCoverageReason::SourceIncomplete,
            LocaleCoverageReason::TargetIncomplete,
        ]
    );
}

#[test]
fn named_policy_can_be_exhaustive_without_an_unneeded_full_source_inventory() {
    let plan = plan_envelope();
    let mut input = locale_evidence();
    input.source.complete = false;
    input.target.pages = input.source.pages.clone();
    input
        .target
        .pages
        .insert("guide/getting-started".to_owned(), digest('f'));
    let evidence = evidence_envelope(&input);
    let assessment = assessed(&plan, Some(&evidence));

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert!(assessment.payload.coverage.complete);
    assert!(assessment.payload.reasons.is_empty());
}

#[test]
fn all_source_and_named_source_absence_remain_distinct() {
    let mut all_source_plan = locale_plan();
    all_source_plan.policy.required = LocalePageRequirement::AllSource;
    let value = plan(&all_source_plan).unwrap();
    let all_source_plan = parse_plan(&json::canonical(&value)).unwrap();
    let mut all_source_evidence = locale_evidence();
    all_source_evidence.plan_payload_digest = all_source_plan.payload_digest;
    all_source_evidence.target.pages = inventory(
        '8',
        &[("guide/getting-started", '9'), ("legacy/removed", 'a')],
    )
    .pages;
    let evidence = evidence_envelope(&all_source_evidence);
    let assessment = assessed(&all_source_plan, Some(&evidence));
    assert_eq!(
        assessment.payload.coverage.target_missing,
        vec!["reference/api"]
    );
    assert_eq!(
        assessment.payload.coverage.target_orphaned,
        vec!["legacy/removed"]
    );

    let plan = plan_envelope();
    let mut source_missing = locale_evidence();
    source_missing.source.pages.remove("reference/api");
    source_missing.target.pages = source_missing.source.pages.clone();
    let evidence = evidence_envelope(&source_missing);
    let assessment = assessed(&plan, Some(&evidence));
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::SourceMissing]
    );
    assert_eq!(
        assessment.payload.coverage.source_missing,
        vec!["reference/api"]
    );
    assert!(assessment.payload.coverage.target_missing.is_empty());
}

#[test]
fn absent_unbound_and_foreign_producer_evidence_stays_unproven() {
    let plan = plan_envelope();
    let absent = assessed(&plan, None);
    assert_eq!(absent.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        absent.payload.reasons,
        vec![LocaleCoverageReason::EvidenceAbsent]
    );
    assert_eq!(absent.payload.evidence_payload_digest, None);

    let mut unbound = locale_evidence();
    unbound.plan_payload_digest = digest('f');
    let evidence = evidence_envelope(&unbound);
    assert_eq!(
        assessed(&plan, Some(&evidence)).payload.reasons,
        vec![LocaleCoverageReason::EvidenceUnbound]
    );

    let mut foreign = locale_evidence();
    foreign.producer.context_digest = digest('e');
    foreign.target.pages.clear();
    let evidence = evidence_envelope(&foreign);
    let assessment = assessed(&plan, Some(&evidence));
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::ProducerMismatch]
    );
    assert!(assessment.payload.coverage.target_missing.is_empty());
}

#[test]
fn bound_fact_disagreements_refute_without_comparing_foreign_inventories() {
    let plan = plan_envelope();
    let mut foreign = locale_evidence();
    foreign.docs.commit = oid('c');
    foreign.scope.target_locale = "fr".to_owned();
    foreign.target.pages.clear();
    let evidence = evidence_envelope(&foreign);
    let assessment = assessed(&plan, Some(&evidence));

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            LocaleCoverageReason::DocsMismatch,
            LocaleCoverageReason::ScopeMismatch,
        ]
    );
    assert!(!assessment.payload.coverage.complete);
    assert!(assessment.payload.coverage.target_missing.is_empty());
}

#[test]
fn assessment_refuses_mutated_envelopes_and_inconsistent_or_unsorted_results() {
    let mut plan = plan_envelope();
    plan.payload_digest = digest('f');
    let error = assess(&plan, None, "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.plan.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let valid_plan = plan_envelope();
    let mut mutated_evidence = evidence_envelope(&locale_evidence());
    mutated_evidence.payload_digest = digest('f');
    let error = assess(&valid_plan, Some(&mutated_evidence), "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.evidence.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let evidence = evidence_envelope(&locale_evidence());
    let value = assess(&valid_plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let inconsistent = String::from_utf8(json::canonical(&value))
        .unwrap()
        .replace("\"refuted\"", "\"matched\"");
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

    let mut unsorted = value;
    let target_missing = member_mut(member_mut(&mut unsorted, "payload"), "coverage");
    *member_mut(target_missing, "target_missing") = Value::array(vec![
        Value::string("reference/z"),
        Value::string("reference/a"),
    ]);
    let error = parse_assessment(&sealed(unsorted)).unwrap_err();
    assert_eq!(error.path, "$.payload.coverage.target_missing");
    assert_eq!(error.kind, ErrorKind::UnsortedSet);
}

#[test]
fn the_published_assessment_replays_from_its_plan_and_evidence() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let plan = parse_plan(&fs::read(examples.join("locale-coverage-plan.json")).unwrap()).unwrap();
    let evidence =
        parse_evidence(&fs::read(examples.join("locale-coverage-evidence.json")).unwrap()).unwrap();
    let published_bytes = fs::read(examples.join("locale-coverage-assessment.json")).unwrap();
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

fn sealed(mut value: Value) -> Vec<u8> {
    let digest = hj(ASSESSMENT_PAYLOAD_SCHEMA, value.member("payload").unwrap());
    *member_mut(&mut value, "payload_digest") = Value::string(digest.to_string());
    json::canonical(&value)
}

fn member_mut<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    let Value::Object(members) = value else {
        panic!("the checked writer produced a non-object value");
    };
    members
        .iter_mut()
        .find(|(key, _value)| key == name)
        .map(|(_key, value)| value)
        .unwrap()
}
