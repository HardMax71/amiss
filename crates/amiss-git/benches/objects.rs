use amiss_git::parse_tree;
use amiss_wire::model::ObjectFormat;
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
