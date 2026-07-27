# Horizontal Bar X-Axis Border Design

## Goal

Make Chart.js horizontal bars (`indexAxis: "y"`) draw the value-axis border
along the bottom of the plot area, matching Chart.js 4.5.1.

## Evidence

In Chart.js 4.5.1, a horizontal bar's x scale resolves to `position: "bottom"`.
Its `drawBorder()` implementation draws a horizontal line at the scale's
`_borderValue` when `border.display` is true, with `border.width` controlling
the line thickness. The current `build_horizontal` implementation deliberately
omits that line, so it diverges from the observed upstream behavior.

## Scope

- Add the bottom x-axis border in `layout::bar::build_horizontal`.
- Honor `spec.x_axis.border.display`, `color`, `width`, and `dash`.
- Use the existing theme text color when no border color is specified, matching
  the other Cartesian-axis border paths in Fulgur Chart.
- Preserve the existing left y-axis border and tick/grid behavior.
- Update horizontal-bar snapshots affected by the new default border.

Refactoring shared axis rendering, changing plot geometry, or modifying vertical
bar, scatter, and boxplot rendering is outside this issue.

## Design

Immediately after the x-axis grid and tick labels are emitted,
`build_horizontal` will inspect `spec.x_axis.border`. When `display` is true it
will append one `Prim::Line` from `(plot_left, plot_bottom)` to
`(plot_right, plot_bottom)`. The line will use the configured width and dash
pattern, and either the configured color or the theme text color.
`display` is the sole emission control: `display=false` omits the primitive,
while `display=true` emits it regardless of width. A configured `width=0` is
preserved as `stroke_width=0` on the emitted `Prim::Line`.

The existing y-axis border remains a separate `Prim::Line` from the plot's
top-left to bottom-left. Keeping the two axes explicit preserves the current
transposed layout boundary and avoids unrelated changes to `common::draw_frame`.

## Testing

Follow test-driven development:

1. Add a scene-level test proving an explicit x-axis border color, width, and
   dash pattern reach a horizontal line at the plot bottom.
2. Add a scene-level test proving `x_axis.border.display=false` removes that
   bottom border.
3. Observe both tests fail against the current implementation.
4. Add the minimal `Prim::Line` emission and make the tests pass.
5. Review and accept only the expected horizontal-bar snapshot changes,
   including stacked horizontal bars.
6. Run formatting, the complete `fulgur-chart` test suite, clippy, and the
   repository patch-coverage gate with 100% changed-line coverage.
