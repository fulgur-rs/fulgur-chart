# Vega-Lite Text Mark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Render Vega-Lite text marks at quantitative x/y coordinates as standalone charts and Cartesian layer leaves.

**Architecture:** Parse the approved text-mark subset into dedicated `VegaText` IR, then map its coordinates through the existing scatter axis layout and emit shared `StyledText` Scene primitives. Extend the existing Vega-Lite schema, composition scale resolution, and input guards so unit and layer behavior stays consistent across native and WASM.

**Tech Stack:** Rust, serde/schemars, existing Vega-Lite frontend and composition parser, Scene/SVG/raster renderers, Cargo integration tests.

**Spec:** `docs/superpowers/specs/2026-09-30-fulgur-chart-pvl-vegalite-text.md`

## Global Constraints

- Accept inline `data.values` and finite quantitative x/y only.
- Support text field/value or literal `mark.text`; reject ambiguous multiple sources.
- Support nominal/ordinal color fields, quantitative size/opacity fields, and the documented basic text properties.
- Keep native and WASM behavior identical and reject unsupported inputs explicitly.
- Apply the existing row, label-byte, and primitive limits before allocating scene primitives.

## Review Focus

- Explicit non-quantitative x/y types must fail rather than be coerced; test categorical, temporal, and missing required fields in Task 1.
- Missing/null/non-finite values in text or quantitative style fields must fail with a field-qualified parser error; test in Task 1.
- Multiple text sources and unsupported format/condition/template/multiline/truncate inputs must fail in strict and non-strict parsing; test in Task 1.
- Layered text and point marks must share the resolved Cartesian frame and compatible scale domains; test in Task 3.
- Large row counts and text byte lengths must be rejected before Scene allocation in standalone and layer charts; test in Task 4.

---

### Task 1: Vega-Lite schema, IR, and parser

**Files:**
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`
- Modify: `crates/fulgur-chart/src/ir.rs`
- Create: `crates/fulgur-chart/src/frontend/vegalite_text.rs`
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite_composition.rs`

- [x] Add failing tests `vegalite_text_parses_field_value_and_literal_sources`, `vegalite_text_rejects_multiple_text_sources`, `vegalite_text_rejects_unsupported_inputs`, and `vegalite_composition_schema_accepts_text_mark_layer_leaf` for field/value/mark text, quantitative x/y, schema shape, and strict plus non-strict rejection.
- [x] Run `cargo test -p fulgur-chart --test frontend_vegalite vegalite_text --locked --offline` and confirm the new cases fail because the mark is unsupported or lacks text-specific validation.
- [x] Add `VegaText` IR and schema variants for unit and composition marks; implement a focused frontend parser that resolves row labels and per-point text styles into that IR.
- [x] Route standalone and inherited layer leaves through the text parser; validate channel types, required fields, scalar values, constant/value ranges, and unsupported properties independent of strict mode.
- [x] Run the focused frontend and schema round-trip tests; confirm supported inputs parse and unsupported inputs return explicit errors.
- [x] Commit the parser, IR, schema, and frontend regression tests as `feat(vegalite): parse text mark labels`.

### Task 2: Quantitative axes and text Scene layout

**Files:**
- Create: `crates/fulgur-chart/src/layout/vega_text.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Modify: `crates/fulgur-chart/src/layout/scatter.rs`
- Modify: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/src/scene.rs`
- Modify: `crates/fulgur-chart/src/svg.rs`
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_text.rs`

- [x] Add failing tests `vegalite_text_scene_maps_coordinates_and_styles_labels` and `vegalite_text_scene_applies_baseline_and_offsets` for shared linear coordinates, row count, styles, baseline, and offsets.
- [x] Run `cargo test -p fulgur-chart --test render_vegalite_text vegalite_text_scene --locked --offline` and confirm the new mark has no layout/Scene implementation.
- [x] Extend quantitative domain/model handling for `VegaText`; reuse scatter frame/tick placement and emit `StyledText` with color, font, alignment, baseline, angle, offsets, and resolved size/opacity. Add baseline positioning to `StyledText`, SVG `dominant-baseline`, and raster font-metric placement.
- [x] Assert out-of-bounds behavior, per-row field styles, native baseline placement, and SVG attributes; rerun `cargo test -p fulgur-chart --test render_vegalite_text --locked --offline`.
- [x] Commit the layout, Scene, renderer, and native regression tests as `feat(vegalite): render coordinate text marks`.

### Task 3: Layer scale and frame integration

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite_composition.rs`
- Modify: `crates/fulgur-chart/src/layout/vega_composition.rs`
- Modify: `crates/fulgur-chart/src/model.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_composition.rs`

- [x] Add failing tests `vegalite_layer_accepts_text_leaf_with_inherited_data` and `vegalite_layer_shares_text_color_size_scales_and_plot_frame` for inherited fields, shared domains, and position alignment with a point mark.
- [x] Run `cargo test -p fulgur-chart --test frontend_vegalite_composition vegalite_layer --locked --offline` and `cargo test -p fulgur-chart --test render_vegalite_composition vegalite_layer --locked --offline`; confirm text leaves fail layer eligibility or do not share the scale/frame.
- [x] Include text marks in layer leaf validation, raw and parsed scale domains, color/size overrides, axis domains, plot-rectangle lookup, and mark primitive ownership.
- [x] Run the focused layer tests and existing Vega-Lite composition suite; verify shared-frame alignment and stable child order.
- [x] Commit the composition scale/layout integration and regression tests as `feat(vegalite): support text marks in layers`.

### Task 4: Guards, example, golden, and cross-runtime coverage

**Files:**
- Modify: `crates/fulgur-chart/src/guard.rs`
- Create: `examples/specs/vegalite_text.json`
- Modify: `crates/fulgur-chart/tests/golden_png.rs`
- Modify: `crates/fulgur-chart/tests/wasm_runtime.rs`
- Modify: `README.md`
- Test: `crates/fulgur-chart/tests/render_vegalite_text.rs`

- [ ] Add failing test `vega_text_guard_enforces_point_label_and_primitive_limits` for row count, UTF-8 label byte size, and total primitive count in standalone and layer inputs.
- [ ] Run `cargo test -p fulgur-chart vega_text_guard --lib --locked --offline` and confirm the new IR currently bypasses text-specific limits.
- [ ] Count text rows/primitives and enforce the existing label byte cap before layout; add the documented example, fixed golden registration, README entry, and native/WASM integration cases.
- [ ] Add WASM tests `vegalite_text_example_renders_deterministic_svg_and_png` and `vegalite_text_rejections_match_on_native_and_wasm`; run focused parser/render/guard tests, `cargo test -p fulgur-chart --locked --offline`, `cargo clippy -p fulgur-chart --all-targets --locked --offline -- -D warnings`, `cargo fmt --all -- --check`, and `cargo check -p fulgur-chart --target wasm32-unknown-unknown --locked --offline`.
- [ ] Update the PNG golden and confirm the example output and native/WASM SVG and PNG are deterministic.
- [ ] Commit guards, example, docs, golden, and WASM coverage as `feat(vegalite): add text mark example and cross-runtime tests`.
