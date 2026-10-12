//! Бенчмарк: paint engine.
//!
//! Метрики:
//!   - время рендеринга страницы в RGBA буфер
//!   - throughput (пикселей/сек)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_bench::fixtures::{extract_css, FIXTURES};
use strata_css::StyleSheet;
use strata_html::parse_html;
use strata_paint::Painter;

fn bench_paint(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_render");
    let (w, h) = (1280u32, 800u32);
    let pixels = (w as u64) * (h as u64);
    group.throughput(Throughput::Elements(pixels));

    for f in FIXTURES {
        let doc = parse_html(f.html).unwrap();
        let css = extract_css(f.html);
        let sheet = if css.is_empty() {
            None
        } else {
            StyleSheet::parse(&css).ok()
        };

        group.bench_with_input(
            BenchmarkId::from_parameter(f.name),
            &(doc, sheet),
            |b, (doc, sheet)| {
                b.iter(|| {
                    let painter = Painter::new(w, h);
                    let res = painter.paint(black_box(doc), black_box(sheet.as_ref())).unwrap();
                    black_box(res);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_paint);
criterion_main!(benches);
