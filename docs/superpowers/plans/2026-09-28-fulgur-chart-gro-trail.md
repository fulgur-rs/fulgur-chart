# Vega-Lite Trail Mark Implementation Plan

**Goal:** Implement Vega-Lite `mark: "trail"` with categorical and temporal x encodings, optional quantitative width encoding, native SVG/PNG rendering, examples, and regression coverage.

**Architecture:** Add a distinct `ChartKind::Trail` and keep its aggregated point widths aligned in `Series`. Reuse the existing line parser, axes, gaps, and decimation; render each retained segment as a clipped, filled outline path with bounded joins. Keep the line mark code path unchanged for existing line inputs.

**Tech Stack:** Rust, serde/serde_json, schemars, existing `Scene`/`Prim::ClippedPath`, cargo tests, PNG golden fixtures.

**Spec:** [docs/superpowers/specs/2026-09-28-fulgur-chart-gro-trail.md](../specs/2026-09-28-fulgur-chart-gro-trail.md)

## Global Constraints

- Accept categorical and temporal x with the same channel forms and ordering as the existing line mark.
- Accept only inline `data.values`; the supported encoding channels are x, y, optional color, and optional quantitative size.
- Reject missing, null, non-numeric, or non-finite size values and non-finite aggregated size values.
- Aggregate repeated category/color or timestamp/color keys by summing y and size independently.
- Map size values across all color groups to widths from 1 to 4 pixels; map a constant domain to 2.5 pixels; use a uniform 1 pixel width when size is omitted.
- Draw with fill and no stroke or point markers; do not support trail interpolation or stacking.
- Keep existing `mark: "line"` output unchanged.
- Merge only after required CI succeeds, then close the Beads issue and remove the worktree.

## Review Focus

- Extreme finite size endpoints whose subtraction overflows must still map to finite widths in `[1, 4]`; test the full `f64` finite range.
- Repeated categorical and temporal keys must aggregate y and size in the same groups and reject either aggregate overflowing; test both x modes.
- Sparse temporal timestamp/color pairs must retain the existing temporal line error rather than shifting a width to another point; add a parser regression case.
- A gap or decimation must preserve the source category index for width lookup; test a retained point whose width differs from adjacent points.
- Duplicate coordinates, acute reversals, and single-point segments must produce finite, bounded geometry or no path; test coordinates and final clipped scene output.

## File Structure

- `crates/fulgur-chart/src/ir.rs`: add the trail chart kind and point-aligned trail widths.
- `crates/fulgur-chart/src/schema/vegalite.rs`: add string/object trail mark schemas and categorical/temporal trail spec variants.
- `crates/fulgur-chart/src/frontend/vegalite.rs`: validate trail input, aggregate size with y, and map the complete size domain to pixel widths.
- `crates/fulgur-chart/src/guard.rs`, `layout/mod.rs`, `layout/common.rs`, and `model.rs`: admit trail through line-compatible validation, Cartesian layout, and semantic model reporting.
- `crates/fulgur-chart/src/layout/line.rs`: generate and render the trail outline geometry using retained line segment indices; keep the pure geometry helper's unit tests beside it.
- `crates/fulgur-chart/tests/frontend_vegalite.rs`: schema, parser, aggregation, and semantic-model regression tests.
- `crates/fulgur-chart/tests/render_vegalite_trail.rs`: SVG/PNG scene behavior tests using the trail examples.
- `crates/fulgur-chart/tests/golden_png.rs`: parse Vega-Lite fixtures through the Vega-Lite frontend and register both examples.
- `examples/specs/vegalite-trail-categorical.json` and `examples/specs/vegalite-trail-temporal.json`: supported-input examples and golden sources.
- `crates/fulgur-chart/tests/golden/vegalite-trail-categorical.png` and `crates/fulgur-chart/tests/golden/vegalite-trail-temporal.png`: committed raster goldens.
- `README.md`: document trail support and its size mapping.

---

## Task 1: Trail IR, schema, and parser

**Files:**

- Modify `crates/fulgur-chart/src/ir.rs`.
- Modify `crates/fulgur-chart/src/schema/vegalite.rs`.
- Modify `crates/fulgur-chart/src/frontend/vegalite.rs`.
- Modify `crates/fulgur-chart/src/guard.rs`, `crates/fulgur-chart/src/layout/mod.rs`, `crates/fulgur-chart/src/layout/common.rs`, and `crates/fulgur-chart/src/model.rs`.
- Modify `crates/fulgur-chart/tests/frontend_vegalite.rs`.

**Interfaces:**

- Add `ChartKind::Trail` as a non-stacked Cartesian path mark.
- Add `Series.trail_widths: Vec<f64>`; after parsing, each entry is a finite pixel width in `[1.0, 4.0]` aligned with `Series.values`, and non-trail series leave it empty.
- Keep trail size aggregation inside the categorical builder and map all aggregated values together with a finite, overflow-safe linear scale from `[1.0, 4.0]`; a constant domain maps to `2.5`, and omitted size fills each point with `1.0`.
- Preflight category×series allocation before constructing aligned values and widths, preserving first-seen category and group order.
- Extend the temporal line aggregation path with an optional size field; `mark: "line"` passes `None`, while trail aggregates size beside y and returns the same sorted temporal domain as line.

1. Add schema/parser tests in `tests/frontend_vegalite.rs` for both mark forms, ChartKind/model type, categorical and temporal axes, all-color-group scaling, constant/null/omitted-size behavior, negative and extreme finite values, duplicate-key sums, explicit errors in both strict modes for extra mark/size properties and invalid values, sparse groups, and aggregate overflow. Add the width-vector guard test beside the guard implementation.
2. Run focused tests after adding them; observe the expected parser/guard failures before implementation.
3. Add trail variants to `VegaLiteSpec`, mark wrappers, and categorical/temporal encodings. Update `parse_mark`, strict allow-lists, required field/type/value validation, category and temporal aggregation, and the global width-scaling helper. Keep line calls on the current no-size path.
4. Add `ChartKind::Trail` handling to model chart type/axes/dimensions, line-compatible layout dispatch, temporal-position guards, plot-area validation, and every `Series` literal's empty-width default. Validate width vector length, finiteness, and the `[1, 4]` bound in `guard.rs` for manually constructed specs.
5. Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_trail` and `cargo check -p fulgur-chart`; confirm the new parser/model cases pass and existing line parser behavior remains unchanged.
6. Commit the task as `feat(vegalite): parse trail mark`.

## Task 2: Variable-width trail geometry

**Files:**

- Modify `crates/fulgur-chart/src/layout/line.rs`.
- Modify `crates/fulgur-chart/tests/render_vegalite_trail.rs`.

**Interfaces:**

- Add `fn trail_outline_path(points: &[(f64, f64, usize)], widths: &[f64]) -> Option<String>` in `layout/line.rs`. It returns one closed path for a segment with at least two distinct points; each retained point carries its source category index for width lookup.
- A duplicate-coordinate run is collapsed to one coordinate carrying its widest width. Ignore zero-length edges when deriving tangents.
- Use linear taper between points, butt caps, miter joins limited to four times the local half-width, and bevel joins when the miter is unstable or exceeds the limit.
- `layout::line::build` uses `Prim::ClippedPath` with `fill: Some(series.fill_at(0))`, no stroke, and the existing plot rectangle for clipping. It selects widths by each retained point's category index.

1. Add unit tests for point widths, butt caps, bounded bevels, duplicate coordinates, gaps, single-point segments, and retained-width lookup in `layout/line.rs`. Add integration tests named `trail_scene_splits_gaps_and_clips_to_plot` and `trail_decimation_keeps_widths_aligned` in `tests/render_vegalite_trail.rs`.
2. Run the new geometry and renderer tests and observe the expected missing-path failures before implementation.
3. Implement `trail_outline_path`; use normalized tangents and offset edge intersections, emit bevel points for unstable/over-limit joins, collapse repeated coordinates to the widest width, and format every coordinate with `fmt_num`.
4. Add a `ChartKind::Trail` branch in line rendering that builds one clipped filled path for each retained segment, uses category indices to select `trail_widths`, suppresses the existing constant-width stroke and point markers, and preserves gap/decimation behavior.
5. Assert in unit and renderer tests that the scene contains no trail stroke/marker, all path coordinates are finite, clip bounds equal the plot rectangle, a one-point segment emits no path, and a gap creates separate paths. Run `cargo test -p fulgur-chart trail_outline_` and `cargo test -p fulgur-chart --test render_vegalite_trail trail_`; confirm all cases pass.
6. Commit the task as `feat(layout): render variable-width trails`.

## Task 3: Examples, documentation, and raster goldens

**Files:**

- Create `examples/specs/vegalite-trail-categorical.json` and `examples/specs/vegalite-trail-temporal.json`.
- Modify `README.md` and `crates/fulgur-chart/tests/golden_png.rs`.
- Create the two matching PNG files under `crates/fulgur-chart/tests/golden/`.
- Extend `crates/fulgur-chart/tests/render_vegalite_trail.rs` to use both examples.

1. Add a categorical example with multiple color groups and a non-constant quantitative size field, and a temporal example with timestamps, color groups, and size values.
2. Add both example names to `golden_png.rs::NAMES`. In `render_to_png`, parse input JSON once to select `vegalite::parse` when a top-level `mark` exists and `chartjs::parse` otherwise.
3. Add the trail/size description to README's Vega-Lite supported subset and include a short CLI render invocation.
4. Render and create only these goldens with `UPDATE_GOLDEN=vegalite-trail-categorical cargo test -p fulgur-chart --test golden_png golden_png_matches` and the matching temporal command. Confirm the single-fixture update mode changes only its target PNG.
5. Run `cargo test -p fulgur-chart --test render_vegalite_trail` and `cargo test -p fulgur-chart --test golden_png`; confirm both trail examples parse, render, and compare against committed goldens.
6. Commit the task as `docs(vegalite): add trail examples and goldens`.

## Task 4: Full verification and pull request

1. Run `cargo fmt --all -- --check` and `git diff --check`.
2. Run `cargo test -p fulgur-chart` and `cargo clippy -p fulgur-chart --all-targets -- -D warnings`; resolve failures and rerun each failing command.
3. Review the final diff for changes outside trail support and verify the existing Vega-Lite line fixtures produce the same output.
4. Push the feature branch and create an English-titled PR for `fulgur-chart-gro` with the spec, implementation summary, and verification results.
5. Monitor required GitHub Actions until all checks pass; address actionable review findings and rerun required checks.
6. Merge the PR, close `fulgur-chart-gro` with the merged PR reference, push Beads updates, and remove `.worktrees/fulgur-chart-gro` after confirming the PR is merged.
