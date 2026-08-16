use amiss_git::{GitLimits, GitResources};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[divan::bench(sample_count = 10_000)]
#[expect(clippy::unwrap_used, reason = "the benchmark uses contract limits")]
fn repeated_pack_index_charge(bencher: Bencher<'_, '_>) {
    const PACK: &str = "0123456789abcdef0123456789abcdef01234567";
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    resources.charge_index(PACK, 1).unwrap();
    bencher.bench_local(|| resources.charge_index(black_box(PACK), black_box(1)));
}

#[divan::bench(sample_count = 1_000)]
fn unique_pack_index_charges(bencher: Bencher<'_, '_>) {
    let members: Vec<String> = (0..256_u16)
        .map(|member| format!("{member:040x}"))
        .collect();
    bencher.bench_local(|| {
        let mut resources = GitResources::new(GitLimits::CONTRACT);
        for member in black_box(&members) {
            resources.charge_index(member, 1)?;
        }
        Ok::<_, amiss_git::Error>(resources)
    });
}
