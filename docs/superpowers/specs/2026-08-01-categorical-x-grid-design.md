# Categorical X-Axis Grid Design

## Goal

Render vertical grid lines for categorical Chart.js x axes using the already-parsed `display`, `color`, and `lineWidth` options.

## Scope

`layout::common::draw_frame` owns categorical labels and the shared frame used by line, bar, and mixed charts. Extend only its `XPositions::Category` branch. Temporal, scatter, and horizontal-bar paths already render their own x-axis grids and are out of scope.

## Geometry

For every category position, add a `Prim::Line` from `frame.plot_top` to `frame.plot_bottom` at the x coordinate that its label would use:

- `ChartKind::Line`: `line_x(spec, frame, index)`, preserving `x_axis.offset`.
- all other common-frame charts: `category_center(frame, index, category_count)`.

The line is emitted only when `spec.x_axis.grid.display` is true, with `color.unwrap_or(spec.theme.grid_color)` and `line_width`. Label auto-skip affects text only, so dense categories keep their grid lines. Labels remain independent of grid visibility.

## Verification

Unit tests in `layout/common.rs` will first prove that categorical vertical grid lines are absent, then verify their coordinates, style propagation, and suppression by `display: false`. Existing temporal-grid coverage remains unchanged.
