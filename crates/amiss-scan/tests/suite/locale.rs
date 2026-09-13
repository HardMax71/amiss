#![expect(
    clippy::unwrap_used,
    reason = "the fixture builds known-valid repositories and coverage plans"
)]

use std::path::Path;

use amiss_fixtures::{CommitChain, Staged, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::locale::{
    InventoryError, LocaleSide, LocaleTreeContext, SOURCE_IDENTICAL_CLASS, tree_inventory,
    tree_producer,
};
use amiss_wire::assessment::{AssessmentVerdict, Nullable};
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    LocaleCoverageAssessment, LocaleCoverageEvidence, LocaleCoveragePlan, LocaleCoverageReason,
    LocaleFallbackRule, LocaleFallbackStatus, LocalePageRequirement, LocaleTargetOrigin, assess,
};
use amiss_wire::model::{ArtifactId, Digest, ObjectFormat, RepoPathText};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/locale-coverage-plan.json");

fn side(root: &str, locale: &str, suffix: Option<&str>) -> LocaleSide {
    LocaleSide {
        root: RepoPathText::new(root.to_owned()).unwrap(),
        locale: locale.to_owned(),
        suffix: suffix.map(str::to_owned),
    }
}

/// Locale directories, the layout `Docusaurus`, `VitePress` and Starlight use.
fn directories() -> LocaleTreeContext {
    LocaleTreeContext {
        source: side("docs", "en", None),
        target: side("docs/de-DE", "de-DE", None),
        documents: vec![".md".to_owned()],
    }
}

fn head(chain: &CommitChain) -> &amiss_fixtures::Commit {
    chain.commits.first().unwrap()
}

fn plan(
    chain: &CommitChain,
    context: &LocaleTreeContext,
    edit: impl FnOnce(&mut LocaleCoveragePlan),
) -> Vec<u8> {
    let mut plan = LocaleCoveragePlan::parse(PLAN).unwrap().payload;
    plan.docs.object_format = ObjectFormat::Sha1;
    plan.docs.commit = head(chain).id.parse().unwrap();
    plan.docs.tree = head(chain).tree.parse().unwrap();
    plan.producer = tree_producer(context).unwrap();
    plan.product = Nullable::Null;
    plan.policy.required = LocalePageRequirement::AllSource {};
    plan.policy.fallbacks = Vec::new();
    plan.policy.require_target_lineage = false;
    edit(&mut plan);
    plan.emit().unwrap()
}

fn inventory(chain: &CommitChain, context: &LocaleTreeContext, plan: &[u8]) -> Vec<u8> {
    produce(chain, context, plan).unwrap()
}

fn produce(
    chain: &CommitChain,
    context: &LocaleTreeContext,
    plan: &[u8],
) -> Result<Vec<u8>, InventoryError> {
    let repo = Repository::open(Path::new(&chain.repo), ObjectFormat::Sha1).unwrap();
    let mut git = GitResources::new(GitLimits::CONTRACT);
    tree_inventory(&repo, &mut git, plan, context)
}

fn coverage(plan: &[u8], evidence: &[u8]) -> LocaleCoverageAssessment {
    let plan = LocaleCoveragePlan::parse(plan).unwrap();
    let evidence = LocaleCoverageEvidence::parse(evidence).unwrap();
    let assessed = assess(&plan, Some(&evidence), "0.0.0-test", Digest::from([7; 32])).unwrap();
    LocaleCoverageAssessment::parse(&assessed).unwrap().payload
}

fn keys(evidence: &[u8]) -> (Vec<String>, Vec<String>) {
    let parsed = LocaleCoverageEvidence::parse(evidence).unwrap().payload;
    (
        parsed
            .source
            .pages
            .iter()
            .map(|page| page.key.clone())
            .collect(),
        parsed
            .target
            .pages
            .iter()
            .map(|page| page.key.clone())
            .collect(),
    )
}

#[test]
fn a_locale_directory_keys_every_page_under_its_own_root() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/guide/start.md", Staged::File(b"# Start\n")),
        ("docs/de-DE/index.md", Staged::File(b"# Widget (de)\n")),
        ("docs/logo.png", Staged::File(b"\x89PNG")),
        ("README.md", Staged::File(b"# Repository\n")),
    ])
    .unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |_plan| {});

    let evidence = inventory(&chain, &context, &plan);

    assert_eq!(
        keys(&evidence),
        (
            vec!["guide/start.md".to_owned(), "index.md".to_owned()],
            vec!["index.md".to_owned()]
        )
    );
    let assessment = coverage(&plan, &evidence);
    assert_eq!(assessment.verdict, AssessmentVerdict::Refuted);
    assert_eq!(assessment.reasons, [LocaleCoverageReason::TargetMissing]);
    assert_eq!(assessment.coverage.target_missing, ["guide/start.md"]);
}

#[test]
fn a_locale_suffix_in_the_filename_keys_the_same_page() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/index.de-DE.md", Staged::File(b"# Widget (de)\n")),
    ])
    .unwrap();
    let context = LocaleTreeContext {
        source: side("docs", "en", None),
        target: side("docs", "de-DE", Some("de-DE")),
        documents: vec![".md".to_owned()],
    };
    let plan = plan(&chain, &context, |_plan| {});

    let evidence = inventory(&chain, &context, &plan);

    assert_eq!(
        keys(&evidence),
        (vec!["index.md".to_owned()], vec!["index.md".to_owned()])
    );
    assert_eq!(
        coverage(&plan, &evidence).verdict,
        AssessmentVerdict::Matched
    );
}

#[test]
fn an_untranslated_copy_is_a_fallback_the_plan_has_to_authorize() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/de-DE/index.md", Staged::File(b"# Widget\n")),
    ])
    .unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |_plan| {});

    let evidence = inventory(&chain, &context, &plan);

    let parsed = LocaleCoverageEvidence::parse(&evidence).unwrap().payload;
    let page = parsed.target.pages.first().unwrap();
    assert_eq!(
        page.origin,
        LocaleTargetOrigin::Fallback {
            class: ArtifactId::new(SOURCE_IDENTICAL_CLASS.to_owned()).unwrap(),
            source_resource_digest: page.resource_digest,
        }
    );
    let assessment = coverage(&plan, &evidence);
    assert_eq!(assessment.verdict, AssessmentVerdict::Refuted);
    assert_eq!(
        assessment.reasons,
        [LocaleCoverageReason::FallbackUnauthorized]
    );
}

#[test]
fn an_authorized_identical_page_carries_the_whole_locale() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/de-DE/index.md", Staged::File(b"# Widget\n")),
    ])
    .unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |plan| {
        plan.policy.fallbacks = vec![LocaleFallbackRule {
            class: ArtifactId::new(SOURCE_IDENTICAL_CLASS.to_owned()).unwrap(),
            pages: LocalePageRequirement::AllSource {},
        }];
    });

    let assessment = coverage(&plan, &inventory(&chain, &context, &plan));

    assert_eq!(assessment.verdict, AssessmentVerdict::Matched);
    assert_eq!(
        assessment.coverage.fallbacks.first().map(|row| row.status),
        Some(LocaleFallbackStatus::Allowed)
    );
}

#[test]
fn a_symlinked_page_leaves_its_side_incomplete() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/de-DE/index.md", Staged::Symlink("../index.md")),
    ])
    .unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |_plan| {});

    let parsed = LocaleCoverageEvidence::parse(&inventory(&chain, &context, &plan))
        .unwrap()
        .payload;

    assert!(parsed.source.complete);
    assert!(!parsed.target.complete);
    assert!(parsed.target.pages.is_empty());
}

#[test]
fn a_plan_bound_to_another_commit_refuses() {
    let chain = staged_repository(&[("docs/index.md", Staged::File(b"# Widget\n"))]).unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |plan| {
        plan.docs.tree = "b".repeat(40).parse().unwrap();
    });

    assert_eq!(produce(&chain, &context, &plan), Err(InventoryError::Plan));
}

#[test]
fn a_context_the_plan_does_not_name_refuses() {
    let chain = staged_repository(&[("docs/index.md", Staged::File(b"# Widget\n"))]).unwrap();
    let context = directories();
    let plan = plan(&chain, &context, |_plan| {});
    let swapped = LocaleTreeContext {
        source: side("docs", "de-DE", None),
        target: side("docs/de-DE", "en", None),
        documents: vec![".md".to_owned()],
    };

    assert_eq!(
        produce(&chain, &swapped, &plan),
        Err(InventoryError::Context)
    );
}
