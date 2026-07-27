# Chart.js Tick Styling Fields Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or `superpowers:executing-plans`
> to execute this plan task by task. Progress is tracked in Bead
> `fulgur-chart-kjx`; numbered steps below are execution instructions, not a
> second task tracker.

**Goal:** Propagate Chart.js 4.5.1 `grid.tickColor`, `tickWidth`, and
`tickLength` through typed schema, IR, and every existing Cartesian tick
rendering path.

**Architecture:** Keep `AxisGrid` as the shared dialect-neutral boundary.
Chart.js parsing supplies typed tick overrides and Chart.js defaults, while the
Vega-Lite temporal frontend explicitly preserves its existing opaque 4px tick
contract. Layout code consumes only resolved `AxisGrid` methods and never reads
frontend schema types.

**Tech Stack:** Rust 2024, serde, schemars, fulgur-chart IR/scene primitives,
Insta regression tests, cargo test, cargo clippy, cargo llvm-cov, Beads

## Global Constraints

- Use Chart.js 4.5.1 as the tick-style oracle.
- Preserve fulgur's intentional `drawTicks=false` default.
- `tickColor` inherits grid color; `tickWidth` inherits grid line width.
- Chart.js `tickLength` defaults to `8.0`.
- Static scalar/array inputs are accepted for `tickColor` and `tickWidth`; the
  current subset uses only the first array element.
- Preserve Vega-Lite temporal ticks as opaque theme-text-color lines with
  `tickLength=4.0`.
- Do not add tick primitives to axis positions that currently have none.
- Do not implement `tickBorderDash`, `tickBorderDashOffset`, or scriptable
  callbacks.
- Do not modify or commit the user's untracked
  `docs/plans/2026-07-14-chartjs-compat-gap.md`.
- Add each behavior test before its production change and observe the expected
  RED.
- Final executable changed-line coverage against `origin/main...HEAD` must be
  100%.

## File Map

- `crates/fulgur-chart/src/schema/common.rs`: typed Chart.js grid input fields
  and serde/schema tests.
- `crates/fulgur-chart/src/ir.rs`: shared tick-style fields, defaults, and
  inheritance helpers.
- `crates/fulgur-chart/src/frontend/chartjs.rs`: Chart.js schema-to-IR mapping.
- `crates/fulgur-chart/src/frontend/vegalite.rs`: explicit preservation of the
  Vega-Lite temporal tick contract.
- `crates/fulgur-chart/src/layout/common.rs`: categorical/linear y ticks and
  temporal x ticks.
- `crates/fulgur-chart/src/layout/scatter.rs`: numeric x/y ticks.
- `crates/fulgur-chart/src/layout/bar.rs`: horizontal-bar numeric x ticks.
- `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`: cross-frontend
  regression for opaque 4px temporal ticks.

---

### Task 1: Type the Chart.js tick-style schema

**Files:**

- Modify/test: `crates/fulgur-chart/src/schema/common.rs:139-173`
- Modify/test: `crates/fulgur-chart/src/schema/common.rs:265-321`

**Interfaces:**

- Consumes: existing `ScalarOrArray<T>` and `ColorString`
- Produces:
  `GridLineOptions.tick_color: Option<ScalarOrArray<ColorString>>`,
  `tick_width: Option<ScalarOrArray<f64>>`, and existing
  `tick_length: Option<f64>`

#### Step 1: Add failing schema behavior tests

Add these tests beside `grid_line_options_camel_case_field_names`:

```rust
#[test]
fn grid_line_options_rejects_wrong_tick_style_types() {
    for json in [
        r#"{"tickColor":42}"#,
        r#"{"tickWidth":"wide"}"#,
        r#"{"tickLength":"long"}"#,
    ] {
        assert!(
            serde_json::from_str::<GridLineOptions>(json).is_err(),
            "invalid tick style must be rejected: {json}"
        );
    }
}

#[test]
fn grid_line_options_roundtrips_static_tick_style_shapes() {
    let json = r##"{
        "tickColor":["#ff0000","#00ff00"],
        "tickWidth":[2.0,3.0],
        "tickLength":6.0
    }"##;
    let parsed: GridLineOptions = serde_json::from_str(json).unwrap();
    let value = serde_json::to_value(parsed).unwrap();

    assert_eq!(value["tickColor"], serde_json::json!(["#ff0000", "#00ff00"]));
    assert_eq!(value["tickWidth"], serde_json::json!([2.0, 3.0]));
    assert_eq!(value["tickLength"], serde_json::json!(6.0));
}
```

Run:

```bash
cargo test -p fulgur-chart schema::common::tests::grid_line_options_rejects_wrong_tick_style_types -- --exact
```

Expected RED: `tickColor: 42` and `tickWidth: "wide"` are currently accepted
because those fields are untyped `serde_json::Value`.

#### Step 2: Replace the untyped fields

Change the production fields and update their comments:

```rust
/// Chart.js `grid.tickColor`. Static arrays are accepted; rendering uses the
/// first element. When absent, the IR inherits `grid.color`.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub tick_color: Option<ScalarOrArray<ColorString>>,
/// Chart.js `grid.tickWidth`. Static arrays are accepted; rendering uses the
/// first element. When absent, the IR inherits `grid.lineWidth`.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub tick_width: Option<ScalarOrArray<f64>>,
```

Retain the existing typed field:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub tick_length: Option<f64>,
```

Update the nearby tests/comments that currently call these fields
"v1 receive-only" so they describe typed static input.

#### Step 3: Verify GREEN

Run:

```bash
cargo test -p fulgur-chart schema::common::tests::grid_line_options_rejects_wrong_tick_style_types -- --exact
cargo test -p fulgur-chart schema::common::tests::grid_line_options_roundtrips_static_tick_style_shapes -- --exact
cargo test -p fulgur-chart schema::common::tests
```

Expected: all schema tests pass.

#### Step 4: Commit

```bash
git add crates/fulgur-chart/src/schema/common.rs
git commit -m "feat(chartjs): type axis tick styling fields"
```

---

### Task 2: Extend AxisGrid and map both frontend contracts

**Files:**

- Modify/test: `crates/fulgur-chart/src/ir.rs:170-196`
- Modify/test: `crates/fulgur-chart/src/ir.rs:639-649`
- Modify/test: `crates/fulgur-chart/src/frontend/chartjs.rs:340-373`
- Modify/test: `crates/fulgur-chart/src/frontend/chartjs.rs:3459-3510`
- Modify/test: `crates/fulgur-chart/src/frontend/vegalite.rs:279-283`
- Modify/test: `crates/fulgur-chart/src/frontend/vegalite.rs:1251-1297`
- Modify/test:
  `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs:97-129`

**Interfaces:**

- Consumes: typed `GridLineOptions` from Task 1
- Produces:
  `AxisGrid::{tick_color, tick_width, tick_length}`,
  `AxisGrid::resolved_tick_color(Color) -> Color`, and
  `AxisGrid::resolved_tick_width() -> f64`

#### Step 1: Add failing IR and Chart.js mapping tests

Extend `axis_grid_default_matches_fulgur_backward_compat` and add a focused
inheritance test:

```rust
#[test]
fn axis_grid_default_matches_fulgur_backward_compat() {
    let g = AxisGrid::default();
    assert!(g.display);
    assert!((g.line_width - 1.0).abs() < 1e-9);
    assert!(!g.draw_ticks);
    assert!(g.color.is_none());
    assert!(g.tick_color.is_none());
    assert!(g.tick_width.is_none());
    assert!((g.tick_length - 8.0).abs() < 1e-9);
}

#[test]
fn axis_grid_tick_style_resolves_override_then_grid_fallback() {
    let grid = Color { r: 1, g: 2, b: 3, a: 1.0 };
    let theme = Color { r: 4, g: 5, b: 6, a: 1.0 };
    let tick = Color { r: 7, g: 8, b: 9, a: 1.0 };
    let mut g = AxisGrid {
        color: Some(grid),
        line_width: 2.5,
        ..Default::default()
    };

    assert_eq!(g.resolved_tick_color(theme), grid);
    assert!((g.resolved_tick_width() - 2.5).abs() < 1e-9);

    g.tick_color = Some(tick);
    g.tick_width = Some(3.5);
    assert_eq!(g.resolved_tick_color(theme), tick);
    assert!((g.resolved_tick_width() - 3.5).abs() < 1e-9);
}
```

Add this beside the existing `axis_grid_from_*` tests:

```rust
#[test]
fn axis_grid_from_maps_tick_style_scalars_and_array_heads() {
    use crate::schema::common::ScalarOrArray;

    let opts = GridLineOptions {
        color: Some(ScalarOrArray::One("#0000ff".into())),
        line_width: Some(ScalarOrArray::One(2.0)),
        draw_ticks: Some(true),
        tick_color: Some(ScalarOrArray::Many(vec![
            "#ff0000".into(),
            "#00ff00".into(),
        ])),
        tick_width: Some(ScalarOrArray::Many(vec![3.0, 4.0])),
        tick_length: Some(12.0),
        ..Default::default()
    };

    let g = axis_grid_from(Some(&opts));
    assert!(g.draw_ticks);
    assert_eq!(g.tick_color.unwrap().r, 255);
    assert_eq!(g.tick_width, Some(3.0));
    assert!((g.tick_length - 12.0).abs() < 1e-9);
}
```

Run:

```bash
cargo test -p fulgur-chart ir::tests::axis_grid_tick_style_resolves_override_then_grid_fallback -- --exact
```

Expected RED: compilation fails because the tick fields and resolution methods
do not exist.

#### Step 2: Add the IR fields, defaults, and helpers

Implement:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct AxisGrid {
    pub display: bool,
    pub color: Option<Color>,
    pub line_width: f64,
    pub draw_ticks: bool,
    pub tick_color: Option<Color>,
    pub tick_width: Option<f64>,
    pub tick_length: f64,
}

impl AxisGrid {
    pub fn resolved_tick_color(&self, fallback_grid_color: Color) -> Color {
        self.tick_color
            .or(self.color)
            .unwrap_or(fallback_grid_color)
    }

    pub fn resolved_tick_width(&self) -> f64 {
        self.tick_width.unwrap_or(self.line_width)
    }
}

impl Default for AxisGrid {
    fn default() -> Self {
        Self {
            display: true,
            color: None,
            line_width: 1.0,
            draw_ticks: false,
            tick_color: None,
            tick_width: None,
            tick_length: 8.0,
        }
    }
}
```

All existing `AxisGrid { display: false, ..Default::default() }` literals remain
valid. The Vega-Lite literal is handled explicitly in Step 4.

#### Step 3: Drive axis_grid_from from AxisGrid::default

Replace literal defaults with a single `defaults` value and add tick mapping:

```rust
fn axis_grid_from(opts: Option<&GridLineOptions>) -> AxisGrid {
    use crate::schema::common::ScalarOrArray;

    let defaults = AxisGrid::default();
    let Some(g) = opts else {
        return defaults;
    };
    let color = match &g.color {
        Some(ScalarOrArray::One(s)) => parse_color(s),
        Some(ScalarOrArray::Many(v)) => v.first().and_then(|s| parse_color(s)),
        None => defaults.color,
    };
    let line_width = match &g.line_width {
        Some(ScalarOrArray::One(w)) => *w,
        Some(ScalarOrArray::Many(v)) => {
            v.first().copied().unwrap_or(defaults.line_width)
        }
        None => defaults.line_width,
    };
    let tick_color = match &g.tick_color {
        Some(ScalarOrArray::One(s)) => parse_color(s),
        Some(ScalarOrArray::Many(v)) => v.first().and_then(|s| parse_color(s)),
        None => defaults.tick_color,
    };
    let tick_width = match &g.tick_width {
        Some(ScalarOrArray::One(w)) => Some(*w),
        Some(ScalarOrArray::Many(v)) => v.first().copied(),
        None => defaults.tick_width,
    };

    AxisGrid {
        display: g.display.unwrap_or(defaults.display)
            && g.draw_on_chart_area.unwrap_or(defaults.display),
        color,
        line_width,
        draw_ticks: g.draw_ticks.unwrap_or(defaults.draw_ticks),
        tick_color,
        tick_width,
        tick_length: g.tick_length.unwrap_or(defaults.tick_length),
    }
}
```

Keep `drawOnChartArea=false` behavior unchanged. Update the function comment to
include tick scalar/array head selection and inherited rendering semantics.

#### Step 4: Preserve the Vega-Lite temporal boundary explicitly

Pass both theme colors:

```rust
let grid = if temporal_line {
    Some(temporal_axis_grid(
        top,
        theme.grid_color,
        theme.text_color,
    )?)
} else {
    None
};
```

Change the helper signature and literal:

```rust
fn temporal_axis_grid(
    top: &Map<String, Value>,
    theme_grid_color: Color,
    theme_text_color: Color,
) -> Result<AxisGrid, String> {
    let display = axis
        .and_then(|axis| axis.get("grid"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    Ok(AxisGrid {
        display,
        color: opacity.map(|_| grid_color),
        line_width: 1.0,
        draw_ticks: true,
        tick_color: Some(theme_text_color),
        tick_width: None,
        tick_length: 4.0,
    })
}
```

Strengthen `grid_opacity_does_not_fade_temporal_tick_marks` before running the
suite:

```rust
assert!(
    tick_strokes
        .iter()
        .all(|stroke| *stroke == spec.theme.text_color),
    "temporal ticks must retain the theme text color: {tick_strokes:?}"
);
assert!((spec.x_axis.grid.tick_length - 4.0).abs() < 1e-9);
```

#### Step 5: Verify GREEN

Run:

```bash
cargo test -p fulgur-chart ir::tests::axis_grid_default_matches_fulgur_backward_compat -- --exact
cargo test -p fulgur-chart ir::tests::axis_grid_tick_style_resolves_override_then_grid_fallback -- --exact
cargo test -p fulgur-chart frontend::chartjs::tests::axis_grid_from_maps_tick_style_scalars_and_array_heads -- --exact
cargo test -p fulgur-chart --test render_vegalite_temporal_line grid_opacity_does_not_fade_temporal_tick_marks -- --exact
cargo test -p fulgur-chart
```

Expected: all pass; production rendering is not changed yet.

#### Step 6: Commit

```bash
git add crates/fulgur-chart/src/ir.rs \
  crates/fulgur-chart/src/frontend/chartjs.rs \
  crates/fulgur-chart/src/frontend/vegalite.rs \
  crates/fulgur-chart/tests/render_vegalite_temporal_line.rs
git commit -m "feat(chartjs): propagate tick styling to axis IR"
```

---

### Task 3: Render independent tick color, width, and length

**Files:**

- Modify/test: `crates/fulgur-chart/src/layout/common.rs:756-776`
- Modify/test: `crates/fulgur-chart/src/layout/common.rs:819-850`
- Modify/test: `crates/fulgur-chart/src/layout/common.rs:1774-1824`
- Modify/test: `crates/fulgur-chart/src/layout/scatter.rs:369-400`
- Modify/test: `crates/fulgur-chart/src/layout/scatter.rs:681-835`
- Modify/test: `crates/fulgur-chart/src/layout/bar.rs:401-417`
- Modify/test: `crates/fulgur-chart/src/layout/bar.rs:1018-1041`

**Interfaces:**

- Consumes: `AxisGrid` fields and resolution helpers from Task 2
- Produces: styled `Prim::Line` ticks on every existing Cartesian tick path

#### Step 1: Add a failing common-frame rendering test

Add beside `grid_draw_ticks_true_adds_tick_marks`:

```rust
#[test]
fn grid_tick_style_fields_reach_common_tick_primitives() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.y_axis.grid.draw_ticks = true;
    spec.y_axis.grid.color = Some(Color {
        r: 0, g: 0, b: 255, a: 1.0,
    });
    spec.y_axis.grid.line_width = 1.0;
    spec.y_axis.grid.tick_color = Some(Color {
        r: 255, g: 0, b: 0, a: 1.0,
    });
    spec.y_axis.grid.tick_width = Some(2.5);
    spec.y_axis.grid.tick_length = 7.0;
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);

    let ticks = items
        .iter()
        .filter(|item| {
            matches!(item,
                Prim::Line { x1, x2, y1, y2, stroke, stroke_width, .. }
                    if (y1 - y2).abs() < 0.01
                        && ((*x2 - *x1) - 7.0).abs() < 1e-9
                        && (*x2 - frame.plot_left).abs() < 0.01
                        && stroke.r == 255 && stroke.g == 0 && stroke.b == 0
                        && (*stroke_width - 2.5).abs() < 1e-9
            )
        })
        .count();
    assert_eq!(ticks, frame.ticks.ticks.len());
}
```

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::grid_tick_style_fields_reach_common_tick_primitives -- --exact
```

Expected RED: no 7px red 2.5px-wide tick exists; production still emits the
fixed 4px grid-colored 1px-wide line.

#### Step 2: Add failing scatter and horizontal-bar tests

Add a scatter test using both axes:

```rust
#[test]
fn scatter_tick_styles_are_independent_on_both_axes() {
    let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
    spec.x_axis.grid.draw_ticks = true;
    spec.x_axis.grid.tick_color = Some(Color {
        r: 0, g: 128, b: 0, a: 1.0,
    });
    spec.x_axis.grid.tick_width = Some(3.0);
    spec.x_axis.grid.tick_length = 6.0;
    spec.y_axis.grid.draw_ticks = true;
    spec.y_axis.grid.tick_color = Some(Color {
        r: 255, g: 0, b: 0, a: 1.0,
    });
    spec.y_axis.grid.tick_width = Some(2.0);
    spec.y_axis.grid.tick_length = 7.0;

    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let layout = compute_scatter_layout(&spec, &m);
    let scene = build(&spec, &m);
    let x_tick = scene.items.iter().any(|item| {
        matches!(item,
            Prim::Line { x1, x2, y1, y2, stroke, stroke_width, .. }
                if (x1 - x2).abs() < 0.01
                    && (*y1 - layout.plot_bottom).abs() < 0.01
                    && ((*y2 - *y1) - 6.0).abs() < 1e-9
                    && stroke.g == 128
                    && (*stroke_width - 3.0).abs() < 1e-9
        )
    });
    let y_tick = scene.items.iter().any(|item| {
        matches!(item,
            Prim::Line { x1, x2, y1, y2, stroke, stroke_width, .. }
                if (y1 - y2).abs() < 0.01
                    && (*x2 - layout.plot_left).abs() < 0.01
                    && ((*x2 - *x1) - 7.0).abs() < 1e-9
                    && stroke.r == 255
                    && (*stroke_width - 2.0).abs() < 1e-9
        )
    });
    assert!(x_tick && y_tick);
}
```

Replace the horizontal tick test input and assertion with explicit independent
fields:

```rust
let spec = parse(
    r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
        "options":{"indexAxis":"y","scales":{"x":{"grid":{
            "drawTicks":true,
            "tickColor":"#123456",
            "tickWidth":2.75,
            "tickLength":9
        }}}}}"##,
);
let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
let scene = build(&spec, &m);
let ticks = scene.items.iter().filter(|item| {
    matches!(item,
        Prim::Line { x1, x2, y1, y2, stroke, stroke_width, .. }
            if (x1 - x2).abs() < 0.01
                && ((*y2 - *y1) - 9.0).abs() < 1e-9
                && stroke.r == 0x12 && stroke.g == 0x34 && stroke.b == 0x56
                && (*stroke_width - 2.75).abs() < 1e-9
    )
}).count();
assert!(ticks > 0);
```

Run:

```bash
cargo test -p fulgur-chart layout::scatter::tests::scatter_tick_styles_are_independent_on_both_axes -- --exact
cargo test -p fulgur-chart layout::bar::horizontal_axis_style_tests::horizontal_x_grid_draw_ticks_true_adds_bottom_tick_marks -- --exact
```

Expected RED: both layouts still use fixed 4px geometry and grid style.

#### Step 3: Replace every existing fixed tick style

In `common::draw_frame`, remove `TICK_LEN` and resolve each axis independently:

```rust
let tick_color = ticks_cfg.resolved_tick_color(spec.theme.grid_color);
let tick_width = ticks_cfg.resolved_tick_width();
// ...
x1: frame.plot_left - ticks_cfg.tick_length,
stroke: tick_color,
stroke_width: tick_width,
```

For temporal x ticks use:

```rust
let tick_color = x_grid.resolved_tick_color(spec.theme.grid_color);
let tick_width = x_grid.resolved_tick_width();
// ...
y2: frame.plot_bottom + x_grid.tick_length,
stroke: tick_color,
stroke_width: tick_width,
```

Apply the same field/method mapping to both `scatter` axes and the horizontal
bar x axis. Do not change gridline, border, label, or plot geometry code.

Update the existing generic common test from 4px to the Chart.js default 8px:

```rust
// tick: x1 = plot_left - 8, x2 = plot_left
&& ((*x2 - *x1) - 8.0).abs() < 1e-9
```

The horizontal test already uses explicit 9px after Step 2. The Vega-Lite
temporal regression remains at 4px because its frontend sets that value
explicitly.

#### Step 4: Verify GREEN and unchanged unrelated output

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::grid_tick_style_fields_reach_common_tick_primitives -- --exact
cargo test -p fulgur-chart layout::common::tests::grid_draw_ticks_true_adds_tick_marks -- --exact
cargo test -p fulgur-chart layout::scatter::tests::scatter_tick_styles_are_independent_on_both_axes -- --exact
cargo test -p fulgur-chart layout::bar::horizontal_axis_style_tests::horizontal_x_grid_draw_ticks_true_adds_bottom_tick_marks -- --exact
cargo test -p fulgur-chart --test render_vegalite_temporal_line grid_opacity_does_not_fade_temporal_tick_marks -- --exact
cargo test -p fulgur-chart
git status --short
```

Expected: all tests pass, no `.snap.new` files appear, and only the planned Rust
files are modified.

#### Step 5: Commit

```bash
git add crates/fulgur-chart/src/layout/common.rs \
  crates/fulgur-chart/src/layout/scatter.rs \
  crates/fulgur-chart/src/layout/bar.rs
git commit -m "feat(chartjs): render independent axis tick styles"
```

---

### Task 4: Verify, close Bead, and publish the branch

**Files:**

- Verify: every committed branch change
- Update: Bead `fulgur-chart-kjx`

**Interfaces:**

- Consumes: Tasks 1-3 and the approved design
- Produces: formatted, tested, clippy-clean code; 100% executable patch
  coverage; closed and synchronized Beads state; clean pushed branch

#### Step 1: Run repository quality gates

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p fulgur-chart --target wasm32-unknown-unknown
```

Expected: every command exits 0 with no Rust warnings. If formatting changes a
file, fold that edit into the owning task's commit and rerun the gates.

#### Step 2: Generate fresh committed-HEAD coverage

Run:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-kjx.info
git diff --unified=0 origin/main...HEAD -- '*.rs'
```

Enumerate every added executable Rust line in the zero-context diff. Exclude
only blank lines, comments, attributes, and braces-only lines. Match every
remaining `path:line` to the LCOV `DA:<line>,<hits>` record. Expected:
`100.00%` with no missed changed executable lines. If a line is missed, add a
behavior-focused test, observe RED when possible, commit the test, and
regenerate coverage from the new committed `HEAD`.

#### Step 3: Record verification and close the Bead

Run:

```bash
bd update fulgur-chart-kjx --notes "Chart.js 4.5.1 の tickColor/tickWidth/tickLength を typed schema から全既存 tick path へ伝搬。drawTicks=false と Vega-Lite temporal 4px contract を維持し、workspace tests/clippy/wasm check、100% patch coverage を確認。未追跡 compat-gap 文書は変更なし。"
bd close fulgur-chart-kjx --reason "Chart.js axis tick styling fields implemented and verified."
```

Expected: `fulgur-chart-kjx` is closed with verification evidence.

#### Step 4: Synchronize and push

Before pushing, report that the completed commits and Beads state will be
published. Then run:

```bash
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected: branch and Beads pushes succeed; status is clean and
`feat/kjx-tick-fields` is up to date with `origin/feat/kjx-tick-fields`.
Do not create a pull request unless separately requested.
