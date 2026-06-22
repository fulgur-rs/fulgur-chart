# Performance Benchmarks Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add criterion render-speed benchmarks and a deterministic dhat memory-allocation gate for representative chart cases, wired into CI so extreme regressions fail the build.

**Architecture:** Two bench targets in `crates/fulgur-chart`. `render.rs` uses criterion to time the E2E pipeline (JSON → SVG and JSON → PNG) — report-only, never gates. `membench.rs` uses dhat as a global allocator to measure deterministic per-case allocation bytes, compares them to a committed baseline JSON, and exits non-zero on regression beyond a threshold. Case generation and the comparison logic live in shared `#[path]`-included files so they can be unit-tested without polluting the public API.

**Tech Stack:** Rust (edition 2024, MSRV 1.85), criterion 0.7, dhat 0.3, serde / serde_json (already deps), GitHub Actions.

**Issue:** fulgur-chart-g0g

---

## Background / Key Facts (read before starting)

- Public library API used by benches (all in crate `fulgur_chart`):
  - `frontend::chartjs::parse(json: &str, strict: bool) -> Result<ChartSpec, String>`
  - `render::render_chart(spec: &ChartSpec) -> String` (SVG)
  - `raster_direct::render_chart_to_png_default(spec: &ChartSpec, scale: f32) -> Result<Vec<u8>, String>` (PNG)
- Input guards (`guard::validate_spec`) are applied **only at the CLI trust boundary**, not in `parse`/render core — so large cases (10k points) render fine via the library API. Default limits (1,000,000 total points) are also well above our cases.
- chart.js JSON shapes:
  - bar/line: `{"type":"bar","data":{"labels":[...],"datasets":[{"label":"d","data":[<numbers>]}]}}`
  - scatter: `{"type":"scatter","data":{"datasets":[{"label":"d","data":[{"x":N,"y":N},...]}]}}`
  - pie: `{"type":"pie","data":{"labels":[...],"datasets":[{"label":"d","data":[<numbers>]}]}}`
- All bench targets live under `crates/fulgur-chart/benches/`. `CARGO_MANIFEST_DIR` for a bench target is the crate dir (`crates/fulgur-chart`), so the baseline path is resolved as `concat!(env!("CARGO_MANIFEST_DIR"), "/benches/membench_baseline.json")` regardless of cwd.
- Run all commands from the worktree root `/home/ubuntu/fulgur-chart/.worktrees/perf/benchmarks` unless noted. The core crate is `fulgur-chart`; pass `-p fulgur-chart` when needed (workspace has two members).
- `cargo bench` builds in a release-like profile; numbers are representative.
- `required-features = ["dhat-heap"]` keeps the membench target (and dhat) out of normal `cargo build`/`cargo test`/`cargo clippy --all-targets` runs — those targets are skipped unless the feature is enabled. The `render` bench has NO required features, so it IS compiled by `cargo clippy --workspace --all-targets` and MUST be clippy-clean.

---

## Task 1: Add dependencies and the `dhat-heap` feature

**Files:**
- Modify: `crates/fulgur-chart/Cargo.toml`
- Modify: `Cargo.lock` (auto-updated by cargo)

**Step 1: Add the dependencies**

Run (from worktree root):

```bash
cargo add criterion --dev -p fulgur-chart
cargo add dhat --optional -p fulgur-chart
```

Expected: `criterion v0.7.x` added to `[dev-dependencies]`, `dhat v0.3.x` added to `[dependencies]` as optional, and cargo prints `Adding feature \`dhat\``.

**Step 2: Add the feature**

Edit `crates/fulgur-chart/Cargo.toml`. After the `[dependencies]` block (and before `[dev-dependencies]`), add:

```toml
[features]
# Enables the dhat global allocator for the `membench` bench target only.
dhat-heap = ["dep:dhat"]
```

Leave `dhat` listed as `optional = true` under `[dependencies]` (cargo add did this). Do NOT enable it by default.

**Step 3: Verify normal build is unaffected**

Run:

```bash
cargo build -p fulgur-chart
```

Expected: builds clean. dhat must NOT be compiled (it is optional and the feature is off):

```bash
cargo tree -p fulgur-chart -i dhat 2>&1 | head
```

Expected: a message that the package `dhat` was not found in the dependency tree (feature off) — i.e. dhat is not pulled into the default build.

**Step 4: Verify tests still pass (clean baseline preserved)**

Run:

```bash
cargo test --workspace --locked
```

Expected: PASS, same as before.

**Step 5: Commit**

```bash
git add crates/fulgur-chart/Cargo.toml Cargo.lock
git commit -m "build(bench): add criterion dev-dep and optional dhat behind dhat-heap feature"
```

---

## Task 2: Shared case generators (`cases.rs`) + their tests

The generators produce chart.js JSON strings for representative small/large cases. They are pure (std + `format!` only — no criterion/dhat imports) so they can be `#[path]`-included by both bench targets and unit-tested via a real integration test.

**Files:**
- Create: `crates/fulgur-chart/benches/cases.rs`
- Create: `crates/fulgur-chart/tests/bench_cases.rs`

**Step 1: Write the failing test**

Create `crates/fulgur-chart/tests/bench_cases.rs`:

```rust
//! Verifies every benchmark case generates valid chart.js JSON that parses and
//! renders to a non-empty SVG. Shares the generators with the bench targets via
//! `#[path]` include so the benched inputs are exactly what we test here.
#[path = "../benches/cases.rs"]
mod cases;

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

#[test]
fn every_case_parses_and_renders() {
    let all = cases::all();
    assert!(!all.is_empty(), "expected at least one case");
    for case in &all {
        let spec = chartjs::parse(&case.json, false)
            .unwrap_or_else(|e| panic!("case {} failed to parse: {e}", case.name));
        let svg = render_chart(&spec);
        assert!(
            svg.starts_with("<svg"),
            "case {} did not render an SVG",
            case.name
        );
    }
}

#[test]
fn case_names_are_unique() {
    let all = cases::all();
    let mut names: Vec<&str> = all.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n, "case names must be unique");
}

#[test]
fn large_cases_have_expected_scale() {
    let all = cases::all();
    let line_large = all.iter().find(|c| c.name == "line_large").unwrap();
    // 10k points => the JSON is large; cheap sanity check on scale.
    assert!(line_large.json.len() > 10_000);
}
```

**Step 2: Run it to verify it fails**

Run:

```bash
cargo test -p fulgur-chart --test bench_cases
```

Expected: FAIL to compile — `cases` module / `benches/cases.rs` does not exist yet.

**Step 3: Write the generators**

Create `crates/fulgur-chart/benches/cases.rs`:

```rust
//! Representative benchmark cases shared by the `render` (speed) and `membench`
//! (memory) bench targets. JSON is generated programmatically (deterministic,
//! no path coupling to examples/). Pure std — no criterion/dhat imports — so it
//! can also be `#[path]`-included by an integration test.
#![allow(dead_code)] // not every includer uses every helper

/// A single benchmark case: a name and a ready-to-parse chart.js JSON spec.
pub struct Case {
    pub name: &'static str,
    pub json: String,
}

/// All benchmark cases, small + synthetic-large, E2E-oriented.
pub fn all() -> Vec<Case> {
    vec![
        Case { name: "bar_small", json: bar(12) },
        Case { name: "bar_large", json: bar(1_000) },
        Case { name: "line_small", json: line(12) },
        Case { name: "line_large", json: line(10_000) },
        Case { name: "scatter_large", json: scatter(10_000) },
        Case { name: "pie_small", json: pie(6) },
    ]
}

/// Deterministic pseudo value in [0, 100) from an index (no RNG → reproducible).
fn val(i: usize) -> usize {
    (i * 37 + 13) % 100
}

fn labels(n: usize) -> String {
    (0..n)
        .map(|i| format!("\"L{i}\""))
        .collect::<Vec<_>>()
        .join(",")
}

fn numbers(n: usize) -> String {
    (0..n)
        .map(|i| val(i).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn bar(n: usize) -> String {
    format!(
        r#"{{"type":"bar","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}

fn line(n: usize) -> String {
    format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}

fn scatter(n: usize) -> String {
    let pts = (0..n)
        .map(|i| format!(r#"{{"x":{},"y":{}}}"#, i, val(i)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"type":"scatter","data":{{"datasets":[{{"label":"d","data":[{pts}]}}]}}}}"#
    )
}

fn pie(n: usize) -> String {
    format!(
        r#"{{"type":"pie","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}
```

**Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test -p fulgur-chart --test bench_cases
```

Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add crates/fulgur-chart/benches/cases.rs crates/fulgur-chart/tests/bench_cases.rs
git commit -m "test(bench): add shared benchmark case generators with parse/render coverage"
```

---

## Task 3: criterion speed bench (`render.rs`) — report-only

**Files:**
- Create: `crates/fulgur-chart/benches/render.rs`
- Modify: `crates/fulgur-chart/Cargo.toml` (add `[[bench]]` entry)

**Step 1: Write the bench**

Create `crates/fulgur-chart/benches/render.rs`:

```rust
//! E2E render-speed benchmarks (report-only; never gates CI).
//! Times JSON -> SVG and JSON -> PNG for each representative case.
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

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

criterion_group!(benches, bench_e2e);
criterion_main!(benches);
```

**Step 2: Register the bench target**

Edit `crates/fulgur-chart/Cargo.toml`, append:

```toml
[[bench]]
name = "render"
harness = false
```

**Step 3: Smoke-run the bench (runs each case once)**

Run:

```bash
cargo bench -p fulgur-chart --bench render -- --test
```

Expected: each `e2e_svg/<case>` and `e2e_png/<case>` runs once and reports `... bench: ... ` / `Success`. No panics.

**Step 4: Verify clippy is clean on the bench (it is compiled by CI `--all-targets`)**

Run:

```bash
cargo clippy -p fulgur-chart --benches -- -D warnings
```

Expected: no warnings.

**Step 5: Commit**

```bash
git add crates/fulgur-chart/benches/render.rs crates/fulgur-chart/Cargo.toml
git commit -m "perf(bench): add criterion E2E render-speed benchmark (report-only)"
```

---

## Task 4: Pure regression-check logic (`membench_check.rs`) + tests

Extract the deterministic comparison so it is unit-testable without disk I/O or dhat.

**Files:**
- Create: `crates/fulgur-chart/benches/membench_check.rs`
- Create: `crates/fulgur-chart/tests/membench_check.rs`

**Step 1: Write the failing test**

Create `crates/fulgur-chart/tests/membench_check.rs`:

```rust
//! Unit tests for the pure memory-regression comparison logic.
#[path = "../benches/membench_check.rs"]
mod membench_check;

use membench_check::{check, Baseline, CaseStat};

fn stat(bytes: u64) -> CaseStat {
    CaseStat { alloc_bytes: bytes, alloc_blocks: 0 }
}

fn baseline() -> Baseline {
    let mut b = Baseline::new();
    b.insert("a".to_string(), stat(1000));
    b.insert("b".to_string(), stat(2000));
    b
}

#[test]
fn no_regression_when_equal() {
    let regs = check(&baseline(), &baseline(), 25.0);
    assert!(regs.is_empty());
}

#[test]
fn within_threshold_is_ok() {
    let mut cur = baseline();
    // +24% on "a" (1000 -> 1240), threshold 25% => OK
    cur.insert("a".to_string(), stat(1240));
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty(), "got {regs:?}");
}

#[test]
fn over_threshold_is_flagged() {
    let mut cur = baseline();
    // +30% on "a" (1000 -> 1300), threshold 25% => regression
    cur.insert("a".to_string(), stat(1300));
    let regs = check(&baseline(), &cur, 25.0);
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0].case, "a");
    assert_eq!(regs[0].current, 1300);
    assert_eq!(regs[0].baseline, 1000);
}

#[test]
fn improvement_is_not_flagged() {
    let mut cur = baseline();
    cur.insert("a".to_string(), stat(500)); // got better
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty());
}

#[test]
fn current_case_missing_from_baseline_is_ignored_by_check() {
    // `check` only compares cases present in both; the membench main handles
    // "missing baseline" separately. A brand-new current case is not a
    // regression here.
    let mut cur = baseline();
    cur.insert("c".to_string(), stat(9999));
    let regs = check(&baseline(), &cur, 25.0);
    assert!(regs.is_empty());
}
```

**Step 2: Run it to verify it fails**

Run:

```bash
cargo test -p fulgur-chart --test membench_check
```

Expected: FAIL to compile — `membench_check` file does not exist yet.

**Step 3: Write the comparison logic**

Create `crates/fulgur-chart/benches/membench_check.rs`:

```rust
//! Pure, deterministic memory-regression comparison shared by the `membench`
//! bench target and its unit tests. No disk I/O, no dhat — just data in/out.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-case allocation stats. `alloc_bytes` is the gated metric; `alloc_blocks`
/// is recorded for context. BTreeMap keys keep the baseline JSON stable-ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseStat {
    pub alloc_bytes: u64,
    pub alloc_blocks: u64,
}

/// case name -> stats.
pub type Baseline = BTreeMap<String, CaseStat>;

/// A flagged regression for one case (alloc_bytes exceeded the allowed factor).
#[derive(Debug, Clone)]
pub struct Regression {
    pub case: String,
    pub baseline: u64,
    pub current: u64,
    pub ratio: f64,
}

/// Returns the cases whose `alloc_bytes` exceeds `baseline * (1 + threshold_pct/100)`.
/// Only cases present in BOTH maps are compared (missing-baseline handling is the
/// caller's responsibility).
pub fn check(baseline: &Baseline, current: &Baseline, threshold_pct: f64) -> Vec<Regression> {
    let factor = 1.0 + threshold_pct / 100.0;
    let mut out = Vec::new();
    for (case, cur) in current {
        let Some(base) = baseline.get(case) else {
            continue;
        };
        let allowed = (base.alloc_bytes as f64) * factor;
        if (cur.alloc_bytes as f64) > allowed {
            let ratio = if base.alloc_bytes == 0 {
                f64::INFINITY
            } else {
                cur.alloc_bytes as f64 / base.alloc_bytes as f64
            };
            out.push(Regression {
                case: case.clone(),
                baseline: base.alloc_bytes,
                current: cur.alloc_bytes,
                ratio,
            });
        }
    }
    out
}
```

**Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test -p fulgur-chart --test membench_check
```

Expected: PASS (5 tests).

**Step 5: Commit**

```bash
git add crates/fulgur-chart/benches/membench_check.rs crates/fulgur-chart/tests/membench_check.rs
git commit -m "test(bench): add pure memory-regression comparison logic with unit tests"
```

---

## Task 5: dhat memory bench (`membench.rs`) + committed baseline

**Files:**
- Create: `crates/fulgur-chart/benches/membench.rs`
- Create: `crates/fulgur-chart/benches/membench_baseline.json` (generated, then committed)
- Modify: `crates/fulgur-chart/Cargo.toml` (add `[[bench]]` entry)

**Step 1: Write the bench harness**

Create `crates/fulgur-chart/benches/membench.rs`:

```rust
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

use membench_check::{check, Baseline, CaseStat};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const DEFAULT_THRESHOLD_PCT: f64 = 25.0;

fn baseline_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/benches/membench_baseline.json"))
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
                format!("{:+.1}%", (cur.alloc_bytes as f64 / base.alloc_bytes as f64 - 1.0) * 100.0)
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

        let missing: Vec<&String> =
            current.keys().filter(|k| !baseline.contains_key(*k)).collect();
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
```

**Step 2: Register the bench target (feature-gated)**

Edit `crates/fulgur-chart/Cargo.toml`, append:

```toml
[[bench]]
name = "membench"
harness = false
required-features = ["dhat-heap"]
```

**Step 3: Verify it builds and prints numbers**

Run:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap
```

Expected: a table with `case / alloc_bytes / vs_baseline`, all cases shown as `(new)` (no baseline yet), exit 0.

**Step 4: Generate the baseline**

Run:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --update
```

Expected: `baseline written to .../benches/membench_baseline.json`. Inspect it:

```bash
cat crates/fulgur-chart/benches/membench_baseline.json
```

Expected: pretty JSON, keys sorted (bar_large, bar_small, line_large, line_small, pie_small, scatter_large), each with `alloc_bytes` and `alloc_blocks`.

**Step 5: Verify the gate passes against the fresh baseline**

Run:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check
echo "exit: $?"
```

Expected: `memory check OK (threshold +25%)`, `exit: 0`.

**Step 6: Verify the gate FAILS on an injected regression (manual sanity, do NOT commit the edit)**

Temporarily halve one baseline value (e.g. `bar_large.alloc_bytes`) so current looks like a >25% regression, then:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check; echo "exit: $?"
```

Expected: a `REGRESSION bar_large: ...` line and `exit: 1`. Then restore the baseline:

```bash
git checkout crates/fulgur-chart/benches/membench_baseline.json
```

**Step 7: Verify clippy on the membench target (feature on)**

Run:

```bash
cargo clippy -p fulgur-chart --bench membench --features dhat-heap -- -D warnings
```

Expected: no warnings.

**Step 8: Commit code + baseline**

```bash
git add crates/fulgur-chart/benches/membench.rs \
        crates/fulgur-chart/benches/membench_baseline.json \
        crates/fulgur-chart/Cargo.toml
git commit -m "perf(bench): add dhat memory measurement with committed baseline gate"
```

---

## Task 6: Wire the `perf` CI job

**Files:**
- Modify: `.github/workflows/ci.yml` (add a new `perf` job)

**Step 1: Add the job**

Edit `.github/workflows/ci.yml`. Add a new job under `jobs:` (sibling of `test`, `coverage`, etc.). Use the same toolchain/cache pattern as the existing jobs:

```yaml
  perf:
    name: Perf (memory gate + speed report)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          key: perf

      # Deterministic dhat allocation gate: fails on extreme memory regressions.
      - name: Memory regression gate (dhat)
        run: cargo bench -p fulgur-chart --bench membench --features dhat-heap --locked -- --check

      # Speed benchmarks are informational only (wall-clock is noisy on shared
      # runners), so this never gates — we just run and archive the numbers.
      - name: Render speed benchmark (report-only)
        run: cargo bench -p fulgur-chart --bench render --locked

      - name: Upload criterion report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: criterion-report
          path: target/criterion
          if-no-files-found: ignore
```

**Step 2: Validate the workflow YAML locally**

Run:

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('yaml ok')"
```

Expected: `yaml ok`.

**Step 3: Reproduce the CI gate command locally**

Run exactly what CI runs:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap --locked -- --check; echo "exit: $?"
```

Expected: `memory check OK`, `exit: 0`.

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add perf job (dhat memory gate + report-only criterion speed)"
```

---

## Task 7: Docs (benches README + CHANGELOG)

**Files:**
- Create: `crates/fulgur-chart/benches/README.md`
- Modify: `CHANGELOG.md`

**Step 1: Write the benches README**

Create `crates/fulgur-chart/benches/README.md`:

```markdown
# Benchmarks

Two bench targets measure rendering performance for representative chart cases
(small + synthetic-large), generated in `cases.rs`.

## Speed (`render`) — report-only

criterion times the E2E pipeline (JSON → SVG, JSON → PNG). It never gates CI:
wall-clock on shared runners is too noisy. CI archives `target/criterion`.

```bash
cargo bench -p fulgur-chart --bench render            # full run
cargo bench -p fulgur-chart --bench render -- --test  # quick smoke (each case once)
```

## Memory (`membench`) — deterministic gate

dhat measures per-case allocation bytes (deterministic), compared against the
committed `membench_baseline.json`. CI fails if any case exceeds the baseline by
more than the threshold (default +25%).

```bash
# Print current numbers
cargo bench -p fulgur-chart --bench membench --features dhat-heap

# Gate against the baseline (what CI runs)
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check

# Custom threshold
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check --threshold 30
```

### Updating the baseline

When an intentional change alters allocations (including adding/removing a case),
regenerate and commit the baseline:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --update
git add crates/fulgur-chart/benches/membench_baseline.json
```

The `dhat-heap` feature is required for `membench` (it installs the dhat global
allocator); `required-features` keeps dhat out of normal builds and tests.
```

**Step 2: Add a CHANGELOG entry**

Open `CHANGELOG.md`, find the top `[Unreleased]` section (or the current top entry — match the existing format), and add under it:

```markdown
### Added

- Performance benchmarks: criterion E2E render-speed bench (`render`, report-only)
  and a deterministic dhat memory-allocation gate (`membench`) with a committed
  baseline. CI fails on extreme memory regressions; speed numbers are archived as
  an artifact. (fulgur-chart-g0g)
```

Match the surrounding heading style/placement; if there is no `[Unreleased]` section, mirror the format of the newest entry.

**Step 3: Verify**

```bash
python3 -c "print('changelog edited')"
sed -n '1,30p' CHANGELOG.md
```

Expected: the new entry appears near the top.

**Step 4: Commit**

```bash
git add crates/fulgur-chart/benches/README.md CHANGELOG.md
git commit -m "docs(bench): document speed/memory benchmarks and baseline workflow"
```

---

## Final Verification (after all tasks)

Run the full local gate set to confirm nothing regressed:

```bash
# Normal build/test/clippy must be unaffected by the optional dhat dep
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check

# Speed smoke + memory gate
cargo bench -p fulgur-chart --bench render -- --test
cargo bench -p fulgur-chart --bench membench --features dhat-heap --locked -- --check
```

**Acceptance (from issue fulgur-chart-g0g):**
- `cargo bench --bench render` produces criterion results for all cases. ✅ Task 3
- `cargo bench --bench membench --features dhat-heap -- --check` passes vs baseline and fails over threshold. ✅ Task 5
- CI `perf` job: memory gate hard-fails on extreme regression, speed is report-only (artifact). ✅ Task 6
- `membench_baseline.json` committed; dhat numbers deterministic. ✅ Task 5
- `benches/README.md` documents run + baseline-update workflow. ✅ Task 7
```
