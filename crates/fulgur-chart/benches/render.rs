//! E2E render-speed benchmarks (report-only; never gates CI).
//! Times JSON -> SVG and JSON -> PNG for each representative case.
use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};

use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png_default;
use fulgur_chart::render::render_chart;

#[path = "cases.rs"]
mod cases;

fn bench_e2e(c: &mut Criterion) {
    let cases = cases::all();

    let mut svg = c.benchmark_group("e2e_svg");
    // Keep CI wall-clock bounded; this bench is informational, not a gate.
    svg.sample_size(20).measurement_time(Duration::from_secs(3));
    for case in &cases {
        svg.bench_function(case.name, |b| {
            b.iter(|| {
                let spec = chartjs::parse(black_box(&case.json), false).unwrap();
                black_box(render_chart(&spec));
            });
        });
    }
    svg.finish();

    let mut png = c.benchmark_group("e2e_png");
    png.sample_size(20).measurement_time(Duration::from_secs(3));
    for case in &cases {
        png.bench_function(case.name, |b| {
            b.iter(|| {
                let spec = chartjs::parse(black_box(&case.json), false).unwrap();
                black_box(render_chart_to_png_default(&spec, 1.0).unwrap());
            });
        });
    }
    png.finish();
}

fn bench_frontend(c: &mut Criterion) {
    let cases = cases::all();
    let mut group = c.benchmark_group("frontend");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(3));
    for case in &cases {
        for strict in [false, true] {
            let name = format!("{}/{}", case.name, if strict { "strict" } else { "normal" });
            group.bench_function(name, |b| {
                b.iter(|| {
                    black_box(
                        fulgur_chart::frontend::parse_with_error_kind(
                            black_box(&case.json),
                            "chartjs",
                            strict,
                        )
                        .unwrap(),
                    );
                })
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_e2e, bench_frontend);
criterion_main!(benches);
