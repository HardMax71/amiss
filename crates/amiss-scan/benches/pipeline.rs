#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use divan::Bencher;

fn main() {
    divan::main();
}

/// One complete evaluation over the representative repository: the
/// incremental-latency figure the promotion gate asks to see measured.
#[divan::bench(sample_count = 10)]
fn commit_pair_500_docs(bencher: Bencher<'_, '_>) {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    amiss_fixtures::representative_repository(dir.path(), 500)
        .unwrap_or_else(|defect| panic!("fixture repository: {defect}"));
    let base = revision(dir.path(), "HEAD~1");
    let candidate = revision(dir.path(), "HEAD");
    let repo = amiss_git::Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = SetupShell {
        engine: EngineProvenance {
            version: "0.0.0-bench".to_owned(),
            digest: hb("amiss/scanner-engine/v1", b"bench engine"),
        },
        enforce: false,
        repository: None,
        candidate_ref: None,
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

fn revision(root: &std::path::Path, spec: &str) -> Oid {
    let raw = amiss_fixtures::git(root, &["rev-parse", spec])
        .unwrap_or_else(|defect| panic!("rev-parse: {defect}"));
    Oid::new(ObjectFormat::Sha1, raw.trim().to_owned()).unwrap_or_else(|| panic!("oid for {spec}"))
}
