#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use std::fmt::Write as _;
use std::path::Path;

use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use divan::Bencher;

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
    let shell = SetupShell {
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
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    bencher.bench_local(|| commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate));
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
