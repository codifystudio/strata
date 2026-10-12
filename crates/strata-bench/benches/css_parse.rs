//! Бенчмарк: парсинг CSS.
//!
//! Метрики:
//!   - время парсинга CSS из фикстур
//!   - время применения стилей к DOM (compute styles)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_bench::fixtures::{extract_css, FIXTURES};
use strata_css::StyleSheet;
use strata_html::parse_html;

fn bench_css_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("css_parse");

    for f in FIXTURES {
        let css = extract_css(f.html);
        if css.is_empty() {
            continue;
        }
        group.throughput(Throughput::Bytes(css.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(f.name), &css, |b, css| {
            b.iter(|| {
                let sheet = StyleSheet::parse(black_box(css)).unwrap();
                black_box(sheet);
            });
        });
    }
    group.finish();
}

fn bench_css_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("css_compute");

    for f in FIXTURES {
        let doc = parse_html(f.html).unwrap();
        let css = extract_css(f.html);
        let sheet = if css.is_empty() {
            None
        } else {
            Some(StyleSheet::parse(&css).unwrap())
        };

        group.bench_function(BenchmarkId::from_parameter(f.name), |b| {
            b.iter(|| {
                if let Some(s) = black_box(&sheet) {
                    let styles = s.compute(black_box(&doc));
                    black_box(styles);
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_css_parse, bench_css_compute);
criterion_main!(benches);
