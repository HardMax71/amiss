#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
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
    LocaleFallbackRule, LocaleFallbackStatus, LocaleLineageStatus, LocalePageRequirement,
    LocaleTargetOrigin, assess,
};
use amiss_wire::model::{Digest, ObjectFormat, RepoPathText};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/locale-coverage-plan.json");
const CHAPTER: &str = include_str!("../../../../docs/src/locale-coverage.md");

fn side(root: &str, locale: &str, suffix: Option<&str>) -> LocaleSide {
    LocaleSide {
        root: RepoPathText::try_from(root.to_owned()).unwrap(),
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
        excluded: None,
        lineage: None,
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

/// A translation written in another page suffix than its source is the same
/// page, and a dotted name under a locale directory is a page like any other;
/// only a shared root makes a dotted name another locale's.
#[test]
fn a_page_keys_across_suffixes_and_keeps_dotted_names() {
    let chain = staged_repository(&[
        ("docs/guide.mdx", Staged::File(b"# Guide\n")),
        ("docs/v1.2-notes.md", Staged::File(b"# Notes\n")),
        ("docs/de-DE/guide.md", Staged::File(b"# Anleitung\n")),
        ("docs/de-DE/v1.2-notes.md", Staged::File(b"# Hinweise\n")),
    ])
    .unwrap();
    let context = LocaleTreeContext {
        documents: vec![".md".to_owned(), ".mdx".to_owned()],
        ..directories()
    };
    let plan = plan(&chain, &context, |_plan| {});

    let evidence = inventory(&chain, &context, &plan);

    assert_eq!(
        keys(&evidence),
        (
            vec!["guide.mdx".to_owned(), "v1.2-notes.md".to_owned()],
            vec!["guide.mdx".to_owned(), "v1.2-notes.md".to_owned()]
        )
    );
    assert_eq!(
        coverage(&plan, &evidence).verdict,
        AssessmentVerdict::Matched
    );
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
        excluded: None,
        lineage: None,
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
            class: SOURCE_IDENTICAL_CLASS,
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
            class: SOURCE_IDENTICAL_CLASS,
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
        excluded: None,
        lineage: None,
    };

    assert_eq!(
        produce(&chain, &swapped, &plan),
        Err(InventoryError::Context)
    );
}

/// A site that keeps its source locale at the content root holds every other
/// locale under it, so the roots the context excludes belong to neither side,
/// and an excluded list out of byte order is refused.
#[test]
fn an_excluded_locale_root_belongs_to_neither_side() {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/de-DE/index.md", Staged::File(b"# Widget (de)\n")),
        ("docs/fr/index.md", Staged::File(b"# Widget (fr)\n")),
        ("docs/fr/only.md", Staged::File(b"# Seulement\n")),
        ("docs/ja/index.md", Staged::File(b"# Widget (ja)\n")),
    ])
    .unwrap();
    let root = |path: &str| RepoPathText::try_from(path.to_owned()).unwrap();
    let context = LocaleTreeContext {
        excluded: Some(vec![root("docs/fr"), root("docs/ja")]),
        ..directories()
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
    let unsorted = LocaleTreeContext {
        excluded: Some(vec![root("docs/ja"), root("docs/fr")]),
        ..directories()
    };
    let unsorted_plan = self::plan(&chain, &unsorted, |_plan| {});
    assert_eq!(
        produce(&chain, &unsorted, &unsorted_plan),
        Err(InventoryError::Context)
    );
}

/// Every context the locale chapter prints, in the order it prints them.
fn documented() -> Vec<LocaleTreeContext> {
    CHAPTER
        .split("```json")
        .skip(1)
        .filter_map(|block| block.split("```").next())
        .map(|block| {
            serde_json::from_str(block)
                .expect("every JSON block in the locale chapter is a locale context")
        })
        .collect()
}

#[test]
fn the_documented_contexts_key_the_layouts_they_claim() {
    let directories = staged_repository(&[
        ("docs/guide/start.md", Staged::File(b"# Start\n")),
        ("docs/index.mdx", Staged::File(b"# Widget\n")),
        ("docs/logo.png", Staged::File(b"\x89PNG")),
        (
            "i18n/de-DE/docusaurus-plugin-content-docs/current/guide/start.md",
            Staged::File(b"# Anfang\n"),
        ),
    ])
    .unwrap();
    let filenames = staged_repository(&[
        ("docs/guide/start.md", Staged::File(b"# Start\n")),
        ("docs/guide/start.de-DE.md", Staged::File(b"# Anfang\n")),
    ])
    .unwrap();
    let root_locale = staged_repository(&[
        ("src/content/docs/index.mdx", Staged::File(b"# Widget\n")),
        (
            "src/content/docs/de/index.mdx",
            Staged::File(b"# Widget (de)\n"),
        ),
        (
            "src/content/docs/fr/index.mdx",
            Staged::File(b"# Widget (fr)\n"),
        ),
        (
            "src/content/docs/ja/guide.md",
            Staged::File(b"# Guide (ja)\n"),
        ),
    ])
    .unwrap();
    let claimed = [
        (vec!["guide/start.md", "index.mdx"], vec!["guide/start.md"]),
        (vec!["guide/start.md"], vec!["guide/start.md"]),
        (vec!["index.mdx"], vec!["index.mdx"]),
    ];
    assert_eq!(documented().len(), claimed.len());

    for ((context, chain), (source, target)) in documented()
        .into_iter()
        .zip([directories, filenames, root_locale])
        .zip(claimed)
    {
        let plan = plan(&chain, &context, |_plan| {});
        let owned = |keys: Vec<&str>| keys.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            keys(&inventory(&chain, &context, &plan)),
            (owned(source), owned(target))
        );
    }
}

/// A translation that records the source commit it was made from carries
/// that commit's source page as its lineage, so the assessment can tell a
/// translation made from the page as it stands from one the source moved on
/// from. A page naming no commit, or one the store does not hold, stays
/// unproven rather than current.
#[test]
fn a_translation_names_the_source_commit_it_was_made_from() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::TempDir::new()?;
    let root = dir.path();
    let git = |args: &[&str]| amiss_fixtures::git(root, args);
    git(&["init", "-q"])?;
    std::fs::create_dir_all(root.join("docs/de-DE"))?;
    std::fs::write(root.join("docs/index.md"), "# Home\n")?;
    std::fs::write(root.join("docs/guide.md"), "# Guide\n")?;
    git(&["add", "."])?;
    git(&["commit", "-qm", "source"])?;
    let made_from = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let translated = |title: &str, commit: &str| {
        format!("---\ntitle: {title}\nl10n:\n  sourceCommit: {commit}\n---\n# {title}\n")
    };
    std::fs::write(root.join("docs/guide.md"), "# Guide\n\nA new step.\n")?;
    std::fs::write(
        root.join("docs/de-DE/index.md"),
        translated("Start", &made_from),
    )?;
    std::fs::write(
        root.join("docs/de-DE/guide.md"),
        translated("Anleitung", &made_from),
    )?;
    std::fs::create_dir_all(root.join("docs/de-DE/more"))?;
    std::fs::write(root.join("docs/more.md"), "# More\n")?;
    std::fs::write(root.join("docs/de-DE/more.md"), "# Mehr\n")?;
    git(&["add", "."])?;
    git(&["commit", "-qm", "translated"])?;
    let commit = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let tree = git(&["rev-parse", "HEAD^{tree}"])?.trim().to_owned();

    let context = LocaleTreeContext {
        lineage: Some(vec!["l10n".to_owned(), "sourceCommit".to_owned()]),
        ..directories()
    };
    let mut plan = LocaleCoveragePlan::parse(PLAN)?.payload;
    plan.docs.object_format = ObjectFormat::Sha1;
    plan.docs.commit = commit.parse()?;
    plan.docs.tree = tree.parse()?;
    plan.producer = tree_producer(&context).map_err(|defect| format!("{defect:?}"))?;
    plan.product = Nullable::Null;
    plan.policy.required = LocalePageRequirement::AllSource {};
    plan.policy.fallbacks = Vec::new();
    plan.policy.require_target_lineage = true;
    let plan = plan.emit()?;
    let repo =
        Repository::open(root, ObjectFormat::Sha1).map_err(|defect| format!("{defect:?}"))?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let evidence = tree_inventory(&repo, &mut resources, &plan, &context)
        .map_err(|defect| format!("{defect:?}"))?;

    let lineage: Vec<(String, LocaleLineageStatus)> = coverage(&plan, &evidence)
        .coverage
        .lineage
        .into_iter()
        .map(|row| (row.key, row.status))
        .collect();
    assert_eq!(
        lineage,
        [
            ("guide.md".to_owned(), LocaleLineageStatus::Stale),
            ("index.md".to_owned(), LocaleLineageStatus::Current),
            ("more.md".to_owned(), LocaleLineageStatus::Unproven),
        ]
    );
    Ok(())
}
