# PNG membench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Track execution state in `bd`.

**Goal:** Gate deterministic allocation regressions for both SVG and PNG rendering across all existing benchmark cases.

**Architecture:** Add a bench-only target-expansion and rendering module shared with integration tests through `#[path]`. The existing `membench` executable will measure each target with its unchanged `HeapStats` delta logic, retain current SVG baseline names, and add `_png` keys for PNG measurements.

**Tech Stack:** Rust, dhat `HeapStats`, tiny-skia-backed PNG rendering, Cargo integration tests and bench targets.

## Global Constraints

- Use all nine existing cases from `crates/fulgur-chart/benches/cases.rs`.
- Preserve current SVG baseline keys exactly.
- Name each PNG baseline key `<case>_png`.
- Render PNG at scale `1.0` with `render_chart_to_png_default`.
- Do not change the +25% default threshold or regression comparison semantics.
- Keep helper code bench-only; do not add public `fulgur-chart` API.
- New changed lines must have corresponding tests.

---

### Task 1: Define and test SVG/PNG measurement targets

**Files:**
- Create: `crates/fulgur-chart/benches/membench_targets.rs`
- Create: `crates/fulgur-chart/tests/membench_targets.rs`

**Interfaces:**
- Consumes: `cases::Case`, `fulgur_chart::frontend::chartjs::parse`, `render::render_chart`, and `raster_direct::render_chart_to_png_default`.
- Produces: `OutputKind`, `MeasurementTarget<'a>`, `all(&[Case]) -> Vec<MeasurementTarget<'_>>`, and `render(&MeasurementTarget<'_>) -> Result<Vec<u8>, String>`.

#### Step 1: Write the failing target-expansion test

Create `crates/fulgur-chart/tests/membench_targets.rs` with:

```rust
#[path = "../benches/cases.rs"]
mod cases;
#[path = "../benches/membench_targets.rs"]
mod membench_targets;

use membench_targets::OutputKind;

#[test]
fn every_case_has_svg_and_png_measurement_targets() {
    let cases = cases::all();
    let targets = membench_targets::all(&cases);

    assert_eq!(targets.len(), cases.len() * 2);
    for case in &cases {
        let svg = targets
            .iter()
            .find(|target| target.name == case.name)
            .unwrap_or_else(|| panic!("missing SVG target for {}", case.name));
        assert_eq!(svg.output, OutputKind::Svg);

        let png_name = format!("{}_png", case.name);
        let png = targets
            .iter()
            .find(|target| target.name == png_name)
            .unwrap_or_else(|| panic!("missing PNG target for {}", case.name));
        assert_eq!(png.output, OutputKind::Png);
    }

    let mut names: Vec<&str> = targets.iter().map(|target| target.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), targets.len(), "target names must be unique");
}
```

#### Step 2: Run the target test and verify RED

Run:

```bash
cargo test -p fulgur-chart --test membench_targets every_case_has_svg_and_png_measurement_targets
```

Expected: compilation fails because `benches/membench_targets.rs` does not exist.

#### Step 3: Implement minimal target expansion

Create `crates/fulgur-chart/benches/membench_targets.rs` with:

```rust
//! Measurement targets shared by the memory bench and integration tests.

use crate::cases::Case;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputKind {
    Svg,
    Png,
}

pub struct MeasurementTarget<'a> {
    pub name: String,
    pub case: &'a Case,
    pub output: OutputKind,
}

pub fn all(cases: &[Case]) -> Vec<MeasurementTarget<'_>> {
    cases
        .iter()
        .flat_map(|case| {
            [
                MeasurementTarget {
                    name: case.name.to_string(),
                    case,
                    output: OutputKind::Svg,
                },
                MeasurementTarget {
                    name: format!("{}_png", case.name),
                    case,
                    output: OutputKind::Png,
                },
            ]
        })
        .collect()
}
```

#### Step 4: Run the target test and verify GREEN

Run:

```bash
cargo test -p fulgur-chart --test membench_targets every_case_has_svg_and_png_measurement_targets
```

Expected: the target-expansion test passes.

#### Step 5: Write the failing real-render dispatch test

Append to `crates/fulgur-chart/tests/membench_targets.rs`:

```rust
#[test]
fn targets_render_the_selected_output_format() {
    let cases = cases::all();
    let targets = membench_targets::all(&cases[..1]);

    let svg = membench_targets::render(&targets[0]).expect("SVG target renders");
    assert!(svg.starts_with(b"<svg"));

    let png = membench_targets::render(&targets[1]).expect("PNG target renders");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}
```

#### Step 6: Run the render-dispatch test and verify RED

Run:

```bash
cargo test -p fulgur-chart --test membench_targets targets_render_the_selected_output_format
```

Expected: compilation fails because `render` does not exist.

#### Step 7: Implement minimal render dispatch

Add to `crates/fulgur-chart/benches/membench_targets.rs`:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png_default;
use fulgur_chart::render::render_chart;

pub fn render(target: &MeasurementTarget<'_>) -> Result<Vec<u8>, String> {
    let spec = chartjs::parse(&target.case.json, false)?;
    match target.output {
        OutputKind::Svg => Ok(render_chart(&spec).into_bytes()),
        OutputKind::Png => render_chart_to_png_default(&spec, 1.0),
    }
}
```

#### Step 8: Run all target tests and verify GREEN

Run:

```bash
cargo test -p fulgur-chart --test membench_targets
```

Expected: both tests pass with no warnings.

#### Step 9: Commit the target module and tests

```bash
git add crates/fulgur-chart/benches/membench_targets.rs crates/fulgur-chart/tests/membench_targets.rs
git commit -m "test(bench): define PNG memory targets"
```

### Task 2: Measure PNG targets and commit their baseline

**Files:**
- Modify: `crates/fulgur-chart/benches/membench.rs`
- Modify: `crates/fulgur-chart/benches/membench_baseline.json`
- Modify: `crates/fulgur-chart/benches/README.md`

**Interfaces:**
- Consumes: Task 1's `membench_targets::all` and `membench_targets::render`.
- Produces: 18 deterministic baseline entries: nine existing SVG names and nine `_png` names.

#### Step 1: Integrate measurement targets into the bench

In `crates/fulgur-chart/benches/membench.rs`:

```rust
#[path = "membench_targets.rs"]
mod membench_targets;
```

Replace the current `for case in cases::all()` loop with:

```rust
let cases = cases::all();
for target in membench_targets::all(&cases) {
    let before = dhat::HeapStats::get();
    let rendered = membench_targets::render(&target)
        .unwrap_or_else(|e| panic!("case {} renders: {e}", target.name));
    let after = dhat::HeapStats::get();
    std::hint::black_box(&rendered);
    out.insert(
        target.name,
        CaseStat {
            alloc_bytes: after.total_bytes - before.total_bytes,
            alloc_blocks: after.total_blocks - before.total_blocks,
        },
    );
}
```

Update the `measure` documentation to describe both SVG and PNG E2E paths.

#### Step 2: Run the membench check and verify the integration failure

Run:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check
```

Expected: all nine `_png` cases are reported missing from the committed baseline.

#### Step 3: Generate the new deterministic baseline

Run:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --update
```

Expected: `membench_baseline.json` contains the existing nine SVG keys and nine new `_png` keys.

#### Step 4: Document dual-path memory gating

Change the Memory section of `crates/fulgur-chart/benches/README.md` to state that dhat measures the E2E JSON-to-SVG and JSON-to-PNG allocation volume for every representative case, and that `_png` identifies PNG baseline entries.

#### Step 5: Run focused verification

Run:

```bash
cargo test -p fulgur-chart --test bench_cases --test membench_check --test membench_targets
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check
cargo clippy -p fulgur-chart --bench membench --features dhat-heap -- -D warnings
cargo fmt --all --check
git diff --check
```

Expected: every command exits zero; the bench reports 18 cases and `memory check OK`.

#### Step 6: Commit implementation, baseline, and docs

```bash
git add crates/fulgur-chart/benches/membench.rs \
  crates/fulgur-chart/benches/membench_baseline.json \
  crates/fulgur-chart/benches/README.md
git commit -m "perf(bench): gate PNG memory allocations"
```

### Task 3: Complete repository gates and publish

**Files:**
- Verify only; update implementation files if a gate exposes an issue.

**Interfaces:**
- Consumes: the completed dual-path memory gate.
- Produces: a pushed feature branch and a closed Beads issue.

#### Step 1: Run the broader Rust quality gates

```bash
cargo test -p fulgur-chart
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

Expected: all tests pass, clippy emits no warnings, and formatting is clean.

#### Step 2: Verify committed state and requirement coverage

```bash
git status --short
git log --oneline --decorate -3
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check
```

Expected: no uncommitted files, the design and implementation commits are present, and all 18 memory measurements pass.

#### Step 3: Close and persist the Beads issue

```bash
bd close 627 --reason="Implemented PNG allocation measurements for every membench case and verified the baseline gate."
bd dolt push
```

#### Step 4: Rebase and push the feature branch

```bash
git pull --rebase
git push -u origin feat/627-png-membench
git status --short --branch
```

Expected: the branch is up to date with `origin/feat/627-png-membench`.
