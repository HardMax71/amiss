#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid renderer contexts and coverage plans"
)]

use amiss_controller::{
    LocaleBuild, LocaleBuildContext, MdBookEvidenceError, mdbook_locale_evidence,
    mdbook_locale_producer,
};
use amiss_wire::assessment::{AssessmentVerdict, Nullable};
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    LocaleCoverageAssessment, LocaleCoverageEvidence, LocaleCoveragePlan, LocaleCoverageReason,
    LocaleLineageStatus, LocalePageRequirement, LocaleTargetOrigin, assess,
};
use amiss_wire::model::{Digest, RepoPathText};
use serde_json::json;

const PLAN: &[u8] = include_bytes!("../../../spec/examples/locale-coverage-plan.json");

fn chapter(
    path: &str,
    source_path: &str,
    content: &str,
    sub_items: &[serde_json::Value],
) -> serde_json::Value {
    json!({
        "Chapter": {
            "content": content,
            "name": "fixture",
            "number": null,
            "parent_names": [],
            "path": path,
            "source_path": source_path,
            "sub_items": sub_items
        }
    })
}

fn context(items: &[serde_json::Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "book": {"items": items},
        "config": {"book": {"src": "guide"}, "output": {"html": {}}},
        "destination": "/operator/build/site",
        "root": "/operator/checkout/docs",
        "version": "0.5.4"
    }))
    .unwrap()
}

fn locales() -> LocaleBuildContext {
    LocaleBuildContext {
        source: LocaleBuild {
            configuration: RepoPathText::new("docs/book.toml".to_owned()).unwrap(),
            locale: "en".to_owned(),
        },
        target: LocaleBuild {
            configuration: RepoPathText::new("docs/de-DE/book.toml".to_owned()).unwrap(),
            locale: "de-DE".to_owned(),
        },
    }
}

/// The spec example with the policy this producer can actually satisfy: every
/// source page required, no fallback classes, no published product.
fn plan(edit: impl FnOnce(&mut LocaleCoveragePlan)) -> Vec<u8> {
    let mut plan = LocaleCoveragePlan::parse(PLAN).unwrap().payload;
    plan.producer = mdbook_locale_producer(&locales()).unwrap();
    plan.product = Nullable::Null;
    plan.policy.required = LocalePageRequirement::AllSource {};
    plan.policy.fallbacks = Vec::new();
    plan.policy.require_target_lineage = false;
    edit(&mut plan);
    plan.emit().unwrap()
}

fn english() -> Vec<u8> {
    context(&[
        chapter(
            "index.md",
            "README.md",
            "# Widget\n",
            &[chapter(
                "guide/start.md",
                "guide/start.md",
                "# Start\n",
                &[],
            )],
        ),
        json!("Separator"),
        json!({"Chapter": {
            "content": "",
            "name": "draft",
            "number": null,
            "parent_names": [],
            "path": null,
            "source_path": null,
            "sub_items": []
        }}),
    ])
}

fn coverage(plan_bytes: &[u8], target: &[u8]) -> LocaleCoverageAssessment {
    let evidence = mdbook_locale_evidence(plan_bytes, &locales(), &english(), target).unwrap();
    let plan = LocaleCoveragePlan::parse(plan_bytes).unwrap();
    let evidence = LocaleCoverageEvidence::parse(&evidence).unwrap();
    let assessed = assess(&plan, Some(&evidence), "0.0.0-test", Digest::from([7; 32])).unwrap();
    LocaleCoverageAssessment::parse(&assessed).unwrap().payload
}

fn german(pages: &[(&str, &str, &str)]) -> Vec<u8> {
    context(
        &pages
            .iter()
            .map(|(path, source_path, content)| chapter(path, source_path, content, &[]))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn a_page_is_keyed_by_the_source_path_both_locales_share() {
    let plan_bytes = plan(|_plan| {});
    let target = german(&[
        ("index.md", "README.md", "# Widget (de)\n"),
        ("guide/start.md", "guide/start.md", "# Anfang\n"),
    ]);

    let evidence = mdbook_locale_evidence(&plan_bytes, &locales(), &english(), &target).unwrap();
    let parsed = LocaleCoverageEvidence::parse(&evidence).unwrap().payload;

    assert_eq!(
        parsed
            .source
            .pages
            .iter()
            .map(|page| page.key.as_str())
            .collect::<Vec<_>>(),
        ["README.md", "guide/start.md"]
    );
    assert_eq!(
        parsed
            .target
            .pages
            .iter()
            .map(|page| page.key.as_str())
            .collect::<Vec<_>>(),
        ["README.md", "guide/start.md"]
    );
    assert!(parsed.source.complete && parsed.target.complete);
    assert_eq!(parsed.producer, mdbook_locale_producer(&locales()).unwrap());
    assert_ne!(parsed.source.input_digest, parsed.target.input_digest);
    for (source, translated) in parsed.source.pages.iter().zip(&parsed.target.pages) {
        assert_eq!(source.key, translated.key);
        assert_ne!(source.resource_digest, translated.resource_digest);
        assert_eq!(
            translated.origin,
            LocaleTargetOrigin::TargetResource {
                based_on_source_digest: Nullable::Null
            }
        );
    }
}

#[test]
fn a_fully_translated_book_matches() {
    let assessment = coverage(
        &plan(|_plan| {}),
        &german(&[
            ("index.md", "README.md", "# Widget (de)\n"),
            ("guide/start.md", "guide/start.md", "# Anfang\n"),
        ]),
    );

    assert_eq!(assessment.verdict, AssessmentVerdict::Matched);
    assert!(assessment.coverage.complete);
    assert!(assessment.reasons.is_empty());
}

#[test]
fn an_untranslated_page_is_reported_under_its_source_path() {
    let assessment = coverage(
        &plan(|_plan| {}),
        &german(&[("index.md", "README.md", "# Widget (de)\n")]),
    );

    assert_eq!(assessment.verdict, AssessmentVerdict::Refuted);
    assert_eq!(assessment.reasons, [LocaleCoverageReason::TargetMissing]);
    assert_eq!(assessment.coverage.target_missing, ["guide/start.md"]);
}

#[test]
fn a_page_only_the_translation_carries_is_orphaned() {
    let assessment = coverage(
        &plan(|_plan| {}),
        &german(&[
            ("index.md", "README.md", "# Widget (de)\n"),
            ("guide/start.md", "guide/start.md", "# Anfang\n"),
            ("anhang.md", "anhang.md", "# Anhang\n"),
        ]),
    );

    assert_eq!(assessment.verdict, AssessmentVerdict::Refuted);
    assert_eq!(assessment.reasons, [LocaleCoverageReason::TargetOrphaned]);
    assert_eq!(assessment.coverage.target_orphaned, ["anhang.md"]);
}

#[test]
fn a_rendered_book_proves_no_translation_lineage() {
    let assessment = coverage(
        &plan(|plan| plan.policy.require_target_lineage = true),
        &german(&[
            ("index.md", "README.md", "# Widget (de)\n"),
            ("guide/start.md", "guide/start.md", "# Anfang\n"),
        ]),
    );

    assert_eq!(assessment.verdict, AssessmentVerdict::Unproven);
    assert_eq!(assessment.reasons, [LocaleCoverageReason::LineageUnproven]);
    assert!(
        assessment
            .coverage
            .lineage
            .iter()
            .all(|row| row.status == LocaleLineageStatus::Unproven)
    );
}

#[test]
fn a_plan_naming_another_producer_leaves_the_coverage_unproven() {
    let mine = mdbook_locale_producer(&locales()).unwrap();
    let assessment = coverage(
        &plan(|plan| plan.producer.version = "9.9.9".to_owned()),
        &german(&[
            ("index.md", "README.md", "# Widget (de)\n"),
            ("guide/start.md", "guide/start.md", "# Anfang\n"),
        ]),
    );

    assert_eq!(mine.version, "1.0.0");
    assert_eq!(assessment.verdict, AssessmentVerdict::Unproven);
    assert_eq!(assessment.reasons, [LocaleCoverageReason::ProducerMismatch]);
}

#[test]
fn locales_the_plan_does_not_name_refuse_before_any_page_is_read() {
    let swapped = LocaleBuildContext {
        source: locales().target,
        target: locales().source,
    };

    let refusal =
        mdbook_locale_evidence(&plan(|_plan| {}), &swapped, &english(), &english()).unwrap_err();

    assert!(matches!(refusal, MdBookEvidenceError::ContextIdentity));
}

#[test]
fn a_document_that_is_not_a_coverage_plan_refuses() {
    let refusal = mdbook_locale_evidence(b"{}", &locales(), &english(), &english()).unwrap_err();

    assert!(matches!(refusal, MdBookEvidenceError::Plan));
}

#[test]
fn two_chapters_claiming_one_source_path_refuse() {
    let target = german(&[
        ("index.md", "README.md", "# Widget (de)\n"),
        ("kopie.md", "README.md", "# Kopie\n"),
    ]);

    let refusal =
        mdbook_locale_evidence(&plan(|_plan| {}), &locales(), &english(), &target).unwrap_err();

    assert!(matches!(refusal, MdBookEvidenceError::UnsupportedBuild));
}
