# QuickChart Violin Chart Implementation Plan

**Goal:** Add QuickChart-compatible vertical and horizontal violin charts without changing boxplot behavior.

**Architecture:** Parse raw per-category observations into a dedicated IR field. A new violin layout module computes a Gaussian KDE for each non-empty category group and paints a clipped symmetric body with mean and median markers. Model reporting, resource guards, documentation, examples, and PNG goldens then consume the new chart kind.

**Tech Stack:** Rust, serde/schemars, the existing `fulgur-chart` IR/layout/scene pipeline, Cargo tests, committed PNG goldens.

**Spec:** `docs/superpowers/specs/2026-09-25-violin-chart-design.md`

## Global Constraints

- Support QuickChart types `violin` and `horizontalViolin`; preserve existing boxplot input and rendering.
- Use 100 density positions and bandwidth `h = 1.06 * min(sample_standard_deviation, IQR / 1.34) * n^(-1/5)`, with type-7 quartiles.
- For singleton or unusable bandwidth, use max(0.01 * value_axis_span, 1e-9) in data units; if the span subtraction overflows, use one percent of the larger absolute axis bound before the 1e-9 lower bound. Singleton/constant groups evaluate around the mean and extend the automatic domain; non-constant zero-IQR groups retain their observed range.
- Count every raw sample slot, including nulls, against `max_total_data_points`; estimate KDE work as finite sample count × 100 with saturating arithmetic and cap it at `max_total_data_points.saturating_mul(100)`.
- Count no more than three categorical primitives per non-empty group: body, median, and mean.
- Draw vertical categories on x and horizontal categories on y. Normalize public model axes to category x and value y in both orientations.
- Keep implementation linear in `sample_count * 100`; do not add adaptive sampling or user KDE options.

## Review Focus

- A constant group or singleton has a finite positive fallback density and remains visible.
- An all-null outer data array maps to empty groups despite the parser's untagged variant precedence; flat non-null numbers remain invalid for violin.
- Inner null samples do not enter KDE, but still count toward the raw input limit.
- Horizontal hard x bounds clip every generated path and marker to the plot frame; model axes still report category x/value y.
- Multiple datasets occupy separate symmetric slots per category and retain their own resolved colors and series indices.

## Files and Responsibilities

- `crates/fulgur-chart/src/ir.rs`: chart-kind orientation and raw sample groups.
- `crates/fulgur-chart/src/schema/chartjs.rs`: top-level chart schema and nested nullable sample arrays.
- `crates/fulgur-chart/src/frontend/chartjs.rs`: chart-name parsing, axis selection, data-shape validation/conversion, parser tests.
- `crates/fulgur-chart/src/layout/violin.rs`: value-domain extension, KDE, orientation-aware frame, clipping, path and marker geometry.
- `crates/fulgur-chart/src/layout/mod.rs`: scene dispatch.
- `crates/fulgur-chart/src/model.rs`: type names, normalized axes, element counts, model tests.
- `crates/fulgur-chart/src/guard.rs`: raw-slot, KDE-work, and categorical-primitive bounds with guard tests.
- `README.md`, `examples/README.md`, `examples/specs/`, `crates/fulgur-chart/tests/golden_png.rs`, `crates/fulgur-chart/tests/golden/`: user documentation and representative renders.

## Implementation Tasks

### Task 1: Add violin IR, schema, and parser

**Consumes:** Existing `DataField::{Nums, Boxes, Points}`, `ChartKind`, and `Series`.

**Produces:** `ChartKind::Violin { horizontal: bool }` and `Series.violin_samples: Vec<Vec<Option<f64>>>`; schema variants named exactly `violin` and `horizontalViolin`.

1. Add parser tests first for vertical and horizontal type selection, nested samples, outer-null and empty groups, inner-null retention, all-null outer input, flat numeric rejection, malformed non-null samples, and `ChartJsSpec` schema round-trip preserving both exact chart names. Use `cargo test -p fulgur-chart parse_violin` to observe the expected missing-variant/parser failures.
2. Add the two schema variants. Give `HorizontalViolin` an explicit serde name because the union's default rule lowercases variant names. Model each datum as an optional group of nullable numeric samples.
3. Add the IR fields. Keep `box_points` and `BoxPlot` unchanged.
4. Extend `DataField` with nullable nested samples after `Boxes`. Convert `Boxes` and the new nested variant to violin groups; for `Nums` accept only empty/all-null rows as empty groups and reject non-null flat values. Preserve boxplot's current `Nums`/`Boxes` behavior.
5. Include violin in null-data support and in index-axis selection (`x` for vertical, `y` for horizontal). Preserve raw null slots for guard accounting.
6. Run `cargo test -p fulgur-chart parse_violin` and the existing boxplot parser tests. The new parser cases must pass while boxplot cases retain their prior results.

Representative parser assertion:

```rust
let spec = chartjs::parse(
    r#"{"type":"violin","data":{"labels":["A"],"datasets":[{"data":[[1,null,3]]}]}}"#,
    false,
).unwrap();
assert_eq!(spec.series[0].violin_samples, vec![vec![Some(1.0), None, Some(3.0)]]);
```

### Task 2: Implement vertical and horizontal KDE layout

**Consumes:** Task 1's `ChartKind::Violin` and `Series.violin_samples`.

**Produces:** `layout::violin::compute_frame` and `layout::violin::build`, plus scene dispatch.

1. Add layout tests before production code for one symmetric body, grouped datasets, mean/median markers, automatic value-domain coverage, hard bounds, singleton and constant inputs in both orientations. Use `cargo test -p fulgur-chart layout::violin` and confirm failures identify missing layout behavior.
2. Add `layout/violin.rs` with separate helpers for finite sample extraction, type-7 quartiles, normal-reference bandwidth, 100-point Gaussian density sampling, and finite fallback bandwidth.
3. Compute one value domain across all finite violin samples while preserving suggestions; use the numeric axis appropriate to orientation and respect hard min/max bounds.
4. Build a category band per label and dataset slot. Normalize each density by its own maximum; create one closed symmetric `Prim::Path` body and clipped mean circle/median diamond markers. Empty/all-null groups yield no primitives.
5. Add vertical and horizontal frame mapping. For horizontal geometry map values to x and density width to y while using the value x-axis; for vertical geometry map values to y and density width to x.
6. Register `layout::violin` and dispatch both orientations through the new chart kind.
7. Run the layout tests plus existing boxplot layout tests. Confirm all generated path coordinates and markers are finite and inside the hard-bounded plot region.

Representative finite-output assertion:

```rust
assert!(scene.items.iter().any(|item| matches!(item, Prim::Path { .. })));
assert!(scene.items.iter().all(|item| match item {
    Prim::Path { d, .. } => !d.contains("NaN") && !d.contains("inf"),
    _ => true,
}));
```

### Task 3: Integrate model reporting and resource guards

**Consumes:** The parsed IR and `layout::violin::compute_frame` from Tasks 1–2.

**Produces:** Correct model type/axes/counts and bounded raw/KDE work.

1. Add tests for `violin` and `horizontalViolin` model type names, category-x/value-y normalized axes, and one reported element per category slot. Assert `meta.type` preserves the exact public spelling for each orientation. Add guard tests showing null sample slots count against the raw point limit, KDE work is rejected above the saturating limit, and categorical primitive estimates use at most three per non-empty group. Run the targeted tests and observe their expected failures.
2. Extend `model.rs` type naming, `element_count`, and `compute_axes`; for horizontal charts read the rendered numeric domain from the horizontal value axis but retain the established category-x/value-y model normalization.
3. Extend `guard.rs` total-point accounting with saturating violin raw-slot counts. Estimate KDE operations using only finite samples and 100 density positions. Add the violin-specific categorical primitive bound for non-empty groups.
4. Run the targeted model and guard tests plus existing model/guard tests. Confirm `BoxPlot` and horizontal bar axis results remain unchanged.

### Task 4: Document, example, and golden both orientations

**Consumes:** The complete parser, layout, model, and guard behavior from Tasks 1–3.

**Produces:** Discoverable QuickChart-compatible examples and visual regression coverage.

1. Add `examples/specs/violin.json` and `examples/specs/violin-horizontal.json`, each with multiple categories and deliberately non-symmetric observations. Include a missing/empty group in one example.
2. Update `README.md` supported types and nested raw-sample input description; update `examples/README.md` inventory.
3. Add both spec names to `crates/fulgur-chart/tests/golden_png.rs`. Run `UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png`, then inspect `git status` and ensure only the two new violin PNGs and intended test changes were created. Run the same golden test without `UPDATE_GOLDEN` and require it to pass.

### Task 5: Full verification and PR readiness

**Consumes:** Tasks 1–4.

**Produces:** A clean branch with all project checks passing and a reviewable diff.

1. Run `cargo fmt --all -- --check`.
2. Run `cargo test -p fulgur-chart` and the golden PNG integration test without update mode; read the complete results.
3. Run the repository CI-equivalent Rust checks documented in the workflow files, and verify the schema round-trip/parser, guard, both orientations, and boxplot regression coverage are included.
4. Review the complete diff for unintended boxplot changes and generated-file noise. Commit implementation tasks with focused messages, create the PR from this worktree, wait for required CI, and merge it with a regular merge commit after the checks pass.
