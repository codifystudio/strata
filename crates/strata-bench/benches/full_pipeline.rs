//! Бенчмарк: полный pipeline — от HTML строки до RGBA буфера.
//!
//! Метрики:
//!   - end-to-end latency (ms)
//!   - throughput (KB/s) = размер_файла / время

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_bench::fixtures::{extract_css, FIXTURES};
use strata_css::StyleSheet;
use strata_html::parse_html;
use strata_paint::Painter;

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");
    let (w, h) = (1280u32, 800u32);

    for f in FIXTURES {
        group.throughput(Throughput::Bytes(f.html.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(f.name), f.html, |b, html| {
            b.iter(|| {
                // 1) parse HTML
                let doc = parse_html(black_box(html)).unwrap();
                // 2) extract & parse CSS
                let css = extract_css(html);
                let sheet = if css.is_empty() {
                    None
                } else {
                    StyleSheet::parse(&css).ok()
                };
                // 3) paint
                let painter = Painter::new(w, h);
                let res = painter.paint(black_box(&doc), black_box(sheet.as_ref())).unwrap();
                black_box(res);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_full_pipeline);
criterion_main!(benches);
