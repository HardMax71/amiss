use amiss_rst::extract;
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

fn bench_extraction(bencher: Bencher<'_, '_>, source: &[u8]) {
    bencher
        .counter(BytesCount::of_slice(source))
        .bench_local(|| extract(black_box(source)));
}

#[divan::bench]
fn representative_document(bencher: Bencher<'_, '_>) {
    let source = concat!(
        "Project guide\n=============\n\n",
        "Read :doc:`the introduction <intro>` and `the example <examples/start.rst>`_.\n",
        "The remaining prose explains the project without additional markup.\n\n",
        "Installation\n------------\n\n",
        "See :ref:`setup details` before continuing.\n\n",
        ".. _downloads: release.rst\n\n",
        ".. image:: images/overview.png\n\n",
        ".. csv-table:: Supported versions\n",
        "   :file: data/versions.csv\n",
    );
    bench_extraction(bencher, source.as_bytes());
}

#[divan::bench(args = [4_096_usize, 65_536])]
fn prose_document(bencher: Bencher<'_, '_>, bytes: usize) {
    let line = "Ordinary documentation prose without interpreted text or section adornment.\n";
    let source = line.repeat(bytes.div_ceil(line.len()));
    let source = source.as_bytes().get(..bytes).unwrap_or(source.as_bytes());
    bench_extraction(bencher, source);
}

#[divan::bench(args = [16_usize, 128])]
fn sectioned_document(bencher: Bencher<'_, '_>, sections: usize) {
    let section = concat!(
        "Section title\n=============\n\n",
        "Read :doc:`guide` and `the example <examples/start.rst>`_.\n",
        "Ordinary prose follows the references in the same text block.\n\n",
    );
    let source = section.repeat(sections);
    bench_extraction(bencher, source.as_bytes());
}
