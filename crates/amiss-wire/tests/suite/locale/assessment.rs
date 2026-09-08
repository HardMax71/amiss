use std::{fs, path::Path};

use super::evidence::{fallback_page, locale_evidence, page_map, set_target_page, target_page};
use super::{digest, locale_plan, oid, product_resource};
use amiss_wire::assessment::Nullable;
use amiss_wire::de::ErrorKind;

use amiss_wire::locale::{
    self, ASSESSMENT_DOCUMENT_BYTES, ASSESSMENT_PAYLOAD_SCHEMA, LocaleCoverageReason,
    LocaleCoverageVerdict, LocaleFallbackStatus, LocaleLineageStatus, LocalePageRequirement,
    LocaleProductResult, LocaleSourcePage, assess, parse_assessment, parse_evidence, parse_plan,
    plan,
};

#[test]
fn complete_inventories_report_exact_missing_and_orphan_pages() {
    let plan = plan(locale_plan()).unwrap();
    let mut input = locale_evidence();
    set_target_page(
        &mut input.target.pages,
        target_page("legacy/removed", 'b', None),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

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
        assessment.payload.subject.report_payload_digest,
        plan.payload.report_payload_digest
    );
    assert_eq!(
        assessment.payload.subject.plan_payload_digest,
        plan.payload_digest
    );
    assert_eq!(
        assessment.payload.subject.evidence_payload_digest,
        Nullable::Value(evidence.payload_digest)
    );
}

#[test]
fn partial_inventories_only_report_absences_the_other_side_proves() {
    let mut all_source = locale_plan();
    all_source.policy.required = LocalePageRequirement::AllSource;
    let all_source = plan(all_source).unwrap();

    let mut partial_source = locale_evidence();
    partial_source.plan_payload_digest = all_source.payload_digest;
    partial_source.source.complete = false;
    let evidence = locale::evidence(partial_source).unwrap();
    let assessment = assess(&all_source, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert!(!assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.target_missing,
        vec!["reference/api"]
    );
    assert!(assessment.payload.coverage.target_orphaned.is_empty());

    let plan = plan(locale_plan()).unwrap();
    let mut partial_target = locale_evidence();
    partial_target.target.complete = false;
    set_target_page(
        &mut partial_target.target.pages,
        target_page("legacy/removed", 'b', None),
    );
    let evidence = locale::evidence(partial_target).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
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
    both_partial
        .source
        .pages
        .retain(|page| page.key != "reference/api");
    let evidence = locale::evidence(both_partial).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
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
    let plan = plan(locale_plan()).unwrap();
    let mut input = locale_evidence();
    input.source.complete = false;
    input.target.pages = page_map(
        &[("guide/getting-started", 'f'), ("reference/api", 'e')],
        |key, digit| target_page(key, digit, None),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert!(assessment.payload.coverage.complete);
    assert!(assessment.payload.reasons.is_empty());
}

#[test]
fn fallback_provenance_must_match_one_authorized_class_page_and_source_digest() {
    let plan = plan(locale_plan()).unwrap();
    let mut allowed = locale_evidence();
    set_target_page(
        &mut allowed.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    let evidence = locale::evidence(allowed.clone()).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert_eq!(assessment.payload.coverage.fallbacks.len(), 1);
    assert_eq!(
        assessment.payload.coverage.fallbacks[0].status,
        LocaleFallbackStatus::Allowed
    );

    let mut unauthorized = allowed.clone();
    set_target_page(
        &mut unauthorized.target.pages,
        fallback_page("reference/api", 'b', "preview-copy", '7'),
    );
    let evidence = locale::evidence(unauthorized).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::FallbackUnauthorized]
    );
    assert_eq!(
        assessment.payload.coverage.fallbacks[0].status,
        LocaleFallbackStatus::Unauthorized
    );

    let mut wrong_page = allowed.clone();
    set_target_page(
        &mut wrong_page.target.pages,
        fallback_page("guide/getting-started", 'b', "source-copy", '6'),
    );
    let evidence = locale::evidence(wrong_page).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::FallbackUnauthorized]
    );
    assert_eq!(
        assessment.payload.coverage.fallbacks[0].status,
        LocaleFallbackStatus::Unauthorized
    );

    let mut stale = allowed;
    set_target_page(
        &mut stale.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '6'),
    );
    let evidence = locale::evidence(stale).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::FallbackSourceMismatch]
    );
    assert_eq!(
        assessment.payload.coverage.fallbacks[0].status,
        LocaleFallbackStatus::SourceMismatch
    );
}

#[test]
fn fallback_source_absence_in_a_partial_inventory_stays_unproven() {
    let plan = plan(locale_plan()).unwrap();
    let mut input = locale_evidence();
    input.source.complete = false;
    input
        .source
        .pages
        .retain(|page| page.key != "reference/api");
    set_target_page(
        &mut input.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            LocaleCoverageReason::SourceIncomplete,
            LocaleCoverageReason::FallbackUnproven,
        ]
    );
    assert!(!assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.fallbacks[0].status,
        LocaleFallbackStatus::SourceUnproven
    );
}

#[test]
fn all_source_fallback_rules_authorize_each_observed_source_page() {
    let mut input_plan = locale_plan();
    input_plan.policy.fallbacks[0].pages = LocalePageRequirement::AllSource;
    let plan = plan(input_plan).unwrap();
    let mut input = locale_evidence();
    input.plan_payload_digest = plan.payload_digest;
    set_target_page(
        &mut input.target.pages,
        fallback_page("guide/getting-started", '9', "source-copy", '6'),
    );
    set_target_page(
        &mut input.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert!(
        assessment
            .payload
            .coverage
            .fallbacks
            .iter()
            .all(|fallback| fallback.status == LocaleFallbackStatus::Allowed)
    );
}

#[test]
fn required_target_lineage_distinguishes_current_stale_and_unproven() {
    let mut input_plan = locale_plan();
    input_plan.policy.require_target_lineage = true;
    let plan = plan(input_plan).unwrap();

    let mut current = locale_evidence();
    current.plan_payload_digest = plan.payload_digest;
    set_target_page(
        &mut current.target.pages,
        target_page("guide/getting-started", '9', Some('6')),
    );
    set_target_page(
        &mut current.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    let evidence = locale::evidence(current.clone()).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert_eq!(assessment.payload.coverage.lineage.len(), 1);
    assert_eq!(
        assessment.payload.coverage.lineage[0].status,
        LocaleLineageStatus::Current
    );

    let mut stale = current.clone();
    set_target_page(
        &mut stale.target.pages,
        target_page("guide/getting-started", '9', Some('5')),
    );
    let evidence = locale::evidence(stale).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::LineageStale]
    );
    assert!(assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.lineage[0].status,
        LocaleLineageStatus::Stale
    );

    let mut unproven = current;
    set_target_page(
        &mut unproven.target.pages,
        target_page("guide/getting-started", '9', None),
    );
    let evidence = locale::evidence(unproven).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::LineageUnproven]
    );
    assert!(!assessment.payload.coverage.complete);
    assert_eq!(
        assessment.payload.coverage.lineage[0].status,
        LocaleLineageStatus::Unproven
    );
}

#[test]
fn lineage_policy_is_explicit_and_applies_outside_the_required_page_set() {
    let coverage_plan = plan(locale_plan()).unwrap();
    let mut ignored = locale_evidence();
    set_target_page(
        &mut ignored.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    set_target_page(
        &mut ignored.target.pages,
        target_page("guide/getting-started", '9', Some('5')),
    );
    let evidence = locale::evidence(ignored).unwrap();
    let assessment = assess(&coverage_plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert!(assessment.payload.coverage.lineage.is_empty());

    let mut input_plan = locale_plan();
    input_plan.policy.require_target_lineage = true;
    let plan = plan(input_plan).unwrap();
    let mut input = locale_evidence();
    input.plan_payload_digest = plan.payload_digest;
    let source_page = LocaleSourcePage {
        key: "optional/overview".to_owned(),
        resource_digest: digest('c'),
    };
    let index = input
        .source
        .pages
        .partition_point(|current| current.key < source_page.key);
    input.source.pages.insert(index, source_page);
    set_target_page(
        &mut input.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    set_target_page(
        &mut input.target.pages,
        target_page("guide/getting-started", '9', Some('6')),
    );
    set_target_page(
        &mut input.target.pages,
        target_page("optional/overview", 'd', Some('e')),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::LineageStale]
    );
    assert_eq!(assessment.payload.coverage.lineage.len(), 2);
}

#[test]
fn lineage_is_not_inferred_without_an_observed_current_source() {
    let mut input_plan = locale_plan();
    input_plan.policy.require_target_lineage = true;
    let plan = plan(input_plan).unwrap();
    let mut input = locale_evidence();
    input.plan_payload_digest = plan.payload_digest;
    input.source.complete = false;
    input
        .source
        .pages
        .retain(|page| page.key != "reference/api");
    set_target_page(
        &mut input.target.pages,
        target_page("guide/getting-started", '9', Some('6')),
    );
    set_target_page(
        &mut input.target.pages,
        target_page("reference/api", 'b', Some('7')),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::SourceIncomplete]
    );
    assert_eq!(assessment.payload.coverage.lineage.len(), 1);
    assert_eq!(
        assessment.payload.coverage.lineage[0].status,
        LocaleLineageStatus::Current
    );
}

#[test]
fn product_alignment_compares_each_locale_to_one_exact_planned_resource() {
    let mut input_plan = locale_plan();
    input_plan.product = Nullable::Value(product_resource('c'));
    let plan = plan(input_plan).unwrap();
    let mut aligned = locale_evidence();
    aligned.plan_payload_digest = plan.payload_digest;
    aligned.source.product = Nullable::Value(product_resource('c'));
    aligned.target.product = Nullable::Value(product_resource('c'));
    set_target_page(
        &mut aligned.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );

    let evidence = locale::evidence(aligned.clone()).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    let Nullable::Value(product) = assessment.payload.product else {
        panic!("selected product comparison was omitted");
    };
    assert_eq!(product.source, LocaleCoverageVerdict::Matched);
    assert_eq!(product.target, LocaleCoverageVerdict::Matched);

    let mut missing = aligned.clone();
    missing.source.product = Nullable::Null;
    let evidence = locale::evidence(missing.clone()).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::SourceProductUnproven]
    );
    assert!(assessment.payload.coverage.complete);

    let mut mismatched = missing;
    mismatched.source.product = Nullable::Value(product_resource('d'));
    mismatched.target.product = Nullable::Null;
    let evidence = locale::evidence(mismatched.clone()).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::SourceProductMismatch]
    );
    assert!(matches!(
        assessment.payload.product,
        Nullable::Value(product) if product.target == LocaleCoverageVerdict::Unproven
    ));

    mismatched.target.product = Nullable::Value(product_resource('e'));
    let evidence = locale::evidence(mismatched).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(
        assessment.payload.reasons,
        vec![
            LocaleCoverageReason::SourceProductMismatch,
            LocaleCoverageReason::TargetProductMismatch,
        ]
    );
}

#[test]
fn coverage_only_policy_ignores_unselected_product_receipts() {
    let plan = plan(locale_plan()).unwrap();
    let mut input = locale_evidence();
    input.source.product = Nullable::Value(product_resource('c'));
    input.target.product = Nullable::Value(product_resource('d'));
    set_target_page(
        &mut input.target.pages,
        fallback_page("reference/api", 'b', "source-copy", '7'),
    );
    let evidence = locale::evidence(input).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    assert_eq!(assessment.payload.product, Nullable::Null);
}

#[test]
fn all_source_and_named_source_absence_remain_distinct() {
    let mut all_source_plan = locale_plan();
    all_source_plan.policy.required = LocalePageRequirement::AllSource;
    let all_source_plan = plan(all_source_plan).unwrap();
    let mut all_source_evidence = locale_evidence();
    all_source_evidence.plan_payload_digest = all_source_plan.payload_digest;
    all_source_evidence.target.pages = page_map(
        &[("guide/getting-started", '9'), ("legacy/removed", 'a')],
        |key, digit| target_page(key, digit, None),
    );
    let evidence = locale::evidence(all_source_evidence).unwrap();
    let assessment = assess(&all_source_plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(
        assessment.payload.coverage.target_missing,
        vec!["reference/api"]
    );
    assert_eq!(
        assessment.payload.coverage.target_orphaned,
        vec!["legacy/removed"]
    );

    let plan = plan(locale_plan()).unwrap();
    let mut source_missing = locale_evidence();
    source_missing
        .source
        .pages
        .retain(|page| page.key != "reference/api");
    source_missing.target.pages = page_map(&[("guide/getting-started", '9')], |key, digit| {
        target_page(key, digit, None)
    });
    let evidence = locale::evidence(source_missing).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
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
    let plan = plan(locale_plan()).unwrap();
    let absent = assess(&plan, None, "0.26.0", digest('a')).unwrap();
    assert_eq!(absent.payload.verdict, LocaleCoverageVerdict::Unproven);
    assert_eq!(
        absent.payload.reasons,
        vec![LocaleCoverageReason::EvidenceAbsent]
    );
    assert_eq!(
        absent.payload.subject.evidence_payload_digest,
        Nullable::Null
    );

    let mut unbound = locale_evidence();
    unbound.plan_payload_digest = digest('f');
    let evidence = locale::evidence(unbound).unwrap();
    assert_eq!(
        assess(&plan, Some(&evidence), "0.26.0", digest('a'))
            .unwrap()
            .payload
            .reasons,
        vec![LocaleCoverageReason::EvidenceUnbound]
    );

    let mut foreign = locale_evidence();
    foreign.producer.context_digest = digest('e');
    foreign.target.pages.clear();
    let evidence = locale::evidence(foreign).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::ProducerMismatch]
    );
    assert!(assessment.payload.coverage.target_missing.is_empty());
}

#[test]
fn bound_fact_disagreements_refute_without_comparing_foreign_inventories() {
    let plan = plan(locale_plan()).unwrap();
    let mut foreign = locale_evidence();
    foreign.docs.commit = oid('c');
    foreign.scope.target_locale = "fr".to_owned();
    foreign.target.pages.clear();
    let evidence = locale::evidence(foreign).unwrap();
    let assessment = assess(&plan, Some(&evidence), "0.26.0", digest('a')).unwrap();

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
    let mut mutated_plan = plan(locale_plan()).unwrap();
    mutated_plan.payload_digest = digest('f');
    let error = assess(&mutated_plan, None, "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.plan.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let valid_plan = plan(locale_plan()).unwrap();
    let mut mutated_evidence = locale::evidence(locale_evidence()).unwrap();
    mutated_evidence.payload_digest = digest('f');
    let error = assess(&valid_plan, Some(&mutated_evidence), "0.26.0", digest('a')).unwrap_err();
    assert_eq!(error.path, "$.evidence.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let evidence = locale::evidence(locale_evidence()).unwrap();
    let document = assess(&valid_plan, Some(&evidence), "0.26.0", digest('a')).unwrap();
    let mut inconsistent = document.clone();
    inconsistent.payload.verdict = LocaleCoverageVerdict::Matched;
    let mut inconsistent_product = document.clone();
    inconsistent_product.payload.product = Nullable::Value(LocaleProductResult {
        source: LocaleCoverageVerdict::Refuted,
        target: LocaleCoverageVerdict::Matched,
    });
    let mut unsorted = document;
    unsorted.payload.coverage.target_missing =
        vec!["reference/z".to_owned(), "reference/a".to_owned()];
    for (mut document, path, kind) in [
        (inconsistent, "$.payload", ErrorKind::Inconsistent),
        (inconsistent_product, "$.payload", ErrorKind::Inconsistent),
        (
            unsorted,
            "$.payload.coverage.target_missing",
            ErrorKind::UnsortedSet,
        ),
    ] {
        document.payload_digest = amiss_wire::digest::hb(
            ASSESSMENT_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let mut bytes = Vec::new();
        amiss_wire::write_json(&document, &mut bytes, ASSESSMENT_DOCUMENT_BYTES).unwrap();
        let error = parse_assessment(&bytes).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, kind);
    }
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
        &published.payload.engine.engine_version,
        published.payload.engine.engine_digest,
    )
    .unwrap();

    assert_eq!(replayed, published);
}
