//! Deterministic memory-allocation measurement + regression gate.
//!
//! Uses dhat as the global allocator and measures per-case allocation bytes via
//! `HeapStats` deltas within a single process (deterministic; no subprocess).
//!
//! Usage (the `dhat-heap` feature is required, enforced by `required-features`):
//!   cargo bench -p fulgur-chart --bench membench --features dhat-heap            # print current
//!   cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check # gate vs baseline
//!   cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --update # rewrite baseline
//!   ... -- --check --threshold 30   # custom % threshold (default 25)
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

#[path = "cases.rs"]
mod cases;
#[path = "membench_check.rs"]
mod membench_check;

use membench_check::{Baseline, CaseStat, check};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const DEFAULT_THRESHOLD_PCT: f64 = 25.0;

fn baseline_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benches/membench_baseline.json"
    ))
}

/// Measure per-case allocation bytes/blocks for the E2E SVG path (parse + render).
/// `total_bytes` is cumulative-allocated (frees don't reduce it), so the delta is
/// the allocation volume of that one case — deterministic for fixed input + code.
fn measure() -> Baseline {
    let _profiler = dhat::Profiler::builder().testing().build();
    let mut out: Baseline = BTreeMap::new();
    for case in cases::all() {
        let before = dhat::HeapStats::get();
        let spec = chartjs::parse(&case.json, false).expect("case parses");
        let svg = render_chart(&spec);
        let after = dhat::HeapStats::get();
        std::hint::black_box(&svg);
        out.insert(
            case.name.to_string(),
            CaseStat {
                alloc_bytes: after.total_bytes - before.total_bytes,
                alloc_blocks: after.total_blocks - before.total_blocks,
            },
        );
    }
    out
}

fn read_baseline(path: &std::path::Path) -> Option<Baseline> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_baseline(path: &std::path::Path, b: &Baseline) {
    let json = serde_json::to_string_pretty(b).expect("serialize baseline");
    std::fs::write(path, format!("{json}\n")).expect("write baseline");
}

fn print_table(current: &Baseline, baseline: Option<&Baseline>) {
    println!("{:<16} {:>14} {:>14}", "case", "alloc_bytes", "vs_baseline");
    for (name, cur) in current {
        let vs = match baseline.and_then(|b| b.get(name)) {
            Some(base) if base.alloc_bytes > 0 => {
                format!(
                    "{:+.1}%",
                    (cur.alloc_bytes as f64 / base.alloc_bytes as f64 - 1.0) * 100.0
                )
            }
            Some(_) => "n/a".to_string(),
            None => "(new)".to_string(),
        };
        println!("{:<16} {:>14} {:>14}", name, cur.alloc_bytes, vs);
    }
}

fn parse_threshold(args: &[String]) -> f64 {
    if let Some(i) = args.iter().position(|a| a == "--threshold") {
        if let Some(v) = args.get(i + 1) {
            if let Ok(n) = v.parse::<f64>() {
                return n;
            }
        }
    }
    DEFAULT_THRESHOLD_PCT
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let do_update = args.iter().any(|a| a == "--update");
    let do_check = args.iter().any(|a| a == "--check");
    let threshold = parse_threshold(&args);
    let path = baseline_path();

    let current = measure();

    if do_update {
        write_baseline(&path, &current);
        println!("baseline written to {}", path.display());
        print_table(&current, None);
        return ExitCode::SUCCESS;
    }

    if do_check {
        let Some(baseline) = read_baseline(&path) else {
            eprintln!(
                "error: baseline not found/parsable at {}; run with --update and commit it",
                path.display()
            );
            return ExitCode::FAILURE;
        };
        print_table(&current, Some(&baseline));

        let missing: Vec<&String> = current
            .keys()
            .filter(|k| !baseline.contains_key(*k))
            .collect();
        let regressions = check(&baseline, &current, threshold);

        if !missing.is_empty() {
            eprintln!(
                "error: {} case(s) missing from baseline: {:?}; run --update and commit",
                missing.len(),
                missing
            );
        }
        for r in &regressions {
            eprintln!(
                "REGRESSION {}: alloc_bytes {} -> {} ({:.2}x, threshold +{:.0}%)",
                r.case, r.baseline, r.current, r.ratio, threshold
            );
        }
        if missing.is_empty() && regressions.is_empty() {
            println!("memory check OK (threshold +{threshold:.0}%)");
            return ExitCode::SUCCESS;
        }
        return ExitCode::FAILURE;
    }

    // Default: just print current numbers.
    print_table(&current, None);
    ExitCode::SUCCESS
}
