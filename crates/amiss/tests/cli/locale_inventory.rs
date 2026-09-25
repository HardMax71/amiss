#![expect(clippy::unwrap_used, reason = "test fixture plumbing")]

use std::path::Path;

use amiss_fixtures::{CommitChain, Staged, staged_repository};
use amiss_scan::{LocaleSide, LocaleTreeContext, tree_producer};
use amiss_wire::assessment::Nullable;
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{LocaleCoveragePlan, LocalePageRequirement};
use amiss_wire::model::{ObjectFormat, RepoPathText};

use crate::support;

const PLAN: &str = "../../spec/examples/locale-coverage-plan.json";

fn side(root: &str, locale: &str) -> LocaleSide {
    LocaleSide {
        root: RepoPathText::try_from(root.to_owned()).unwrap(),
        locale: locale.to_owned(),
        suffix: None,
    }
}

fn context() -> LocaleTreeContext {
    LocaleTreeContext {
        source: side("docs", "en"),
        target: side("docs/de-DE", "de-DE"),
        documents: vec![".md".to_owned()],
        excluded: None,
        lineage: None,
    }
}

/// One checkout, one plan bound to its commit, one layout, all on disk.
fn staged() -> (CommitChain, tempfile::TempDir) {
    let chain = staged_repository(&[
        ("docs/index.md", Staged::File(b"# Widget\n")),
        ("docs/guide/start.md", Staged::File(b"# Start\n")),
        ("docs/de-DE/index.md", Staged::File(b"# Widget (de)\n")),
    ])
    .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let head = chain.commits.first().unwrap();
    let mut plan = LocaleCoveragePlan::parse(&std::fs::read(PLAN).unwrap())
        .unwrap()
        .payload;
    plan.docs.object_format = ObjectFormat::Sha1;
    plan.docs.commit = head.id.parse().unwrap();
    plan.docs.tree = head.tree.parse().unwrap();
    plan.producer = tree_producer(&context()).unwrap();
    plan.product = Nullable::Null;
    plan.policy.required = LocalePageRequirement::AllSource {};
    plan.policy.fallbacks = Vec::new();
    plan.policy.require_target_lineage = false;
    std::fs::write(scratch.path().join("plan.json"), plan.emit().unwrap()).unwrap();
    std::fs::write(
        scratch.path().join("context.json"),
        serde_json::to_vec(&context()).unwrap(),
    )
    .unwrap();
    (chain, scratch)
}

fn shown(root: &Path, name: &str) -> String {
    root.join(name).to_str().unwrap().to_owned()
}

#[test]
fn a_checkout_becomes_evidence_the_coverage_assessment_reads() {
    let (chain, scratch) = staged();
    let plan = shown(scratch.path(), "plan.json");
    let context = shown(scratch.path(), "context.json");

    let (code, stdout, stderr) = support::amiss(&[
        "locale-inventory",
        "--repo",
        &chain.repo,
        "--plan",
        &plan,
        "--context",
        &context,
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = support::payload(&stdout);
    assert_eq!(payload["source"]["pages"].as_array().unwrap().len(), 2);
    assert_eq!(payload["target"]["pages"][0]["key"], "index.md");

    let evidence = shown(scratch.path(), "evidence.json");
    std::fs::write(&evidence, &stdout).unwrap();
    let (code, stdout, stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        &plan,
        "--evidence",
        &evidence,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let payload = support::payload(&stdout);
    assert_eq!(payload["verdict"], "refuted");
    assert_eq!(payload["coverage"]["target_missing"][0], "guide/start.md");
}

#[test]
fn the_human_projection_counts_each_locale() {
    let (chain, scratch) = staged();

    let (code, stdout, stderr) = support::amiss(&[
        "locale-inventory",
        "--repo",
        &chain.repo,
        "--plan",
        &shown(scratch.path(), "plan.json"),
        "--context",
        &shown(scratch.path(), "context.json"),
    ]);

    let producer = tree_producer(&context()).unwrap();
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        format!(
            "amiss locale-inventory: en 2 pages complete\n\
             amiss locale-inventory: de-DE 1 pages complete\n\
             target pages still carrying the source bytes: 0\n\
             producer {} {} context {}\n",
            producer.identity.as_str(),
            producer.version,
            producer.context_digest,
        )
    );
}

#[test]
fn a_checkout_without_the_planned_commit_refuses() {
    let (_chain, scratch) = staged();
    let elsewhere = staged_repository(&[("docs/index.md", Staged::File(b"# Other\n"))]).unwrap();

    let (code, _stdout, stderr) = support::amiss(&[
        "locale-inventory",
        "--repo",
        &elsewhere.repo,
        "--plan",
        &shown(scratch.path(), "plan.json"),
        "--context",
        &shown(scratch.path(), "context.json"),
    ]);

    assert_eq!(code, 2);
    assert!(stderr.contains("GIT_OBJECT_MISSING"), "{stderr}");
}

#[test]
fn the_grammar_closes_the_inventory_form() {
    let (chain, scratch) = staged();
    let plan = shown(scratch.path(), "plan.json");
    let context = shown(scratch.path(), "context.json");
    let complete: &[&str] = &[
        "locale-inventory",
        "--repo",
        &chain.repo,
        "--plan",
        &plan,
        "--context",
        &context,
    ];
    let unknown = "a".repeat(40);
    for extra in [
        ["--candidate", unknown.as_str()],
        ["--object-format", "sha1"],
        ["--evidence", plan.as_str()],
        ["--format", "sarif"],
    ] {
        let mut argv = complete.to_vec();
        argv.extend(extra);
        let (code, _stdout, stderr) = support::amiss(&argv);
        assert_eq!(code, 2, "{extra:?} is outside the inventory form: {stderr}");
    }
    for missing in [
        ["locale-inventory"].as_slice(),
        &["locale-inventory", "--repo", &chain.repo],
        &["locale-inventory", "--plan", &plan, "--context", &context],
    ] {
        let (code, _stdout, _stderr) = support::amiss(missing);
        assert_eq!(code, 2);
    }
}
