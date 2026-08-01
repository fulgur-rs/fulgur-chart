# Categorical X-Axis Grid Implementation Plan

> **For agentic workers:** Historical implementation plan. Task state is tracked in bd.

**Goal:** Render styled vertical grid lines for categorical Chart.js x axes.

**Architecture:** Keep the change in the shared `draw_frame` category branch, beside the x-label position calculation. The feature consumes existing `AxisGrid` IR data; no schema or frontend change is needed. Geometry matches the displayed label: `line_x` for line charts and category-band centers otherwise.

**Tech Stack:** Rust, fulgur-chart scene primitives, Cargo unit tests.

## Global Constraints

- Restrict behavior to categorical common-frame charts; do not alter temporal, scatter, or horizontal-bar rendering.
- Honor existing `x_axis.grid.display`, `color`, and `line_width` semantics.
- Preserve label auto-skip and keep labels visible when grids are hidden.

---

### Task 1: Prove and implement categorical vertical-grid rendering

**Files:**

- Modify: `crates/fulgur-chart/src/layout/common.rs:784-821`
- Test: `crates/fulgur-chart/src/layout/common.rs:tests`

**Interfaces:**

- Consumes: `ChartSpec.x_axis.grid: AxisGrid`, `line_x`, `category_center`, `Frame`, and `Prim::Line`.
- Produces: one plot-height vertical `Prim::Line` for each categorical position while x-grid display is enabled.

**Step 1: Write the failing tests**

Add a bar-chart test that sets a distinctive x-grid color and width, calls `draw_frame`, and asserts three plot-height vertical lines at `category_center` positions. Set `display = false` and assert those lines are absent while the `A`/`B`/`C` labels remain. Add a dense-label case proving text auto-skip does not remove grid lines. Add a line-chart test with `offset = false` that asserts the first and final vertical lines use `line_x` at the plot edges.

**Step 2: Run tests to verify they fail**

Run: `cargo test -p fulgur-chart categorical_x_grid --lib`

Expected: FAIL because the categorical `XPositions::Category` branch emits text but no plot-height vertical `Prim::Line`.

**Step 3: Write minimal implementation**

In the category loop, calculate the x position for every index and conditionally append the vertical line before applying the label's empty/auto-skip guard:

```rust
if x_grid.display {
    items.push(Prim::Line {
        x1: x,
        y1: frame.plot_top,
        x2: x,
        y2: frame.plot_bottom,
        stroke: x_grid.color.unwrap_or(spec.theme.grid_color),
        stroke_width: x_grid.line_width,
        dash: Vec::new(),
    });
}
```

Bind `x_grid` and its resolved color once before the category loop. Keep the existing label x calculation and auto-skip guard solely for text. Do not change temporal rendering.

**Step 4: Run focused tests to verify they pass**

Run: `cargo test -p fulgur-chart categorical_x_grid --lib`

Expected: PASS for the new bar and line coordinate/style tests.

**Step 5: Run regression tests**

Run: `cargo test -p fulgur-chart layout::common:: --lib`

Expected: PASS, including existing temporal-grid and y-grid assertions.

**Step 6: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "feat(chartjs): draw categorical x-axis grids"
```
