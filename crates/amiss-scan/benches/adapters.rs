use amiss_scan::{ScanLimits, ScanResources, scan_document};
use amiss_wire::model::Adapter;
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// Both adapters over the parser-eligibility fixture: the adversarial 4 MiB
/// worst case behind the two-second law. The law itself is a release test;
/// this tracks drift.
#[divan::bench(args = [Adapter::Markdown, Adapter::Mdx], sample_count = 10)]
fn worst_case(bencher: Bencher<'_, '_>, adapter: Adapter) {
    let source = amiss_fixtures::worst_case_markdown(4 * 1_024 * 1_024);
    bencher
        .counter(BytesCount::of_slice(&source))
        .bench_local(|| {
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            scan_document(&mut resources, black_box(adapter), black_box(&source))
        });
}

/// Both adapters over one typical hand-written page.
#[divan::bench(args = [Adapter::Markdown, Adapter::Mdx])]
fn typical_page(bencher: Bencher<'_, '_>, adapter: Adapter) {
    let source = amiss_fixtures::representative_documents(2)
        .pop()
        .map(|(_path, body)| body.into_bytes())
        .unwrap_or_default();
    bencher
        .counter(BytesCount::of_slice(&source))
        .bench_local(|| {
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            scan_document(&mut resources, black_box(adapter), black_box(&source))
        });
}
