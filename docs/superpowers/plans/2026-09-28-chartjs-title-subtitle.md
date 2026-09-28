# Chart.js Title and Subtitle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement independent Chart.js title and subtitle configuration for every supported Chart.js chart kind while preserving Vega-Lite and native title output.

**Architecture:** Add typed schema and resolved title options to the Chart.js frontend and IR. Build all Chart.js charts through a shared title layout stage that reduces the chart viewport, composes the existing scene in a translated group, and emits title text in reserved boxes. Extend SVG, raster, validation, and normalized model geometry to honor the same group offset.

**Tech Stack:** Rust, serde, schemars, the existing SVG renderer, tiny-skia direct raster renderer, and the standalone WASM schema/runtime crate.

**Spec:** `docs/superpowers/specs/2026-09-28-chartjs-title-subtitle-design.md`

## Global Constraints

- Support every Chart.js chart kind currently accepted by the frontend.
- Vega-Lite and native title behavior must remain unchanged.
- A string is one line; an array is the explicit list of lines.
- Disabled or absent plugins reserve no space.
- Raster output uses the font bytes selected by the renderer; `family` is retained for SVG output and does not select an additional raster font.
- With fixed canvas sizing, title insets reduce the chart viewport; with plot-area sizing, the output scene grows by title insets.
- If title boxes consume all available space, the chart viewport is clamped to zero width or height.
- Rendered scene dimensions stay finite and within the existing 32,768 px dimension limit; title text is clipped to its reserved box, and plot-area expansion past that limit is rejected by the guard.
- Add no new dependencies.

## Review Focus

- Missing or disabled title plugins must preserve current chart scene output byte-for-byte; pin this in Task 4.
- Text line count distinguishes an empty string, empty array, and multiline text; line height is line count × resolved line height + top and bottom padding. Pin this in Tasks 2 and 4.
- Multiple boxes on the same side and different `fullSize` values must use deterministic alignment and stacking; pin this in Task 4.
- Invalid or extreme font line height and padding must not create non-finite or unbounded scene dimensions; pin resolved metrics in Task 2 and final layout in Task 4.
- Translated groups must preserve clips, gradients, circle batching, draw order, and child bounds checks; pin this in Task 3.

---

## Execution Prerequisite

Before implementation, confirm the feature branch is based on the latest available `origin/main`. At plan review, both refs are `c4a04fe` (which includes merged PRs #201–#203). The remote could not be queried because DNS/network access is unavailable, so retry fetch when available; preserve the untracked spec and plan files during any rebase. Do not rebase onto an older ref.

## Task 1: Type the shared title plugin schema

**Files:**
- Modify: `crates/fulgur-chart/src/schema/common.rs`
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
- Modify: `crates/bindings/wasm/src/chartjs-schema.json`
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`
- Test: `crates/bindings/wasm/src/lib.rs`

**Interfaces:**
- Add `TitleAlign::{Start, Center, End}` and `TitlePosition::{Top, Left, Bottom, Right}` in `schema/common.rs`.
- Reuse `ScalarOrArray<String>` for `TitlePlugin.text`, preserving the distinction between a scalar empty string and an empty array.
- Add untagged `TitlePadding::{Number(f64), Sides(TitlePaddingSides)}` with optional `top` and `bottom` fields.
- Extend `TitlePlugin` with `align`, `position`, `color`, `font: Option<FontSpec>`, `padding`, and camel-case `fullSize`.
- Add `subtitle: Option<TitlePlugin>` to every Chart.js `*Plugins` schema, including `SparklinePlugins`; add `title` there as well.

- [x] **Step 1: Write schema acceptance tests**

Add `chartjs_title_schema_accepts_all_chart_kinds` in `tests/frontend_chartjs.rs`. Deserialize a valid minimal fixture for every `ChartJsSpec` variant and alias with both plugins, all configuration fields, and both string and array text forms. Assert each round-trips through `serde_json::Value` with the same fields and that an unknown nested title key is rejected.

Add `embedded_chartjs_schema_includes_title_and_subtitle_for_all_kinds` in the WASM schema fixture tests. Assert the embedded schema equals `schema_for!(ChartJsSpec)` and that the generated schema contains `subtitle` under each chart-kind plugin object.

- [x] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --locked -p fulgur-chart --test frontend_chartjs chartjs_title_schema_accepts_all_chart_kinds`

Run: `cargo test --manifest-path crates/bindings/wasm/Cargo.toml --locked embedded_chartjs_schema_includes_title_and_subtitle_for_all_kinds`

Expected: compile or schema failures because the plugin types and fields are not present yet.

- [x] **Step 3: Add typed schema fields**

Implement the enums, text and padding types, and `TitlePlugin` fields in `schema/common.rs`. Add `title` and `subtitle` fields to every Chart.js plugin schema struct in `schema/chartjs.rs`. Keep `deny_unknown_fields` on title objects and the existing per-kind plugin structs.

- [x] **Step 4: Regenerate the embedded Chart.js schema and rerun focused tests**

Run: `cargo run --manifest-path crates/bindings/wasm/Cargo.toml --locked --example regenerate_schemas`

Run the two focused tests from Step 2.

Expected: both pass; only `chartjs-schema.json` changes, and the Vega-Lite schema remains byte-identical.

- [x] **Step 5: Commit Task 1**

```bash
git add crates/fulgur-chart/src/schema/common.rs crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs crates/bindings/wasm/src/chartjs-schema.json crates/bindings/wasm/src/lib.rs
git commit -m "feat(chartjs): type title and subtitle options"
```

## Task 2: Resolve title options into Chart.js IR

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Test: unit tests in `crates/fulgur-chart/src/frontend/chartjs.rs`
- Modify: ChartSpec construction sites under `crates/fulgur-chart/src` and `crates/fulgur-chart/tests`
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Interfaces:**
- Add `ChartJsTitle`, `ChartJsTitleAlign`, `ChartJsTitlePosition`, and `ChartJsTitlePadding` to `ir.rs`.
- Add `chartjs_title: Option<ChartJsTitle>` and `chartjs_subtitle: Option<ChartJsTitle>` to `ChartSpec`; retain `ChartSpec.title` unchanged.
- Add `resolve_chartjs_title(raw: Option<&RawTitle>, is_subtitle: bool, theme: &Theme) -> Result<Option<ChartJsTitle>, String>` in `frontend/chartjs.rs`.
- `ChartJsTitle` stores `display`, `text: Vec<String>`, typed alignment/position, resolved `Color`, font size/family/weight/style, resolved line height, top/bottom padding, and `full_size`.

- [x] **Step 1: Write failing IR mapping tests**

Add these tests in `tests/frontend_chartjs.rs`:

```rust
chartjs_title_and_subtitle_map_independently
chartjs_title_plugin_defaults_match_title_and_subtitle_defaults
chartjs_title_text_preserves_empty_string_and_empty_array
chartjs_title_line_height_resolves_number_px_em_percent_and_normal
chartjs_title_rejects_invalid_alignment_position_and_padding
chartjs_title_invalid_line_height_falls_back_to_default_multiplier
chartjs_title_extreme_line_height_and_padding_resolve_to_bounded_metrics
chartjs_title_strict_allowlists_accept_both_plugins_for_all_kinds
```

Add the non-finite line-height unit case inside `frontend/chartjs.rs`, where the resolver can be called with an in-memory `RawTitle`. Assert explicit align/position/color/font/padding/fullSize values in both IR fields; title defaults are bold with padding 10, subtitle defaults are normal with padding 0, and both use centered/top/theme-color/theme-size defaults. Invalid, zero, and non-finite line heights fall back to exactly `1.2 × font size`; extreme finite line heights and padding resolve to finite metrics no greater than `DEFAULT_MAX_DIMENSION_PX`. Task 4 verifies the resulting scene dimensions.

- [x] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --locked -p fulgur-chart --test frontend_chartjs chartjs_title_`

Expected: compilation or assertion failures because `ChartSpec` and the Chart.js mapper do not yet carry resolved title settings.

- [x] **Step 3: Add resolved title IR and update ChartSpec construction**

Implement the four IR types and two independent optional fields. Update every `ChartSpec` literal in the crate and its integration tests with `None` defaults unless a test intentionally sets title options. Keep the legacy `title` field and its existing frontend mappings unchanged.

- [x] **Step 4: Parse title and subtitle in every Chart.js parser path**

Extend `RawPlugins`/`RawTitle`, share `resolve_chartjs_title` between the common parser and special gauge/progress/outlabeled paths, and update strict allowlists so both keys are recognized for every supported type. Stop mapping Chart.js plugin text into `ChartSpec.title`; update the existing `title_from_plugins` assertion to check `ChartSpec.chartjs_title`. Preserve ordered string-array text and resolve defaults as defined in the spec. The strict-mode acceptance test must include every chart kind and alias.

- [x] **Step 5: Rerun parser tests and commit Task 2**

Run: `cargo test --locked -p fulgur-chart --test frontend_chartjs chartjs_title_`

Expected: all new title mapping tests pass, including special parser paths and strict-mode key checks.

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/src crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(chartjs): map independent title and subtitle IR"
```

## Task 3: Add translated scene groups to both renderers

**Files:**
- Modify: `crates/fulgur-chart/src/scene.rs`
- Modify: `crates/fulgur-chart/src/svg.rs`
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
- Modify: `crates/fulgur-chart/src/guard.rs`
- Test: unit tests in `scene.rs`, `svg.rs`, and `raster_direct.rs`

**Interfaces:**
- Add `Prim::Group { translate_x: f64, translate_y: f64, clip: Option<Box<ClipRect>>, children: Vec<Prim> }`. When present, the rectangular clip is in group-local coordinates; box it so the enum size stays bounded.
- Add recursive scene traversal helpers that carry inherited translation and visit children in input order.
- SVG group output uses `<g transform="translate(x y)" clip-path="url(#clipN)">` when a clip is present; raster traversal composes translation with device scale and applies the same clip rectangle.

- [x] **Step 1: Write failing group traversal and output tests**

Add `translated_group_moves_each_primitive_kind_in_svg`, `translated_group_moves_clipped_and_gradient_paths_in_svg`, `translated_group_moves_text_and_shapes_in_png`, `translated_group_clips_children_in_svg_and_png`, and `translated_group_preserves_circle_stamp_output`.

Each test uses a group translated by `(13, 7)` containing representative rect, line, circle, text, path, clipped path, styled path, and gradient path primitives. Assert SVG transforms and clip/gradient definitions are present, and compare decoded PNG pixels against an equivalent flat scene whose coordinates are offset by `(13, 7)`. The clipped-group case must prove that child text and paths cannot paint outside the local clip. Add raster tests proving translated circle and clipped-path bounds include inherited offsets and reject coordinates beyond `MAX_SAFE_DEVICE_CIRCLE_COORD_PX`.

- [x] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --locked -p fulgur-chart --lib translated_group_`

Expected: compile failures for the new primitive or renderer behavior.

- [x] **Step 3: Implement recursive SVG traversal**

Update definition collection and `write_prim` in `svg.rs` to recurse in depth-first order, emit groups, and keep gradient and clip identifiers deterministic across nested items.

- [x] **Step 4: Implement translation-aware raster traversal and validation**

Refactor the item loop into a recursive renderer that passes accumulated offsets and group clips to `render_prim`. Preserve uniform-circle stamping by translating stamp centers before device scaling. Include inherited translation in clipped-path and circle device-bound checks and clip-mask cache keys. Keep draw order identical to depth-first child order.

- [x] **Step 5: Rerun group tests and commit Task 3**

Run: `cargo test --locked -p fulgur-chart --lib translated_group_`

Expected: every group test passes and PNG pixels match the flat translated reference.

```bash
git add crates/fulgur-chart/src/scene.rs crates/fulgur-chart/src/svg.rs crates/fulgur-chart/src/raster_direct.rs crates/fulgur-chart/src/guard.rs
git commit -m "feat(scene): support translated primitive groups"
```

## Task 4: Reserve title margins in the shared scene and model layouts

**Files:**
- Create: `crates/fulgur-chart/src/layout/chartjs_title.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Modify: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/src/guard.rs`
- Test: unit tests in `layout/chartjs_title.rs`
- Test: unit tests in `crates/fulgur-chart/src/guard.rs`
- Test: `crates/fulgur-chart/tests/render_chartjs_titles.rs`

**Interfaces:**
- Add `ChartJsTitleLayout { left: f64, top: f64, right: f64, bottom: f64, viewport_width: f64, viewport_height: f64, scene_width: f64, scene_height: f64, text_items: Vec<Prim> }` in `layout/chartjs_title.rs`.
- Add `pub(crate) fn chartjs_title_layout(spec: &ChartSpec, base_scene_width: f64, base_scene_height: f64) -> Option<ChartJsTitleLayout>`. For `SizeMode::Canvas`, outer dimensions stay equal to the requested canvas and the viewport subtracts title insets. For `SizeMode::PlotArea`, viewport dimensions stay equal to the base chart scene and outer dimensions add the title insets.
- Add `pub(crate) fn chart_view_spec(spec: &ChartSpec, layout: &ChartJsTitleLayout) -> ChartSpec` to clone a Chart.js spec with the reduced child viewport dimensions for `SizeMode::Canvas`; preserve `width` and `height` for `SizeMode::PlotArea`, and clear the Chart.js title fields on the child spec.
- Refactor the kind match into `build_chart_scene(spec, measurer)` and keep `build_scene(spec, measurer)` as the shared title/background wrapper. Canvas mode builds the child scene from the reduced spec; plot-area mode first builds the base child scene at the requested plot size, then wraps that scene without changing its viewport.

- [x] **Step 1: Write failing shared layout tests**

Add these tests in `layout/chartjs_title.rs`:

```rust
chartjs_title_layout_reserves_each_side_and_stacks_same_side_boxes
chartjs_title_layout_uses_full_canvas_or_viewport_alignment_bounds
chartjs_title_layout_positions_rotated_text_on_all_four_sides
chartjs_title_layout_clamps_viewport_when_boxes_exhaust_canvas
chartjs_title_layout_preserves_plot_area_size_by_expanding_scene
chartjs_title_layout_distinguishes_empty_string_from_empty_array
chartjs_title_layout_multiline_thickness_is_line_count_times_line_height_plus_padding
chartjs_title_layout_extreme_metrics_keep_layout_values_finite
```

Use title font size 14, line height 17.5, title padding `{top: 8, bottom: 4}`, subtitle font size 10, line height 12, and subtitle padding 0. Assert top thicknesses of 29.5 and 12, deterministic side placement, and nonnegative viewport dimensions. Assert a displayed empty string occupies one line plus padding, an empty array occupies padding only, and a three-line title occupies exactly `3 × line_height + top_padding + bottom_padding`. With extreme but finite line height and padding inputs, assert all computed insets and outer dimensions remain finite; the guard test verifies oversized plot-area output is rejected.

Add `chartjs_title_scene_preserves_original_scene_when_plugins_are_hidden` in `tests/render_chartjs_titles.rs`; compare the built scene and SVG output with the same spec lacking plugin objects.

Add `legacy_chart_title_still_renders_without_chartjs_title_plugins` in `tests/render_chartjs_titles.rs` using a programmatically constructed `ChartSpec` with `title: Some(...)` and both Chart.js title fields unset; assert the legacy title output is unchanged.

- [x] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --locked -p fulgur-chart --lib chartjs_title_layout_`

Run: `cargo test --locked -p fulgur-chart --test render_chartjs_titles chartjs_title_scene_preserves_original_scene_when_plugins_are_hidden`

Expected: missing title-layout helper or assertion failures.

- [x] **Step 3: Implement shared title box layout**

Implement per-side thickness, title-before-subtitle stacking from canvas edge toward the chart viewport, horizontal and rotated vertical text placement, `align`, `fullSize`, and styled text lines. Use a clipped `Prim::Group` for each title box so long lines remain inside their reserved box. `display: false` contributes neither insets nor text primitives. Use saturating finite arithmetic for box thicknesses and viewport insets; fixed-canvas dimensions remain unchanged and plot-area dimensions are checked against the existing limit in the guard path.

- [x] **Step 4: Wrap every chart kind through `build_scene`**

For visible plugin boxes, `SizeMode::Canvas` builds the chart-kind scene from `chart_view_spec`; `SizeMode::PlotArea` reuses the base scene built at the requested plot size. Compose the chart scene as one translated `Prim::Group`, append title/subtitle text groups, and return the outer scene dimensions. Preserve the current no-title dispatch path and insert the theme background at the outermost scene level.

- [x] **Step 5: Apply the same viewport to normalized model geometry**

Update `model.rs` geometry construction to use the same `ChartJsTitleLayout` and child `ChartSpec`; add the outer `(left, top)` offset before normalizing plot-area coordinates against the final scene dimensions.

Add `chartjs_title_model_geometry_matches_translated_viewport` in `tests/inspect_model.rs`, asserting that title insets move and resize the normalized plot area consistently for bar and scatter models.

Add `chartjs_title_expansion_over_dimension_limit_is_rejected` in `guard.rs` tests. For `SizeMode::PlotArea`, a title-expanded output beyond `DEFAULT_MAX_DIMENSION_PX` must return the existing scene-dimension validation error before rendering.

- [x] **Step 6: Verify shared layout and commit Task 4**

Run: `cargo test --locked -p fulgur-chart --lib chartjs_title_layout_`

Run: `cargo test --locked -p fulgur-chart --test render_chartjs_titles`

Run: `cargo test --locked -p fulgur-chart --test inspect_model chartjs_title_model_geometry_matches_translated_viewport`

Run: `cargo test --locked -p fulgur-chart --lib chartjs_title_expansion_over_dimension_limit_is_rejected`

Expected: all title layout, hidden-plugin compatibility, and model geometry tests pass.

```bash
git add crates/fulgur-chart/src/layout/chartjs_title.rs crates/fulgur-chart/src/layout/mod.rs crates/fulgur-chart/src/model.rs crates/fulgur-chart/tests/render_chartjs_titles.rs crates/fulgur-chart/tests/inspect_model.rs
git commit -m "feat(chartjs): reserve shared title and subtitle margins"
```

## Task 5: Verify every chart kind and native/WASM output

**Files:**
- Modify: `crates/fulgur-chart/tests/render_chartjs_titles.rs`
- Modify: `crates/fulgur-chart/tests/frontend_chartjs.rs`
- Modify: `crates/fulgur-chart/tests/wasm_runtime.rs`
- Create: `examples/specs/chartjs_title_subtitle.json`
- Verify: `crates/bindings/wasm/src/chartjs-schema.json`

**Interfaces:**
- Use the `ChartJsTitleLayout` and `Prim::Group` interfaces from Tasks 3 and 4.
- Use the existing public SVG, PNG, and WASM render entry points; no binding API changes are introduced.

- [x] **Step 1: Add cross-kind rendering tests**

Add `chartjs_title_and_subtitle_render_for_every_chart_kind` in `render_chartjs_titles.rs`. Parse one valid spec for every supported `ChartJsSpec` variant and alias with both plugins enabled; assert final scene dimensions, child group offset, text primitive content, and unchanged item order inside the child group.

Add `chartjs_title_options_render_in_svg_and_png` using a Cartesian bar chart and a non-Cartesian pie chart. Assert SVG font/color/rotation attributes and decode PNG to assert title-color pixels occupy the reserved top and side bands.

Add `chartjs_title_subtitle_render_through_wasm` in `wasm_runtime.rs`. Render the same two-line title plus subtitle through the WASM API and assert successful SVG and PNG output.

- [x] **Step 2: Run cross-kind and renderer tests**

Run: `cargo test --locked -p fulgur-chart --test render_chartjs_titles`

Run: `cargo test --locked -p fulgur-chart --test frontend_chartjs chartjs_title_`

Run: `wasm-pack test --node crates/fulgur-chart --test wasm_runtime`

Expected: every accepted chart kind renders its title and subtitle, and both native and WASM outputs retain the configured styling.

- [x] **Step 3: Add an example and confirm non-Chart.js regressions stay fixed**

Create `examples/specs/chartjs_title_subtitle.json` with a bar chart, a two-line title, and a subtitle using different alignment, color, and font settings. Run the existing Vega-Lite/native title snapshot and golden tests without updating their expected files.

Run: `cargo test --locked -p fulgur-chart --test render_vegalite_temporal_line`

Run: `cargo test --locked -p fulgur-chart --test render_vega_rect`

Run: `cargo test --locked -p fulgur-chart --test golden_png`

Expected: Vega-Lite and native legacy-title snapshots remain unchanged. Update Chart.js title snapshots where the new Chart.js defaults or placement intentionally change output; do not alter unrelated snapshots or PNG goldens.

- [ ] **Step 4: Run complete quality gates**

Run: `cargo fmt --all -- --check`

Run: `cargo test --workspace --locked`

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`

Run: `cargo check -p fulgur-chart --target wasm32-unknown-unknown --locked`

Run: `cargo test --manifest-path crates/bindings/wasm/Cargo.toml --locked`

Expected: every command succeeds; embedded Chart.js schema fixture matches the Rust schema.

- [x] **Step 5: Commit final compatibility coverage**

```bash
git add crates/fulgur-chart/tests/frontend_chartjs.rs crates/fulgur-chart/tests/render_chartjs_titles.rs crates/fulgur-chart/tests/wasm_runtime.rs examples/specs/chartjs_title_subtitle.json crates/bindings/wasm/src/chartjs-schema.json
git commit -m "test(chartjs): cover title and subtitle rendering"
```

After the tasks, inspect the whole branch diff, run the configured PR review, push the branch, open a PR for `fulgur-chart-6qq`, and wait for all CI checks to pass before merging.
