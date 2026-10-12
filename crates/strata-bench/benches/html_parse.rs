//! Бенчмарк: парсинг HTML.
//!
//! Метрики:
//!   - время парсинга для каждой фикстуры
//!   - throughput (MB/s) = размер_файла / время

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_bench::fixtures::FIXTURES;
use strata_html::parse_html;

fn bench_html_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("html_parse");
    for f in FIXTURES {
        group.throughput(Throughput::Bytes(f.html.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(f.name), f.html, |b, html| {
            b.iter(|| {
                let doc = parse_html(black_box(html)).unwrap();
                black_box(doc);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_html_parse);
criterion_main!(benches);
