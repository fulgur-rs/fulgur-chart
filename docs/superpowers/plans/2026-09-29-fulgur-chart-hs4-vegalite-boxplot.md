# Vega-Lite boxplot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Vega-Lite v6 の単体 `boxplot` mark を inline raw data から集計し、専用 IR と layout を通して native/WASM 共通で描画する。

**Architecture:** 専用 `frontend/vegalite_boxplot.rs` が入力を検証し、type-7 quantile、Tukey/min-max extent、group/style を `ChartKind::VegaBoxPlot` に正規化する。専用 `layout/vega_boxplot.rs` が統計値を Scene primitives にし、既存の axis、guard、native/WASM renderer に接続する。Chart.js の `ChartKind::BoxPlot` と `Series` の既存契約は変えない。

**Tech Stack:** Rust 2024、serde/serde_json、schemars、既存 `ChartSpec` / `Scene` renderer、Cargo tests、WASM bindings、PNG goldens。

**Spec:** [docs/superpowers/specs/2026-09-29-fulgur-chart-hs4-vegalite-boxplot.md](../specs/2026-09-29-fulgur-chart-hs4-vegalite-boxplot.md)

## Global Constraints

- `mark` は文字列 `"boxplot"` と `{ "type": "boxplot", ... }` を受理する。
- data source は `data.values` の inline record array とし、URL、欠落 data、空配列、record 以外の要素は parse error にする。
- 測定 channel は x/y の一方だけ quantitative とし、他方は省略または categorical とする。orient は測定軸から自動決定し、矛盾する明示値を拒否する。
- 位置カテゴリ、color、detail の group と category order は first-seen 順で決定的に保つ。
- quantile は線形補間 type-7 とし、extent は既定 1.5 の Tukey、有限な 0 以上の係数、または `"min-max"` とする。Tukey whisker は fence 内の実データ端点、外れ値は fence 外の raw values とする。
- `transform`、`layer`、pre-aggregated summary、URL data、および未対応 mark/channel/style/scale/legend extension は strict/non-strict 両 mode で明示エラーにする。
- mark/encoding/component style は spec の範囲と優先順位を守り、opacity は 0..1、size/strokeWidth/strokeDash は有限な 0 以上の値とする。
- data point、category/group、生成 primitive 数を既存 `InputLimits` で preflight し、上限超過を切り詰めない。
- axis domain は outlier を含む全 raw measurement values から決め、hard bounds と clip は既存 axis 規則に従う。
- native と WASM は同じ IR、guard、layout、Scene path を使い、既存 Chart.js `ChartKind::BoxPlot` は維持する。

## Review Focus

- 全 finite input でも extent 計算が overflow して non-finite summary を作る場合は拒否する。`vegalite_boxplot_rejects_nonfinite_statistical_results` で parser error として固定する (Task 3)。
- singleton または全値同一の group はゼロ幅/ゼロ IQR でも有限 geometry として描画する。`vegalite_boxplot_handles_singleton_and_constant_groups` で固定する (Task 3)。
- measurement/category/color/detail field の null・欠落は group を黙って欠落させず拒否する。`vegalite_boxplot_rejects_missing_or_null_measurement_and_group_fields` で両 parser mode を固定する (Task 3)。
- detail/color により group 数と outlier primitives が増える入力も各上限の直前・超過を正しく判定する。`vega_boxplot_guard_enforces_point_category_and_primitive_limits` で固定する (Task 3)。
- hard bounds が raw outlier の表示を切っても data domain と boxplot の mapping は全測定値を基準にする。`boxplot_axis_domain_includes_outliers_and_clips_to_hard_bounds` で固定する (Task 3)。

---

## File Structure

- `crates/fulgur-chart/src/schema/vegalite.rs`: typed boxplot mark, component style, encoding、root spec variant を追加する。
- `crates/fulgur-chart/src/ir.rs`: Vega-Lite 固有 orient、extent、summary、group、component style、chart data の型を追加する。Chart.js の `BoxPlot` と `Series` は変更しない。
- `crates/fulgur-chart/src/frontend/vegalite_boxplot.rs`: inline records の検証、type-7 quantile/Tukey/min-max summary、grouping、orient と style の解決を担当する。
- `crates/fulgur-chart/src/frontend/vegalite_error.rs`: 既存の type-7 quantile helper を sibling parser から使える可視性にする。
- `crates/fulgur-chart/src/frontend/vegalite.rs` と `frontend/mod.rs`: `boxplot` の早期 dispatch と parser module 登録を追加する。
- `crates/fulgur-chart/src/guard.rs`: boxplot data/category/group、finite summary、生成 primitive 上限を検証する。
- `crates/fulgur-chart/src/model.rs`: model の type、軸、カテゴリ、color groups を Vega-Lite boxplot IR から報告する。
- `crates/fulgur-chart/src/layout/vega_boxplot.rs` と `layout/mod.rs`: 専用 axis frame と box/median/whisker/tick/outlier の Scene geometry、chart dispatch を追加する。
- `crates/fulgur-chart/tests/frontend_vegalite.rs`: typed schema と strict/non-strict parser の回帰を追加する。
- `crates/fulgur-chart/tests/render_vegalite_boxplot.rs`: boxplot の render、Scene primitive、axis、clip、style の統合テストを追加する。
- `crates/fulgur-chart/tests/wasm_runtime.rs`: boxplot example の native/WASM parser/render parity を追加する。
- `crates/fulgur-chart/tests/golden_png.rs` と `tests/golden/vegalite-boxplot.png`: 固定 golden list と representative raster output を追加する。
- `examples/specs/vegalite-boxplot.json`: raw data・カテゴリ grouping・outlier を示す example を追加する。
- `crates/bindings/wasm/src/vegalite-schema.json`: core schema generator から更新する。

## Task 1: Typed schema contracts

**Files:**

- Modify `crates/fulgur-chart/src/ir.rs`.
- Modify `crates/fulgur-chart/src/schema/vegalite.rs`.
- Test `crates/fulgur-chart/tests/frontend_vegalite.rs`.

**Interfaces:**

- Add `VegaLiteSpec::BoxPlot(VlBoxPlotSpec)`.
- Add public IR types `VegaBoxPlotExtent::{Tukey { coefficient: f64 }, MinMax}` and `VegaBoxPlotSummary { pub q1: f64, pub median: f64, pub q3: f64, pub whisker_low: f64, pub whisker_high: f64, pub data_min: f64, pub data_max: f64, pub outliers: Vec<f64> }` in `ir.rs`; do not add the ChartKind variant until Task 3 so existing layout/model exhaustive matches keep compiling.
- Add `VlBoxPlotSpec { mark: MarkBoxPlot, data: VlData, encoding: VlBoxPlotEncoding, schema: Option<String>, width: Option<f64>, height: Option<f64>, title: Option<VlTitle>, background: Option<String>, config: Option<VlConfig> }` with the existing serde field renames and `deny_unknown_fields` convention.
- Add `MarkBoxPlot` as an untagged string/object wrapper; `MarkBoxPlotObject` admits only `type`, `extent`, `orient`, `size`, `color`, `opacity`, `clip`, `box`, `median`, `outliers`, `rule`, and `ticks`.
- Add typed mark extent/orient definitions; represent each component property as boolean or `VlBoxPlotPartStyle`. The part style object admits only `color`, `fill`, `stroke`, `strokeWidth`, `strokeDash`, `opacity`, and `size`.
- Add `VlBoxPlotEncoding` for x/y position, color, detail, size, and opacity. Position fields accept only quantitative/nominal/ordinal hints; categorical grouping channels reject quantitative hints; size accepts the quantitative/value forms in the spec; opacity is constrained to 0..1 when constant.

- [ ] Add `vegalite_boxplot_schema_accepts_string_and_object_mark` and assert each mark form deserializes successfully through the typed `VegaLiteSpec` root.
- [ ] Add `vegalite_boxplot_schema_accepts_supported_encoding_channels` and assert quantitative measurement plus categorical/detail/size/opacity definitions deserialize.
- [ ] Add `vegalite_boxplot_schema_rejects_unknown_mark_and_part_keys` and assert unsupported properties are rejected by typed schema deserialization.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_boxplot_schema`; confirm the tests fail because the schema variant/types are absent.
- [ ] Add the typed mark, part-style, encoding, and root spec definitions to `schema/vegalite.rs`; register `BoxPlot` in `VegaLiteSpec` and preserve the existing root fields.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_boxplot_schema`; confirm all three schema tests pass.
- [ ] Commit as `feat(vegalite): add boxplot schema`.

## Task 2: Boxplot statistics helper

**Files:**

- Modify `crates/fulgur-chart/src/frontend/mod.rs`.
- Create `crates/fulgur-chart/src/frontend/vegalite_boxplot.rs`.
- Test helper unit tests in `crates/fulgur-chart/src/frontend/vegalite_boxplot.rs`.

**Interfaces:**

- Add `pub(super) fn summarize_boxplot(values: &[f64], extent: VegaBoxPlotExtent) -> Result<VegaBoxPlotSummary, String>` in `vegalite_boxplot.rs`, using the public types from `ir.rs`. It rejects empty/non-finite input and any non-finite derived quantile/fence/endpoint.
- Keep the helper pure: it does not depend on ChartSpec, parser mode, or Scene layout. Change the current `type7_quantile(sorted: &[f64], probability: f64) -> f64` in `frontend/vegalite_error.rs` to `pub(super)` and call that same formula without changing error mark behavior.

- [ ] Add `boxplot_quantiles_use_type7_interpolation`; assert Q1/median/Q3 for `[1, 2, 3, 4, 5]` are `2`, `3`, and `4`.
- [ ] Add `boxplot_tukey_uses_observed_whiskers_and_keeps_outliers`; assert `[1, 2, 3, 4, 5, 100]` uses actual in-fence whiskers and retains `100` as an outlier.
- [ ] Add `boxplot_min_max_uses_data_extrema_without_outliers`; assert minimum/maximum endpoints and an empty outlier list.
- [ ] Add `boxplot_summary_handles_singleton_and_constant_samples`; assert finite identical quartiles/whiskers for singleton and all-equal inputs.
- [ ] Add `boxplot_summary_rejects_empty_nonfinite_and_overflowing_fences`; assert errors for empty/non-finite values and a coefficient/sample combination whose derived fence is not finite.
- [ ] Run `cargo test -p fulgur-chart boxplot_`; confirm helper tests fail before the summary implementation exists.
- [ ] Implement `summarize_boxplot` using type-7 quantiles, observed Tukey whisker endpoints, raw Tukey outliers, and min/max behavior from the spec. Sort a copy for quantiles while preserving raw outlier order.
- [ ] Run `cargo test -p fulgur-chart boxplot_`; confirm all statistics and invalid-result tests pass.
- [ ] Commit as `feat(vegalite): add boxplot statistics`.

## Task 3: Parser, dedicated IR, guard, model, and Scene layout

**Files:**

- Modify `crates/fulgur-chart/src/ir.rs`, `frontend/mod.rs`, `frontend/vegalite.rs`, `guard.rs`, `model.rs`, and `layout/mod.rs`.
- Create `crates/fulgur-chart/src/layout/vega_boxplot.rs`.
- Test `crates/fulgur-chart/tests/frontend_vegalite.rs`, `crates/fulgur-chart/tests/render_vegalite_boxplot.rs`, and relevant unit tests in the parser/layout/guard/model modules.

**Interfaces:**

- Add public `VegaBoxPlotOrient::{Horizontal, Vertical}` and `VegaBoxPlotGroup { pub category_index: Option<usize>, pub color_label: Option<String>, pub detail_label: Option<String>, pub color: Color, pub size: Option<f64>, pub opacity: f64, pub summary: VegaBoxPlotSummary }` in `ir.rs`. Vector order is the first-seen group order.
- Add public `VegaBoxPlotData { pub orient: VegaBoxPlotOrient, pub categories: Vec<String>, pub groups: Vec<VegaBoxPlotGroup>, pub has_category: bool, pub extent: VegaBoxPlotExtent, pub style: VegaBoxPlotStyle }`.
- Add public `VegaBoxPlotStyle { pub clip: bool, pub opacity: f64, pub box_part: VegaBoxPlotPartStyle, pub median_part: VegaBoxPlotPartStyle, pub outliers_part: VegaBoxPlotPartStyle, pub rule_part: VegaBoxPlotPartStyle, pub ticks_part: VegaBoxPlotPartStyle }` and `VegaBoxPlotPartStyle { pub visible: bool, pub fill: Option<Color>, pub stroke: Option<Color>, pub stroke_width: Option<f64>, pub stroke_dash: Vec<f64>, pub opacity: Option<f64>, pub size: Option<f64> }`.
- Add `ChartKind::VegaBoxPlot(Box<VegaBoxPlotData>)`; keep `ChartKind::BoxPlot` and `Series` unchanged. Add chart type/model metadata and exhaustive dispatch arms in the same task so the branch compiles at every commit.
- Add `pub(super) fn parse_boxplot_spec(top: &mut serde_json::Map<String, serde_json::Value>, limits: &InputLimits) -> Result<ChartSpec, String>` to `frontend/vegalite_boxplot.rs`; call it from `vegalite::parse_with_limits` before generic `parse_mark` handling.
- Add `pub(crate) fn build_checked(spec: &ChartSpec, m: &TextMeasurer, primitive_limit: usize) -> Result<Scene, String>` to `layout/vega_boxplot.rs`; dispatch `ChartKind::VegaBoxPlot` to it from `layout/mod.rs`.
- Guard the raw point count with `max_total_data_points`, category/group counts with existing category limits, and estimated output with `max_categorical_primitives` before group/Scene allocations.

- [ ] Add parser tests `vegalite_boxplot_infers_horizontal_and_vertical_orientation`, `vegalite_boxplot_rejects_conflicting_orient`, and `vegalite_boxplot_groups_category_color_and_detail_in_first_seen_order`; assert x-measurement -> horizontal, y-measurement -> vertical, conflicting orient rejection, and stable category/color/detail group order.
- [ ] Add `vegalite_boxplot_rejects_transform_layer_and_summary_in_both_modes`; assert each unsupported input errors in strict and non-strict modes.
- [ ] Add `vegalite_boxplot_rejects_missing_or_null_measurement_and_group_fields` and `vegalite_boxplot_rejects_nonfinite_statistical_results`; assert no malformed values silently drop or enter the IR.
- [ ] Add layout tests `boxplot_vertical_horizontal_and_1d_geometry`, `boxplot_groups_are_side_by_side_deterministically`, `boxplot_styles_apply_mark_encoding_and_part_precedence`, `vegalite_boxplot_handles_singleton_and_constant_groups`, and `boxplot_axis_domain_includes_outliers_and_clips_to_hard_bounds`; assert geometry positions, component visibility/style precedence, finite singleton/constant geometry, outlier domain, and clipping.
- [ ] Add `vega_boxplot_guard_enforces_point_category_and_primitive_limits` and model test `vega_boxplot_model_reports_type_axes_and_groups`; assert each configurable limit and metadata.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_boxplot_` and `cargo test -p fulgur-chart --test render_vegalite_boxplot`; confirm the new integration tests fail before parser/IR/layout support.
- [ ] Implement dedicated `VegaBoxPlotData` / group / style IR, then parse raw values, validate supported schema in both modes, preflight limits, infer/validate orientation, and group by position + color + detail in first-seen order. Return explicit errors for transform/layer/summary/URL and malformed fields.
- [ ] Implement `parse_boxplot_spec` to resolve `extent`, type-7 summaries, color/size/opacity values, component styles, clip, title, dimensions, axes, and palette. Derive axis domain from every input value including outliers.
- [ ] Implement the Vega-Lite model and guard branches, then `layout/vega_boxplot.rs` frame/mapping for numeric measurement axes and optional categorical position axes. Place 1D boxes at the orthogonal plot center; position category groups side-by-side deterministically; render box, median, whisker rule, endpoint ticks, and outlier points as Scene primitives.
- [ ] Implement plot clipping and hard-bound mapping with existing axis/Scene primitives; ensure the parser and layout reject any non-finite derived value rather than emitting non-finite SVG coordinates.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_boxplot_`, `cargo test -p fulgur-chart --test render_vegalite_boxplot`, and focused `cargo test -p fulgur-chart --lib vega_boxplot`; confirm grouping, statistics integration, component geometry/styles, limits, hard bounds, and model assertions pass.
- [ ] Commit as `feat(vegalite): parse and render boxplots`.

## Task 4: Example, WASM schema parity, golden, and full verification

**Files:**

- Create `examples/specs/vegalite-boxplot.json`.
- Modify `crates/fulgur-chart/tests/{golden_png.rs,wasm_runtime.rs}`.
- Generate `crates/bindings/wasm/src/vegalite-schema.json`.
- Create `crates/fulgur-chart/tests/golden/vegalite-boxplot.png`.

**Interfaces:**

- Register `vegalite-boxplot` in the fixed `golden_png.rs` `NAMES` list.
- Add the example to a boxplot fixture helper in `wasm_runtime.rs`; native/WASM checks use the same JSON and public Vega-Lite parser/render APIs.
- Regenerate the embedded schema using `cargo run --manifest-path crates/bindings/wasm/Cargo.toml --example regenerate_schemas`.

- [ ] Add `examples/specs/vegalite-boxplot.json` with categorical groups, a color grouping, finite Tukey outliers, mark styling, and the dimensions used by other Vega-Lite examples.
- [ ] Add the fixture to `golden_png.rs` and generate only its image with `UPDATE_GOLDEN=vegalite-boxplot cargo test -p fulgur-chart --test golden_png golden_png_matches`; verify no other golden changes.
- [ ] Add `vegalite_boxplot_example_renders_deterministic_svg_and_png` to `wasm_runtime.rs`; assert repeated SVG/PNG output is identical, output is valid, and SVG contains box, rule, and outlier geometry.
- [ ] Run `cargo run --manifest-path crates/bindings/wasm/Cargo.toml --example regenerate_schemas`; verify the regenerated Vega-Lite schema includes both mark forms and rejects unknown component-style keys through the schema tests.
- [ ] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_boxplot_`, `cargo test -p fulgur-chart --test render_vegalite_boxplot`, `cargo test -p fulgur-chart --test golden_png`, `cargo test --manifest-path crates/bindings/wasm/Cargo.toml`, and `wasm-pack test --node crates/fulgur-chart --test wasm_runtime`; confirm native and WASM exercise the same boxplot fixture.
- [ ] Run repository CI quality gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`; resolve failures before opening the PR.
- [ ] Commit as `test(vegalite): cover boxplot native and wasm rendering`.

## Merge and issue handoff

After the implementation commits and quality gates pass, push the feature branch, create a PR for `fulgur-chart-hs4`, and monitor required CI to completion. The user authorized merging after CI passes; merge the PR once all required checks are green, close the Beads issue, push Beads data, and remove only this issue's worktree after the merge. Finish with the repository session close steps in `AGENTS.md`.
