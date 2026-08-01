# Batched categorical X-grid rendering design

## Goal

Keep categorical x-axis grids visually identical while avoiding one `Prim::Line`
and one SVG element per visible tick. This removes the fixed allocation increase
that the new grid feature introduced in small bar charts.

## Scope

Only `layout::common::draw_frame` for categorical x axes changes. Temporal and
scatter grids retain their existing per-line primitives. The public chart
configuration and all grid styles remain unchanged.

## Design

`draw_frame` builds one SVG path for all visible categorical x-grid segments
when `x_axis.grid.display` is true. Each segment is a separate `M x top L x
bottom` subpath. The path has no fill and uses the existing grid color and line
width. Category labels remain separate `Prim::Text` items.

The visible tick rule remains the existing automatic `step`: an empty or
skipped category produces neither a grid segment nor a label. Line-chart
positions still honor `x_axis.offset`; bar and mixed charts use band centers.

## Compatibility and validation

The SVG markup changes from individual `<line>` tags to one `<path>`, so
affected snapshots and deterministic SVG/PNG expectations are regenerated.
Tests verify the number and positions of path segments, `display: false`, line
offset behavior, and tick auto-skipping. The dhat membench gate must pass with
the committed baseline unchanged.
