#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use amiss_md::{Heading, HeadingSource};
use amiss_scan::anchor::{RULES, identities};
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_scan::{Classification, DocumentRecord, DocumentStatus, SnapshotDiscovery};
use amiss_wire::controls::GitMode;
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// The translation-mirror shape that made starship the ledger's outlier:
/// many locales, each with one heading-heavy target and readers full of
/// fragment references into it. This holds the anchor lane's cost to the
/// retained identities instead of a second parse per distinct target.
#[divan::bench(sample_count = 3)]
fn mirror_corpus_20_locales(bencher: Bencher<'_, '_>) {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("README.md"), "# R\n").unwrap_or_else(|defect| panic!("{defect}"));
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = revision(root, "HEAD");

    for locale in 0..20_usize {
        let dir_name = format!("docs/l{locale:02}");
        std::fs::create_dir_all(root.join(&dir_name)).unwrap_or_else(|defect| panic!("{defect}"));
        let mut target = String::from("# Config\n\n");
        for heading in 0..300_usize {
            let _ = writeln!(target, "## Option {locale} {heading}\n\nbody text\n");
        }
        std::fs::write(root.join(format!("{dir_name}/config.md")), target)
            .unwrap_or_else(|defect| panic!("{defect}"));
        for reader in 0..3_usize {
            let mut body = String::from("# Reader\n\n");
            for fragment in 0..10_usize {
                let _ = writeln!(
                    body,
                    "[o{fragment}](config.md#option-{locale}-{})",
                    fragment.saturating_mul(29)
                );
            }
            std::fs::write(root.join(format!("{dir_name}/reader{reader}.md")), body)
                .unwrap_or_else(|defect| panic!("{defect}"));
        }
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "mirrors"]);
    let candidate = revision(root, "HEAD");

    let repo = amiss_git::Repository::open(root, ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = shell();
    bencher.bench_local(|| commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate));
}

/// Repeated headings exercise each renderer's collision suffix sequence.
#[divan::bench(sample_count = 3)]
fn duplicate_heading_identities(bencher: Bencher<'_, '_>) {
    let headings: Vec<Heading> = (0..4_096_usize)
        .map(|index| Heading {
            text: "Repeated heading".to_owned(),
            attribute: None,
            source: HeadingSource::Markdown,
            span: (index, index.saturating_add(1)),
        })
        .collect();
    let rule = RULES
        .first()
        .unwrap_or_else(|| panic!("the renderer table is nonempty"));
    bencher.bench_local(|| identities(black_box(rule), black_box(&headings)));
}

/// One heading-heavy target asked for many distinct missing fragments.
#[divan::bench(sample_count = 3)]
fn missing_anchor_corpus(bencher: Bencher<'_, '_>) {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("README.md"), "# R\n").unwrap_or_else(|defect| panic!("{defect}"));
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = revision(root, "HEAD");

    let mut target = String::new();
    for heading in 0..4_096_usize {
        let _ = writeln!(target, "# Published heading {heading}\n");
    }
    std::fs::create_dir_all(root.join("docs")).unwrap_or_else(|defect| panic!("{defect}"));
    std::fs::write(root.join("docs/target.md"), target).unwrap_or_else(|defect| panic!("{defect}"));
    let mut reader = String::from("# Reader\n\n");
    for fragment in 0..2_048_usize {
        let _ = writeln!(
            reader,
            "[missing {fragment}](target.md#absent-fragment-{fragment})"
        );
    }
    std::fs::write(root.join("docs/reader.md"), reader).unwrap_or_else(|defect| panic!("{defect}"));
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "anchors"]);
    let candidate = revision(root, "HEAD");

    let repo = amiss_git::Repository::open(root, ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = shell();
    bencher.bench_local(|| {
        commit_pair(
            black_box(&repo),
            &shell.engine,
            None,
            &shell,
            black_box(&base),
            black_box(&candidate),
        )
    });
}

/// A late policy-included path in the maximal ordered document inventory.
#[divan::bench(sample_count = 20)]
fn late_policy_bound_adapter(bencher: Bencher<'_, '_>) {
    let oid =
        Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap_or_else(|| panic!("benchmark oid"));
    let documents: Vec<DocumentRecord> = (0..100_000_usize)
        .map(|index| DocumentRecord {
            path: RepoPath::new(format!("generated/{index:06}.page"))
                .unwrap_or_else(|| panic!("benchmark path")),
            classification: Classification::PolicyIncluded,
            adapter: Some(Adapter::Markdown),
            status: DocumentStatus::ExcludedBuiltIn,
            oid: oid.clone(),
            mode: GitMode::RegularFile,
            byte_count: 0,
            raw_digest: None,
        })
        .collect();
    let snapshot = SnapshotDiscovery {
        documents,
        outside_document_set: 0,
        tree_entries: 0,
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
        labels: BTreeMap::new(),
    };
    let path = RepoPath::new("generated/099999.page".to_owned())
        .unwrap_or_else(|| panic!("benchmark lookup path"));
    bencher.bench_local(|| snapshot.bound_adapter(black_box(&path)));
}

fn git(dir: &Path, args: &[&str]) {
    amiss_fixtures::git(dir, args).unwrap_or_else(|defect| panic!("git {args:?}: {defect}"));
}

fn revision(dir: &Path, name: &str) -> Oid {
    let hex = amiss_fixtures::git(dir, &["rev-parse", name])
        .unwrap_or_else(|defect| panic!("rev-parse: {defect}"));
    Oid::new(ObjectFormat::Sha1, hex.trim().to_owned())
        .unwrap_or_else(|| panic!("fixture revision is a valid oid"))
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-bench".to_owned(),
        digest: amiss_wire::digest::hb("amiss/scanner-engine", b"bench engine"),
    }
}

fn shell() -> SetupShell {
    SetupShell {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}
