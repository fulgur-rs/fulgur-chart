# Vega-Lite Tick Mark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Vega-Lite `mark: "tick"` as one short Scene rectangle per inline data record for strip plots.

**Architecture:** Parse tick marks through a dedicated Vega-Lite frontend module into a dedicated IR variant. A shared native/WASM layout maps quantitative, temporal, and categorical coordinates to Scene rectangles; composition reuses the existing Vega-Lite leaf and domain handling. Typed schema and input guards make the supported subset explicit and bounded.

**Tech Stack:** Rust, serde_json, schemars, existing Vega-Lite IR/layout, Scene primitives, Cargo integration tests, WASM browser smoke tests.

**Spec:** `fulgur-chart-2h1` and the approved in-chat design for inline-data tick marks.

## Global Constraints

- Accept inline `data.values`; reject URL data, transforms, bin/aggregate, and unsupported channels with clear parse errors.
- Require at least one of x/y; support quantitative, temporal, nominal, and ordinal positions; center an omitted orthogonal position in the plot.
- Render every input record as an individual tick rectangle; share geometry between native and WASM.
- Support horizontal/vertical orientation, mark and encoding size/color/opacity, and `config.tick.bandSize` / `config.tick.thickness`.
- Default `config.tick.bandSize` to 0.75 of the discrete step and `config.tick.thickness` to 1.
- Keep strict typed schema behavior and enforce existing data-point, category, and primitive limits before expensive allocation.

## Review Focus

- One-position-channel input must center ticks on the missing orthogonal axis; pin with an x-only and y-only render test.
- `orient` must select the size axis consistently; pin both orientations with width/height assertions.
- Default thickness and config band size must handle single-category and multi-category data; pin default and configured dimensions.
- Field encodings must retain per-record values and reject missing, mixed-type, non-finite, or unsupported inputs; pin parser and guard tests.
- Layer and concat must keep tick categories and Scene bounds correct; pin a composition render regression.

---

### Task 1: Schema, parser, IR, and input guards

**Files:**
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Create: `crates/fulgur-chart/src/frontend/vegalite_tick.rs`
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/src/guard.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Produce `VegaTickData` in `ChartKind::VegaTick`, containing validated per-record coordinates, resolved styles, orientation, and category domains.
- Dispatch `mark: "tick"` to `frontend::vegalite_tick::parse_tick_spec` before generic mark parsing, including composition leaves.

- [x] **Step 1: Add failing parser and schema tests**

Add tests for string/object tick mark forms, inline-data record retention, `x`-only and `y`-only position encodings, supported channel styles, `config.tick` keys, and rejection of URL data, transform, unsupported types, and unknown strict keys.

- [x] **Step 2: Run the tests to verify expected failures**

Run: `cargo test -p fulgur-chart --test frontend_vegalite tick_ --locked --offline`
Expected: typed schema rejects `tick` or the parser reports `未対応の mark: tick` before this feature is implemented.

- [x] **Step 3: Add typed schema and VegaTick IR**

Add string/object mark types for tick, add tick to unit and composition mark unions, define typed tick config under `VlConfig`, and add the IR structs and `ChartKind` variant.

- [x] **Step 4: Implement bounded tick parsing**

In `frontend/vegalite_tick.rs`, validate supported field types and inline records, preserve deterministic category order and per-record values, resolve constant/field styles and config defaults, reject unsupported properties, and check allocation limits before building segment vectors.

- [x] **Step 5: Add guard accounting and run parser tests**

Include tick records and emitted rectangles in point/primitive accounting. Run the focused parser/schema tests and `cargo test -p fulgur-chart --test frontend_vegalite --locked --offline`.

### Task 2: Shared layout and composition

**Files:**
- Create: `crates/fulgur-chart/src/layout/vega_tick.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Modify: `crates/fulgur-chart/src/layout/vega_composition.rs`
- Modify: `crates/fulgur-chart/src/frontend/vegalite_composition.rs`
- Modify: `crates/fulgur-chart/tests/render_vegalite_tick.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_composition.rs`

**Interfaces:**
- Consume `ChartKind::VegaTick(VegaTickData)` from Task 1.
- Produce plot bounds and `Prim::Rect` ticks through the shared Scene builder for native and WASM.

- [x] **Step 1: Add failing layout tests**

Cover horizontal quantitative-x/nominal-y ticks, vertical nominal-x/quantitative-y ticks, x-only and y-only centered placement, configured thickness/band size, and field-driven size/color/opacity. Assert the number, dimensions, and positions of rendered rectangles.

- [x] **Step 2: Run the tests to verify expected failures**

Run: `cargo test -p fulgur-chart --test render_vegalite_tick --locked --offline`
Expected: the feature-missing test fails at parsing or layout, with no compilation-only failure.

- [x] **Step 3: Implement tick coordinate scales and rectangle layout**

Map numeric/temporal positions through the continuous scale and categorical positions through first-seen category centers. Use configured thickness across the mark and band size along its orientation; center missing orthogonal positions. Clip via the existing Vega-Lite layer/plot path.

- [x] **Step 4: Integrate composition domains and leaf validation**

Register tick as an allowed composition leaf, return its position domains/category labels from composition parsing, include its mark geometry in primitive detection and layer sizing, and dispatch the dedicated layout for unit and composed charts.

- [x] **Step 5: Run focused render and composition tests**

Run: `cargo test -p fulgur-chart --test render_vegalite_tick --test render_vegalite_composition --locked --offline`.
Expected: both orientations, one-axis placement, styles, and composition pass.

### Task 3: Fixture, public docs, and WASM coverage

**Files:**
- Create: `examples/specs/vegalite-tick.json`
- Modify: `README.md`
- Modify: `crates/fulgur-chart/tests/golden_png.rs`
- Create: `crates/fulgur-chart/tests/golden/vegalite-tick.png`
- Modify: `crates/fulgur-chart/tests/wasm_runtime.rs`
- Modify: `crates/bindings/wasm/__test__/fixtures.mjs`
- Modify: `crates/bindings/wasm/__test__/browser-smoke.mjs`

- [x] **Step 1: Add an example exercising categorical and quantitative strip-plot positions**

Use inline values and an explicit orient/style configuration that demonstrates one tick per record.

- [x] **Step 2: Add native/WASM rendering regressions and golden fixture registration**

Verify deterministic SVG/PNG output through shared rendering and browser smoke. Add the fixture to the fixed golden test list and create its PNG golden with `UPDATE_GOLDEN=vegalite-tick cargo test -p fulgur-chart --test golden_png --locked --offline`.

- [x] **Step 3: Document the supported tick subset and example**

Add the mark and its supported channels, config defaults, and explicit unsupported inputs to the Vega-Lite README list.

- [x] **Step 4: Run feature and quality gates**

Run: `cargo fmt --all -- --check`
Run: `cargo test -p fulgur-chart --locked --offline`
Run: `cargo test -p fulgur-chart --test golden_png --locked --offline`
Run: `npm run build && npm run test:browser` from `crates/bindings/wasm`.
Expected: all existing and new tests pass; the new golden is stable on a second run.
