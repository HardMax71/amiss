use amiss_scan::{ScanLimits, ScanResources, scan_document};
use amiss_wire::model::Adapter;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// Both adapters over the parser-eligibility fixture (the adversarial
/// 4 MiB worst case) and over a typical hand-written page, as bytes
/// throughput. The eligibility law itself is a release test; this tracks
/// drift.
fn adapters(bencher: &mut Criterion) {
    let worst = amiss_fixtures::worst_case_markdown(4 * 1_024 * 1_024);
    let typical = amiss_fixtures::representative_documents(2)
        .pop()
        .map(|(_path, body)| body.into_bytes())
        .unwrap_or_default();

    let mut group = bencher.benchmark_group("adapters");
    for (label, source) in [("worst-4mib", &worst), ("typical-page", &typical)] {
        group.throughput(Throughput::Bytes(
            u64::try_from(source.len()).unwrap_or(u64::MAX),
        ));
        for adapter in [Adapter::Markdown, Adapter::Mdx] {
            group.bench_function(format!("{label}/{adapter:?}"), |bench| {
                bench.iter(|| {
                    let mut resources = ScanResources::new(ScanLimits::CONTRACT);
                    scan_document(&mut resources, adapter, source)
                });
            });
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = adapters
}
criterion_main!(benches);
