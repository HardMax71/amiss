use amiss_rst::references;
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[divan::bench]
fn typical_line(bencher: Bencher<'_, '_>) {
    let line = "Read :doc:`the guide <guide>` and :ref:`setup`, then follow `the example <examples/setup.rst>`_.";
    bencher
        .counter(BytesCount::of_slice(line.as_bytes()))
        .bench_local(|| references(black_box(line), black_box(0)));
}

#[divan::bench(args = [16_usize, 128])]
fn dense_mixed_references(bencher: Bencher<'_, '_>, count: usize) {
    let line = ":doc:`guide` :ref:`setup label` `example <examples/setup.rst>`_ ".repeat(count);
    bencher
        .counter(BytesCount::of_slice(line.as_bytes()))
        .bench_local(|| references(black_box(&line), black_box(0)));
}

#[divan::bench(args = [256_usize, 4_096])]
fn prose_without_markup(bencher: Bencher<'_, '_>, bytes: usize) {
    let line = "ordinary documentation prose without interpreted text ".repeat(bytes.div_ceil(50));
    let line = line.get(..bytes).unwrap_or(&line);
    bencher
        .counter(BytesCount::of_slice(line.as_bytes()))
        .bench_local(|| references(black_box(line), black_box(0)));
}
