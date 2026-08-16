#![expect(clippy::panic, reason = "benchmark fixture setup fails loudly")]

use std::fmt::Write as _;

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

/// Every structured adapter over one typical hand-written page.
#[divan::bench(args = [
    Adapter::Markdown,
    Adapter::Mdx,
    Adapter::Rst,
    Adapter::AsciiDoc,
])]
fn typical_page(bencher: Bencher<'_, '_>, adapter: Adapter) {
    let source = reference_page(adapter, 12);
    bencher
        .counter(BytesCount::of_slice(&source))
        .bench_local(|| {
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            scan_document(&mut resources, black_box(adapter), black_box(&source))
        });
}

/// Position accounting at half the per-document reference ceiling, with
/// enough lines to expose indexing work without measuring a rejected scan.
#[divan::bench(
    args = [
        Adapter::Markdown,
        Adapter::Mdx,
        Adapter::Rst,
        Adapter::AsciiDoc,
    ],
    sample_count = 10,
)]
fn dense_references(bencher: Bencher<'_, '_>, adapter: Adapter) {
    let source = reference_page(adapter, 4_096);
    let mut validation_resources = ScanResources::new(ScanLimits::CONTRACT);
    let scanned = scan_document(&mut validation_resources, adapter, &source)
        .unwrap_or_else(|defect| panic!("dense {adapter:?} fixture: {defect:?}"));
    assert_eq!(scanned.occurrences.len(), 8_192);

    bencher
        .counter(BytesCount::of_slice(&source))
        .bench_local(|| {
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            scan_document(&mut resources, black_box(adapter), black_box(&source))
        });
}

fn reference_page(adapter: Adapter, lines: usize) -> Vec<u8> {
    let (header, next, home) = match adapter {
        Adapter::Markdown | Adapter::Mdx => (
            "# Document\n\n",
            "[next](doc.md#part)",
            "[home](../README.md)",
        ),
        Adapter::Rst => (
            "Document\n========\n\n",
            "`next <doc.rst#part>`_",
            "`home <../README.rst>`_",
        ),
        Adapter::AsciiDoc => (
            "= Document\n\n",
            "xref:doc.adoc#part[next]",
            "xref:../README.adoc[home]",
        ),
        Adapter::PlainAdvisory => ("", "", ""),
    };
    let mut source = String::with_capacity(lines.saturating_mul(96));
    source.push_str(header);
    for index in 0..lines {
        let _infallible = writeln!(source, "Paragraph {index} links {next} and {home}.\n");
    }
    source.into_bytes()
}
