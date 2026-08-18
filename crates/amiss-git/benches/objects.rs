#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use std::path::Path;

use amiss_git::{GitLimits, GitResources, Repository, parse_tree};
use amiss_wire::model::{ObjectFormat, Oid};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[divan::bench(args = [1_000_usize, 100_000], sample_count = 20)]
fn wide_tree(bencher: Bencher<'_, '_>, entry_count: usize) {
    let mut body = Vec::with_capacity(entry_count.saturating_mul(44));
    for entry in 0..entry_count {
        body.extend_from_slice(format!("100644 {entry:016x}\0").as_bytes());
        body.extend_from_slice(&[0xab_u8; 20]);
    }

    bencher.bench_local(|| parse_tree(ObjectFormat::Sha1, black_box(&body)));
}

#[divan::bench(sample_count = 20)]
fn repeated_loose_object(bencher: Bencher<'_, '_>) {
    let pair = amiss_fixtures::commit_pair(&[("README.md", "# reusable inflater\n")], &[])
        .unwrap_or_else(|defect| panic!("fixture repository: {defect}"));
    let repository = Repository::open(Path::new(&pair.repo), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open repository: {defect:?}"));
    let oid = Oid::new(ObjectFormat::Sha1, pair.candidate)
        .unwrap_or_else(|| panic!("invalid candidate oid"));
    let mut resources = GitResources::new(GitLimits::CONTRACT);

    bencher.bench_local(|| repository.read_object(black_box(&mut resources), black_box(&oid)));
}
