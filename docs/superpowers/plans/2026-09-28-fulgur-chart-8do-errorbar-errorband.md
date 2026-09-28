# Vega-Lite errorbar / errorband Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Vega-Lite `errorbar` and `errorband` for raw and pre-aggregated inline data, with shared native/WASM range geometry.

**Architecture:** Normalize both input forms into `ErrorMarkData` stored in a chart-kind-specific IR variant. Resolve raw statistics before rendering, build axes and clipped Scene primitives in `layout/error_mark.rs`, and keep color-series metadata in the existing `ChartSpec.series` for legends and the semantic model.

**Tech Stack:** Rust 2024, serde/serde_json, schemars, existing `ChartSpec` / `Scene` renderers, cargo tests, WASM bindings, PNG goldens.

**Spec:** [docs/superpowers/specs/2026-09-28-fulgur-chart-8do-errorbar-errorband.md](../specs/2026-09-28-fulgur-chart-8do-errorbar-errorband.md)

## Global Constraints

- Accept `data.values` inline records and both mark forms: string and `{ "type": ... }`.
- Accept raw statistics `stderr`, `stdev`, `ci`, and `iqr`; default to `stderr`.
- For `stderr`, use sample standard deviation / √n; for `stdev`, use sample standard deviation with n−1 denominator.
- For `ci`, use 1,000 bootstrap resamples of n values, percentile endpoints 2.5% / 97.5% with linear interpolation, and a seed derived from the group key.
- Limit total bootstrap draws to 10,000,000 before allocating or sampling; error instead of truncating.
- For `iqr`, use type-7 q1 / q3; reject `stderr` / `stdev` / `ci` groups with fewer than two samples.
- Accept pre-aggregated lower/upper (`x`+`x2` or `y`+`y2`) and center/error channels (`x`+`xError`[/`xError2`] or `y`+`yError`[/`yError2`]); reject mixed forms and malformed offsets.
- Support `horizontal` and `vertical` orientations, categorical / temporal / quantitative independent axes, and 1D marks with their specified center/full-axis behavior.
- Support constant or nominal/ordinal field color; reject quantitative color and field opacity. Constant opacity is within 0..1.
- Default mark color to `#4682b4` and mark opacity to 1; propagate them to each component unless a part style overrides that attribute.
- `errorbar` defaults to rule on / ticks off. `errorband` defaults to band opacity 0.3 / borders off. A false component flag hides that component; a true flag or style object enables it.
- Support every listed Vega-Lite v6 errorband interpolation; reject interpolation/tension on 1D bands.
- Keep URL data, generic transforms, layer/concat, facet, tooltip, and selection outside this change.
- Native and WASM use the same IR, guard, layout, and Scene path. Regenerate the embedded WASM Vega-Lite schema after schema changes.

## Review Focus

- A singleton group cannot produce `stderr`, `stdev`, or `ci`; the statistics helper rejects n < 2 in Task 2 and `vegalite_error_extent_rejects_single_sample_groups` pins parser behavior in Task 3.
- Asymmetric `xError` / `xError2` and `yError` / `yError2` must map to the correct upper/lower endpoints, and lower/upper input must reject reversed endpoints; `vegalite_error_mark_rejects_malformed_ranges` pins both forms in Task 3.
- The same independent coordinate repeated in one pre-aggregated errorband series is ambiguous; `vegalite_errorband_rejects_duplicate_independent_positions` pins the error in Task 3.
- Non-positive values on either logarithmic axis must fail, while positive ranges cut by hard bounds must clip at the plot rectangle; `error_mark_log_values_fail_and_hard_bounds_clip` pins both behaviors in Task 4.
- A two-dimensional errorband group with one point has zero area and must emit no fill path without panicking; `errorband_single_point_group_emits_no_fill` pins this in Task 5.

---

## File Structure

- `crates/fulgur-chart/src/ir.rs`: define the shared error-mark kind, orientation, independent-coordinate, range-point, style, and interpolation IR; add one `ChartKind::ErrorMark` variant in Task 3 without adding range storage to every `Series`.
- `crates/fulgur-chart/src/schema/vegalite.rs`: define strict typed schemas for errorbar/errorband mark forms, range/error channels, color/opacity, and part styles; register both spec variants.
- `crates/fulgur-chart/src/frontend/vegalite.rs` and `frontend/mod.rs`: route both marks into normalized error-range parsing and keep existing common chart defaults / title / axes behavior; register the statistics helper module.
- `crates/fulgur-chart/src/frontend/vegalite_error.rs`: implement pure statistics helpers for stderr, stdev, bootstrap CI, and IQR, with deterministic seeds and bounded work.
- `crates/fulgur-chart/src/guard.rs`: validate range-point alignment, finite coordinates/endpoints/styles, logarithmic inputs, and generated primitive/work limits.
- `crates/fulgur-chart/src/layout/error_mark.rs`: compute range-aware axes and build errorbar/errorband primitives in the shared Scene.
- `crates/fulgur-chart/src/layout/common.rs`, `bar.rs`, and `scatter.rs`: expose or extend existing axis-frame helpers only where the error-mark layout needs categorical, temporal, or quantitative axis mapping.
- `crates/fulgur-chart/src/layout/mod.rs`: dispatch the new chart kind.
- `crates/fulgur-chart/src/model.rs`: report `errorbar` / `errorband`, color series, counts, and the matching x/y axis models.
- `crates/fulgur-chart/tests/frontend_vegalite.rs`: schema, channel validation, aggregation, grouping, and strict/non-strict error tests.
- `crates/fulgur-chart/src/frontend/vegalite_error.rs` and `crates/fulgur-chart/src/layout/error_mark.rs`: focused pure-statistics and geometry unit tests.
- `crates/fulgur-chart/tests/render_vegalite_error_mark.rs`: Scene-level orientation, style, clipping, interpolation, and one-dimensional regression tests.
- `crates/fulgur-chart/tests/wasm_runtime.rs`: exercise parsing/rendering through the WASM binding.
- `crates/fulgur-chart/tests/golden_png.rs`: add four fixture names to the fixed comparison list.
- `examples/specs/vegalite-errorbar-raw.json`, `vegalite-errorbar-preaggregated.json`, `vegalite-errorband-raw.json`, and `vegalite-errorband-preaggregated.json`: compact input examples for the four supported input/mark combinations.
- `crates/fulgur-chart/tests/golden/vegalite-errorbar-raw.png`, `vegalite-errorbar-preaggregated.png`, `vegalite-errorband-raw.png`, and `vegalite-errorband-preaggregated.png`: representative raster outputs.
- `crates/bindings/wasm/src/vegalite-schema.json`: regenerated from the core schema generator.

## Task 1: Typed schema and IR contracts

**Files:**

- Modify `crates/fulgur-chart/src/schema/vegalite.rs`.
- Modify `crates/fulgur-chart/src/ir.rs`.
- Test `crates/fulgur-chart/tests/frontend_vegalite.rs`.

**Interfaces:**

- Add `ErrorMarkKind::{ErrorBar, ErrorBand}` and `ErrorMarkOrient::{Horizontal, Vertical}`.
- Add `ErrorPosition::{FullAxis, Category(usize), Quantitative(f64), Temporal(i64)}`.
- Add `ErrorRangePoint { series_index: usize, detail: Option<String>, position: ErrorPosition, center: f64, lower: f64, upper: f64 }`; `series_index` indexes `ChartSpec.series`.
- Add `ErrorMarkData { kind: ErrorMarkKind, orient: ErrorMarkOrient, ranges: Vec<ErrorRangePoint>, style: ErrorMarkStyle }`.
- Add `ErrorMarkStyle { opacity: f64, clip: bool, rule: ErrorPartStyle, ticks: ErrorPartStyle, band: ErrorPartStyle, borders: ErrorPartStyle, interpolation: ErrorBandInterpolation, tension: f64 }`.
- Add `ErrorPartStyle { visible: bool, fill: Option<Color>, stroke: Option<Color>, stroke_width: Option<f64>, opacity: Option<f64>, size: Option<f64>, stroke_dash: Vec<f64> }`.
- Add `ErrorBandInterpolation` variants for the 13 Vega-Lite v6 modes listed in the spec.
- Add `MarkErrorBar`, `MarkErrorBand`, `VlErrorBarSpec`, `VlErrorBandSpec`, and shared `VlErrorMarkEncoding`; register `VegaLiteSpec::ErrorBar` and `VegaLiteSpec::ErrorBand`. Both root spec structs include mark, inline data, encoding, and the existing Vega-Lite schema/size/title/background/config fields. The shared encoding has optional field channels x/y, x2/y2, xError/xError2/yError/yError2, and detail, plus typed color (constant value or nominal/ordinal field) and opacity (constant value only); parser validation enforces required combinations. Mark objects admit only `type`, `extent`, `orient`, `color`, `opacity`, `clip`, `rule`/`ticks` (errorbar), or `band`/`borders`/`interpolate`/`tension` (errorband). Range channels are field-only. Errorbar part styles admit color/fill/stroke, strokeWidth, opacity, size, and strokeDash; errorband part styles admit the same fields except size. Enforce finite nonnegative strokeWidth/size and 0..1 opacity/tension. Both string and object mark forms are represented in generated JSON Schema.

- [ ] Add typed-schema tests named `vegalite_error_mark_schema_accepts_both_mark_forms`, `vegalite_error_mark_schema_accepts_range_channels`, and `vegalite_error_mark_schema_rejects_unknown_part_keys`; assert that both variants deserialize to the intended `VegaLiteSpec` arm and malformed style objects fail schema deserialization.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_error_mark_schema`; confirm the new tests fail because the variants/types are absent.
- [ ] Add the error-mark enums/structs in `ir.rs` with the exact fields above; keep `Series` unchanged. `ChartKind::ErrorMark` is added with parser/layout integration in Task 3 so each commit keeps exhaustive matches buildable.
- [ ] Add the errorbar/errorband mark and encoding schemas in `schema/vegalite.rs`; allow only string/nominal/ordinal color and constant opacity forms from the spec. Register both variants in `VegaLiteSpec`.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_error_mark_schema` and `cargo check -p fulgur-chart`; confirm all three schema tests pass and the schema-only change compiles.
- [ ] Commit as `feat(vegalite): define error mark schemas and IR`.

## Task 2: Raw statistics helper

**Files:**

- Modify `crates/fulgur-chart/src/frontend/mod.rs`.
- Create `crates/fulgur-chart/src/frontend/vegalite_error.rs`.
- Test the new helper's unit tests in `crates/fulgur-chart/src/frontend/vegalite_error.rs`.

**Interfaces:**

- Add `pub(super) enum ErrorExtent { Stderr, Stdev, Ci, Iqr }` and `pub(super) struct ErrorRangeSummary { pub center: f64, pub lower: f64, pub upper: f64 }` in `vegalite_error.rs`.
- Add `pub(super) fn summarize(values: &[f64], extent: ErrorExtent, seed: u64) -> Result<ErrorRangeSummary, String>`; this helper has no chart-kind dependency.
- Add `pub(super) fn validate_bootstrap_budget(sample_count: usize) -> Result<usize, String>`; return the checked draw count or reject counts above 10,000,000 before sampling/allocation.

- [ ] Add tests named `error_mark_statistics_stderr_stdev_iqr`, `error_mark_statistics_ci_is_deterministic_per_seed`, `error_mark_statistics_rejects_invalid_samples`, and `error_mark_statistics_preflights_bootstrap_draws`; assert exact endpoints, same-seed repeatability, changed-seed output, n < 2 rejection for stderr/stdev/ci, singleton IQR endpoints, non-finite sample rejection, and the 10,000,000-draw boundary through the preflight helper.
- [ ] Run `cargo test -p fulgur-chart error_mark_statistics`; confirm the helper tests fail because the module/functions are absent.
- [ ] Implement `summarize` using sample standard deviation, type-7 quartiles, 1,000 bootstrap means, linearly interpolated 2.5%/97.5% quantiles, and the seed passed from deterministic serialization of the group key. Check `sample_count × 1,000` with saturating arithmetic before allocating bootstrap samples.
- [ ] Run `cargo test -p fulgur-chart error_mark_statistics`; confirm exact statistics, repeatability, invalid sample handling, and the work limit pass.
- [ ] Commit as `feat(vegalite): add error mark statistics`.

## Task 3: Parser normalization

**Files:**

- Modify `crates/fulgur-chart/src/frontend/{mod.rs,vegalite.rs}`.
- Modify `crates/fulgur-chart/src/{guard.rs,model.rs,layout/mod.rs}` only for the new chart-kind exhaustive arms needed to keep the parser change buildable.
- Test `crates/fulgur-chart/tests/frontend_vegalite.rs`.

**Interfaces:**

- Add `parse_error_mark_spec` and `parse_error_mark_encoding` in `frontend/vegalite.rs`; normalize raw and pre-aggregated inputs to `ErrorMarkData` and `ChartSpec.series`.
- Add `ChartKind::ErrorMark(ErrorMarkData)`; keep the range points inside that variant and the existing color/legend metadata inside `ChartSpec.series`.

- [ ] Add parser tests named `vegalite_error_marks_accept_string_and_object_forms`, `vegalite_errorbar_raw_aggregates_by_position_color_and_detail`, `vegalite_errorband_raw_builds_ordered_series_ranges`, `vegalite_error_mark_accepts_both_preaggregated_forms`, `vegalite_error_mark_rejects_malformed_ranges`, `vegalite_error_extent_rejects_single_sample_groups`, `vegalite_error_mark_rejects_unsupported_encoding_in_both_modes`, `vegalite_errorband_rejects_duplicate_independent_positions`, and `vegalite_error_mark_rejects_one_dimensional_interpolation`.
- [ ] Assert raw grouping order, null/missing/non-finite rejection, lower/upper endpoint order, and signed center/error offsets. `xError` / `yError` is the nonnegative upper offset; `xError2` / `yError2` is the nonpositive lower offset; if the second offset is omitted, mirror the first.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_error_`; confirm parser tests fail before implementation.
- [ ] Implement raw grouping by independent position, color, and detail; seed CI from deterministic group-key serialization and preflight the aggregate 10,000,000 bootstrap-draw budget before sampling. Preserve first-seen category order and reject missing/null required values or non-finite summaries.
- [ ] Implement lower/upper and center/error input normalization. Reject mixed/raw-plus-preaggregated forms, multiple measure axes, datum/value range definitions, unknown extent/orient/style/channel keys in both strict modes, reversed endpoints, malformed signed offsets, duplicate pre-aggregated errorband independent positions, and interpolation/tension on one-dimensional errorbands.
- [ ] Extend `check_unknown_keys` and `parse_mark`; populate dimensions/title/axes/theme/legend defaults and resolved mark/part styles. Add minimal guard/model/layout dispatch for `ChartKind::ErrorMark` and the required model metadata so this parser change compiles without affecting existing chart kinds.
- [ ] Run `cargo check -p fulgur-chart`, `cargo test -p fulgur-chart --test frontend_vegalite vegalite_error_`, and `cargo test -p fulgur-chart --test inspect_model`; confirm raw/pre-aggregated normalization and strict/non-strict validation pass.
- [ ] Commit as `feat(vegalite): normalize error mark input`.

## Task 4: Errorbar axis frame, guard/model, and Scene geometry

**Files:**

- Create `crates/fulgur-chart/src/layout/error_mark.rs`.
- Modify `crates/fulgur-chart/src/layout/{mod.rs,common.rs,bar.rs,scatter.rs}` as needed for shared axis helpers.
- Modify `crates/fulgur-chart/src/{guard.rs,model.rs}`.
- Create and test `crates/fulgur-chart/tests/render_vegalite_error_mark.rs` and add focused module tests.

**Interfaces:**

- Add `pub(crate) fn compute_frame(spec: &ChartSpec, m: &TextMeasurer) -> ErrorMarkFrame` and `pub(crate) fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene`.
- `ErrorMarkFrame` carries the plot rectangle and x/y coordinate maps for category-index, numeric, temporal, and full-axis positions; `map_position(axis: ErrorAxis, position: ErrorPosition) -> Result<f64, String>` returns plot-space coordinates.
- Compute the measured-axis domain from every lower and upper range endpoint, then apply the matching `AxisSpec` scale, hard/suggested bounds, tick formatting, and clipping policy. Keep legend sizing and Scene background/frame behavior shared with existing layout helpers.

- [ ] Add layout tests named `errorbar_vertical_and_horizontal_ranges_map_to_axis_endpoints`, `errorbar_1d_uses_plot_center_on_the_other_axis`, `errorbar_ticks_are_optional_and_use_part_styles`, `error_mark_frame_maps_category_temporal_quantitative_positions`, `error_mark_degenerate_ranges_remain_renderable`, and `error_mark_log_values_fail_and_hard_bounds_clip`; assert endpoint coordinates, cap orientation, plot-center and full-axis placement, category/temporal/quantitative position mapping, finite degenerate output, clipping, and log errors. Add guard/model tests named `error_mark_guard_rejects_misaligned_and_nonfinite_ranges`, `error_mark_guard_bounds_generated_primitives`, and `error_mark_model_reports_chart_type_and_axes`.
- [ ] Run `cargo test -p fulgur-chart --test render_vegalite_error_mark errorbar_`, `cargo test -p fulgur-chart --test render_vegalite_error_mark error_mark_`, and `cargo test -p fulgur-chart --lib error_mark_`; confirm the frame, geometry, guard, and model tests fail before implementation.
- [ ] Implement `ErrorMarkFrame` by adapting common categorical/temporal, horizontal-bar categorical, and scatter numeric axis helpers. Map one-dimensional errorbars to the other plot axis center and errorbands to its full extent. Hard bounds, tick formats, titles, legends, and plot clipping follow the matching `AxisSpec`.
- [ ] Implement vertical/horizontal errorbar rules and optional endpoint caps as clipped Scene lines. Resolve part style over mark color/opacity; rule defaults on and ticks default off. Enforce range alignment, finite values, log positivity, and the generated-primitive limit in guard/layout.
- [ ] Repeat the three focused test commands above and run `cargo test -p fulgur-chart --test inspect_model`; confirm both orientations, 1D positioning, styles, axis types, clipping, guard, and model assertions pass.
- [ ] Commit as `feat(layout): add errorbar Scene geometry`.

## Task 5: Errorband geometry and interpolation

**Files:**

- Modify `crates/fulgur-chart/src/layout/error_mark.rs`.
- Test `crates/fulgur-chart/tests/render_vegalite_error_mark.rs` and layout unit tests.

**Interfaces:**

- Add `fn errorband_path(points: &[MappedErrorRange], interpolation: ErrorBandInterpolation, tension: f64) -> Option<String>`; `MappedErrorRange` contains mapped independent, lower, and upper pixel coordinates.
- Group paths by `series_index` and `detail`, sort by independent coordinate, and use `ErrorMarkData.style.band` / `borders` for fill and boundary primitives.

- [ ] Add tests named `errorband_vertical_horizontal_and_1d_geometry`, `errorband_single_point_group_emits_no_fill`, `errorband_styles_control_fill_and_boundaries`, `errorband_all_interpolations_emit_finite_paths`, and `errorband_groups_keep_detail_paths_separate`; assert fill/border counts, finite path coordinates, orientation, one-dimensional full-axis coverage, single-point behavior, detail separation, and style opacity.
- [ ] Run `cargo test -p fulgur-chart --test render_vegalite_error_mark errorband_`; confirm missing fill paths / interpolation behavior fail before implementation.
- [ ] Implement `errorband_path` for all 13 enum modes in the spec; for a 2D group with one independent coordinate, return `None` for fill geometry. Apply interpolation to both boundaries using the same tension and close the polygon without self-crossing at endpoints.
- [ ] Emit clipped band fill and optional upper/lower boundaries; preserve one primitive limit estimate per generated path and reject any non-finite mapped coordinate before creating Scene items.
- [ ] Run `cargo test -p fulgur-chart --test render_vegalite_error_mark errorband_` and `cargo test -p fulgur-chart --lib error_mark_`; confirm every interpolation and style case passes.
- [ ] Commit as `feat(layout): render errorband paths`.

## Task 6: Examples, WASM schema, and goldens

**Files:**

- Create the four errorbar/errorband examples under `examples/specs/`.
- Modify `crates/fulgur-chart/tests/golden_png.rs` and `tests/wasm_runtime.rs`.
- Regenerate `crates/bindings/wasm/src/vegalite-schema.json`.
- Create the four named PNG goldens under `crates/fulgur-chart/tests/golden/`.

- [ ] Add one raw and one pre-aggregated example for each mark; include one horizontal or temporal case among them, a discrete color series, and representative ticks/borders/interpolation while keeping each data set small.
- [ ] Add the four example names to `golden_png.rs::NAMES` and add a WASM runtime test that parses each example, renders SVG and PNG on native/WASM, and checks valid dimensions, deterministic output, expected path/line geometry, and absence of non-finite coordinates.
- [ ] Run `cargo run --manifest-path crates/bindings/wasm/Cargo.toml --example regenerate_schemas`; verify the embedded schema accepts both new mark forms and rejects unsupported style/channel keys through `cargo test --manifest-path crates/bindings/wasm/Cargo.toml`.
- [ ] Generate each fixture alone with `UPDATE_GOLDEN=<spec-name> cargo test -p fulgur-chart --test golden_png golden_png_matches`, once for each of `vegalite-errorbar-raw`, `vegalite-errorbar-preaggregated`, `vegalite-errorband-raw`, and `vegalite-errorband-preaggregated`; verify each command changes only its matching PNG.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite`, `cargo test -p fulgur-chart --test render_vegalite_error_mark`, `cargo test -p fulgur-chart --test golden_png`, `wasm-pack test --node crates/fulgur-chart --test wasm_runtime`, and `cargo test --manifest-path crates/bindings/wasm/Cargo.toml`; confirm native and WASM parse/render the same fixture coverage.
- [ ] Commit as `test(vegalite): add error mark examples and goldens`.

## Task 7: Full verification, pull request, and merge

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run `cargo test -p fulgur-chart`, `cargo clippy -p fulgur-chart --all-targets -- -D warnings`, and `cargo test --manifest-path crates/bindings/wasm/Cargo.toml`; resolve failures and rerun affected commands.
- [ ] Review the final diff against the spec, including the new public `ChartKind` source-compatibility impact; verify existing line, area, bar, and scatter fixtures have no changed output.
- [ ] Push `feat/fulgur-chart-8do-errorbar-errorband` and create a PR referencing `fulgur-chart-8do`, with the four examples, architecture summary, and verification results.
- [ ] Monitor required CI, address actionable review comments, and wait for all checks to pass.
- [ ] Merge the PR under the existing automatic-merge authorization, close `fulgur-chart-8do` with the PR reference, push Beads state, and remove the issue worktree and temporary bare checkout after confirming the merge.
