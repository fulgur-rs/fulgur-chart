# Chart.js Bar Border Radius Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Chart.js bar datasets render the per-dataset `borderRadius` number or per-corner object in SVG and PNG.

**Architecture:** Parse the public option into a small IR enum in `BarGeometryOptions`. Keep radius-only options out of the legacy bar geometry switch so they cannot change bar width or placement. Resolve the value-end corners from bar orientation, sign, and stack position, then use a shared helper in `layout/bar.rs` to emit either the existing `Prim::Rect` or an existing cubic `Prim::Path`. SVG and raster renderers already support `Prim::Path`, so no renderer primitive or output pipeline changes are needed.

**Tech Stack:** Rust, serde, schemars, existing SVG `Prim::Path`, existing tiny-skia path parser.

**Spec:** `docs/plans/2026-09-23-chartjs-bar-border-radius.md`

## Global Constraints

- `borderRadius` is per dataset and accepts a number or the four named corners.
- The default `borderSkipped: "start"` behavior leaves base-side corners square.
- Numeric radii on value-stacked bars apply only to the outer segment for each sign and stack group.
- Corner radii are finite, non-negative, and capped at half the smaller bar dimension.
- Unspecified or all-zero radii preserve the existing `Prim::Rect` output.
- A radius-only dataset option preserves the current bar width and placement calculations.
- Scriptable/indexable, hover, chart-wide defaults, and custom `borderSkipped` are out of scope.

## Review Focus

1. Horizontal negative values must round the left value-end corners and keep the right base corners square; Task 5 pins this behavior.
2. A number on positive and negative stacks must round only each stack's outer segment; Task 4 pins this behavior.
3. Missing corner fields must remain square, and unknown corner names must be rejected; Task 1 pins this behavior.
4. Zero, negative, and oversized values must not emit non-finite or out-of-bounds corner geometry; Task 3 pins this behavior.
5. A radius-only option must preserve bar width and placement, while no radius preserves rectangular primitives; Task 3 pins this behavior.

---

## File map

- `crates/fulgur-chart/src/schema/chartjs.rs`: public serde/JSON Schema option types on `BarDataset` and mixed `LineDataset`.
- `crates/fulgur-chart/src/frontend/chartjs.rs`: raw JSON field, strict-key allowlists, and conversion into IR.
- `crates/fulgur-chart/src/ir.rs`: frontend-independent `BarBorderRadius` stored in `BarGeometryOptions`; geometry-control detection must ignore radius.
- `crates/fulgur-chart/src/layout/bar.rs`: radius selection and cubic path helper; vertical and horizontal bar emit sites.
- `crates/fulgur-chart/src/layout/mixed.rs`: mixed-chart bar emit site.
- Unit tests stay beside the schema/parser/layout code that owns each contract. SVG and PNG are checked with their existing public render functions.

### Task 1: Public schema contract

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
- Test: `crates/fulgur-chart/src/schema/chartjs.rs`

**Interfaces:**
- Produces `BorderRadius::{Pixels(f64), Corners(BorderRadiusCorners)}` and `BorderRadiusCorners { top_left, top_right, bottom_left, bottom_right }`, with optional `f64` fields.
- Adds `Option<BorderRadius>` to `BarDataset` and `LineDataset` using the existing camelCase serde policy.

- [ ] **Step 1: Write the failing schema test**

Add this unit test to the existing schema tests:

```rust
#[test]
fn bar_datasets_accept_border_radius() {
    for json in [
        r#"{"data":[1],"borderRadius":6}"#,
        r#"{"data":[1],"borderRadius":{"topLeft":4,"bottomRight":2}}"#,
    ] {
        assert!(serde_json::from_str::<BarDataset>(json).is_ok(), "{json}");
    }
    assert!(serde_json::from_str::<BarDataset>(
        r#"{"data":[1],"borderRadius":{"topCentre":4}}"#
    )
    .is_err());
    assert!(serde_json::from_str::<LineDataset>(
        r#"{"type":"bar","data":[1],"borderRadius":6}"#
    )
    .is_ok());
}
```

- [ ] **Step 2: Verify the test fails for the missing fields**

Run: `cargo test -p fulgur-chart schema::chartjs::tests::bar_datasets_accept_border_radius --lib`
Expected: the valid inputs fail deserialization because `borderRadius` is not in the public dataset types.

- [ ] **Step 3: Add the schema types and dataset fields**

Define the untagged number/object enum and strict corner struct:

```rust
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
pub enum BorderRadius {
    Pixels(f64),
    Corners(BorderRadiusCorners),
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BorderRadiusCorners {
    pub top_left: Option<f64>,
    pub top_right: Option<f64>,
    pub bottom_left: Option<f64>,
    pub bottom_right: Option<f64>,
}
```

Add `#[serde(skip_serializing_if = "Option::is_none")] pub border_radius: Option<BorderRadius>` to `BarDataset` and `LineDataset`.

- [ ] **Step 4: Verify the schema contract passes**

Run: `cargo test -p fulgur-chart schema::chartjs::tests::bar_datasets_accept_border_radius --lib`
Expected: valid number/object forms pass and the unknown corner field fails.

### Task 2: IR and strict Chart.js parser

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Test: `crates/fulgur-chart/src/frontend/chartjs.rs`

**Interfaces:**
- Produces `BarBorderRadius::{Uniform(f64), Corners { top_left, top_right, bottom_left, bottom_right }}` in `ir.rs`.
- `BarGeometryOptions.border_radius` stores the option; `RawDataset` accepts the schema enum and maps it to the IR enum.

- [ ] **Step 1: Write the failing parser test**

Add a focused test that reaches both parser paths without referring to types that do not exist yet:

```rust
#[test]
fn bar_dataset_border_radius_parses_per_dataset_in_strict_mode() {
    let cases = [
        r#"{"type":"bar","data":{"datasets":[{"data":[1],"borderRadius":6}]}}"#,
        r#"{"type":"line","data":{"datasets":[{"type":"bar","data":[1],"borderRadius":{"topLeft":4}}]}}"#,
    ];
    for json in cases {
        let spec = parse(json, false).unwrap();
        assert!(spec.series[0].bar_geometry.is_some(), "{json}");
        assert!(parse(json, true).is_ok(), "strict parse rejected {json}");
    }
    assert!(parse(
        r#"{"type":"bar","data":{"datasets":[{"data":[1],"borderRaduis":6}]}}"#,
        true
    )
    .is_err());
}
```

- [ ] **Step 2: Verify the test fails**

Run: `cargo test -p fulgur-chart frontend::chartjs::tests::bar_dataset_border_radius_parses_per_dataset_in_strict_mode --lib`
Expected: the inputs either fail the public/raw contract or the resulting IR has no radius field.

- [ ] **Step 3: Add IR and parser mapping**

Add the IR enum and field:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarBorderRadius {
    Uniform(f64),
    Corners {
        top_left: Option<f64>,
        top_right: Option<f64>,
        bottom_left: Option<f64>,
        bottom_right: Option<f64>,
    },
}
```

Add `border_radius: Option<SchemaBorderRadius>` to `RawDataset`. Convert `Pixels(r)` to `BarBorderRadius::Uniform(r)` and `Corners(c)` to the corresponding corner fields. Include the option in the `bar_geometry` presence check and append `"borderRadius"` to both bar-root and line-root strict dataset-key allowlists.

- [ ] **Step 4: Verify parser behavior**

Run: `cargo test -p fulgur-chart frontend::chartjs::tests::bar_dataset_border_radius_parses_per_dataset_in_strict_mode --lib`
Expected: scalar/object values reach bar-series IR, including a bar dataset inside a line-root mixed chart; strict parsing rejects the misspelled key.

### Task 3: Rounded bar primitive helper

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/layout/bar.rs`
- Modify: `crates/fulgur-chart/src/layout/mixed.rs`
- Test: `crates/fulgur-chart/src/layout/bar.rs`

**Interfaces:**
- Produces `BarSide::{Top, Right, Bottom, Left}` and `bar_primitive(x, y, w, h, fill, radius, base_side, uniform_enabled) -> Prim`.
- `Uniform(r)` rounds corners opposite `base_side` only when `uniform_enabled` is true. `Corners` uses explicit values, while corners touching `base_side` stay square.

- [ ] **Step 1: Write failing scene and geometry tests**

Add `bar_primitive_rounds_only_value_end_corners`, `bar_primitive_clamps_radii_and_keeps_zero_square`, and `bar_primitive_radius_only_preserves_vertical_bar_geometry` tests through the existing bar scene builder so they compile before the new helper exists. Parse positive and negative vertical bars, a one-corner object, and zero/negative/oversized radii. Assert the dataset-colored primitive has the intended corner path or remains a rectangle for zero radii; the positive-radius cases fail before implementation. Compare `vertical_bar_boxes` for the same chart with and without only `borderRadius`; every `BarBox` field must match.

```rust
let spec = crate::frontend::chartjs::parse(
    r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[4],"borderRadius":3}]}}"#,
    false,
)
.unwrap();
let fill = spec.series[0].fill_at(0);
let measurer = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
let scene = super::build(&spec, &measurer);
assert!(scene.items.iter().any(|prim| matches!(
    prim,
    Prim::Path { fill: Some(path_fill), d, .. } if *path_fill == fill && d.contains(" C ")
)));
```

- [ ] **Step 2: Verify the scene tests fail**

Run: `cargo test -p fulgur-chart bar_primitive --lib`
Expected: both positive-radius assertions fail because bars are still rectangles.

- [ ] **Step 3: Implement radius resolution and path generation**

Implement `BarGeometryOptions::has_geometry_controls()` to return true only for category percentage, bar percentage, thickness, max thickness, or min length. Update the three `legacy_geometry` calculations in `layout/bar.rs` and `layout/mixed.rs` to use that predicate. Implement the shared helper with this interface:

```rust
fn bar_primitive(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: Color,
    radius: Option<BarBorderRadius>,
    base_side: BarSide,
    uniform_enabled: bool,
) -> Prim
```

Clamp each finite radius to `0..=min(w, h) / 2`; treat non-finite and negative values as zero. Build a clockwise `M/L/C/Z` path with cubic quarter-circle controls (`0.5522847498307936`) and `fmt_num`. Return `Prim::Rect` if all four resolved radii are zero. Wire the helper into the vertical non-stacked bar path while retaining the existing `BarBox` geometry and label placement.

- [ ] **Step 4: Verify helper geometry**

Run: `cargo test -p fulgur-chart bar_primitive --lib`
Expected: orientation, clamping, zero-radius fallback, corner bounds, and unchanged category geometry pass.

### Task 4: Vertical and mixed bars

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`
- Modify: `crates/fulgur-chart/src/layout/mixed.rs`
- Test: `crates/fulgur-chart/src/layout/bar.rs` and existing mixed-render test module as needed.

- [ ] **Step 1: Write failing layout tests**

Add a stacked fixture with two same-stack positive datasets and two same-stack negative datasets. Collect scene primitives by each dataset fill. Assert the later positive segment and later negative segment are paths, earlier same-sign segments are rectangles, and an equivalent fixture with no radius emits only rectangles. Add a mixed line-root fixture whose bar dataset has a radius and assert that bar fill uses a path. These scene assertions must fail before the wiring change.

```rust
let stack_json = r#"{
  "type":"bar",
  "data":{"labels":["A"],"datasets":[
    {"stack":"s","data":[2],"borderRadius":4},
    {"stack":"s","data":[3],"borderRadius":4},
    {"stack":"s","data":[-2],"borderRadius":4},
    {"stack":"s","data":[-3],"borderRadius":4}
  ]},
  "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}
}"#;
let spec = crate::frontend::chartjs::parse(stack_json, false).unwrap();
let measurer = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
let scene = super::build(&spec, &measurer);
let rounded = scene.items.iter().filter(|prim| matches!(prim,
    Prim::Path { fill: Some(fill), .. } if *fill == spec.series[1].fill_at(0)
)).count();
assert_eq!(rounded, 1);
```

- [ ] **Step 2: Verify the layout tests fail**

Run: `cargo test -p fulgur-chart vertical_bar_border_radius --lib`
Expected: bars are still emitted as `Prim::Rect` and the stack-end behavior is absent.

- [ ] **Step 3: Wire the helper into vertical and mixed bar emitters**

Use the y scale direction and each value sign to identify the base side in `build_vertical`. For value-stacked numeric radii, enable the uniform radius only when no later renderable same-stack segment has the same sign at that category:

```rust
let has_later_same_sign = spec.series.iter().enumerate().any(|(next, series)| {
    next > b.series
        && stack_groups[next] == stack_groups[b.series]
        && series.values.get(b.index).is_some_and(|value| {
            value.is_finite() && value.signum() == b.value.signum()
        })
});
let uniform_enabled = !value_stacked || !has_later_same_sign;
```

Use the computed `base` and `head` coordinates in `draw_bar_dataset` to find the mixed chart base side. Preserve `BarBox` geometry and data-label positions.

- [ ] **Step 4: Verify vertical and mixed layout behavior**

Run: `cargo test -p fulgur-chart vertical_bar_border_radius --lib`
Expected: positive/negative endpoints, stack edges, and unchanged no-radius geometry pass.

### Task 5: Horizontal bars and rendered output

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`
- Test: `crates/fulgur-chart/src/layout/bar.rs` and `crates/fulgur-chart/tests/frontend_chartjs.rs`

- [ ] **Step 1: Write failing horizontal/output tests**

Add an integration test in `tests/frontend_chartjs.rs` that parses an `indexAxis: "y"` chart with one positive and one negative value and a scalar radius. Assert the SVG includes a cubic path and that decoded PNG pixels show both value-end corners filled and both base-side corners unfilled. Use `render_chart(&spec)` for SVG and `raster_direct::render_chart_to_png(&spec, 1.0, fulgur_chart::font::DEFAULT_FONT)` for PNG; decode with the existing `image` test dependency. Run the test before horizontal emitters change and confirm the rounded-corner assertions fail.

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p fulgur-chart horizontal_bar_border_radius`
Expected: the bar scene uses square `Prim::Rect` values and the SVG lacks the rounded bar path.

- [ ] **Step 3: Wire the helper into horizontal bar emitters**

At each horizontal bar emit site, derive `base_side` from the actual `base` and `head` x coordinates and pass the per-dataset option and numeric enablement into `bar_primitive`:

```rust
let base_side = if base <= head { BarSide::Left } else { BarSide::Right };
items.push(bar_primitive(
    x, y, w, h, fill,
    ser.bar_geometry.and_then(|geometry| geometry.border_radius),
    base_side,
    uniform_enabled,
));
```

- [ ] **Step 4: Verify horizontal, SVG, and PNG output**

Run: `cargo test -p fulgur-chart horizontal_bar_border_radius`
Expected: positive/negative orientation and actual SVG/PNG corner pixels pass.

### Task 6: Full verification and review

**Files:**
- Review all changed files; no renderer implementation changes are expected.

- [ ] **Step 1: Format**

Run: `cargo fmt --all -- --check`
Expected: no formatting changes are required.

- [ ] **Step 2: Run the crate suite**

Run: `cargo test -p fulgur-chart`
Expected: all existing and new tests pass.

- [ ] **Step 3: Review patch and scope**

Run: `git diff --check` and `git status --short`; confirm radius-free SVG snapshots remain unchanged and only the approved schema/parser/IR/layout/docs files changed.

## Self-review

- Spec coverage: schema and strict parser are Task 1–2; value orientation and stack behavior are Task 3–5; both output formats and default compatibility are Task 3–5; bounds and invalid radii are Task 3.
- Review Focus coverage: each of the five listed inputs maps to a test in Tasks 1, 3, 4, or 5.
- Type consistency: schema `BorderRadius` maps to IR `BarBorderRadius`; `BarGeometryOptions.border_radius` is passed unchanged to the shared `bar_primitive` helper.
- Placeholder scan: all tasks give concrete files, behavior, and commands.
