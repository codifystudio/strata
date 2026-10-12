//! Бенчмарк: layout engine.
//!
//! Метрики:
//!   - время построения layout tree
//!   - время compute_layout для всей страницы

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use strata_bench::fixtures::{extract_css, FIXTURES};
use strata_css::StyleSheet;
use strata_html::parse_html;
use strata_layout::LayoutTree;
use taffy::geometry::Size;

fn bench_layout_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_build");
    let viewport = Size { width: 1280u32, height: 800u32 };

    for f in FIXTURES {
        let doc = parse_html(f.html).unwrap();
        let css = extract_css(f.html);
        let sheet = if css.is_empty() {
            None
        } else {
            StyleSheet::parse(&css).ok()
        };

        group.bench_function(BenchmarkId::from_parameter(f.name), |b| {
            b.iter(|| {
                let tree = LayoutTree::build(
                    black_box(&doc),
                    black_box(sheet.as_ref()),
                    black_box(viewport),
                )
                .unwrap();
                black_box(tree);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_layout_build);
criterion_main!(benches);
