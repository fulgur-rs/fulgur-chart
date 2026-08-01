# Categorical tick and grid compatibility design

## Goal

Match Chart.js categorical-axis behavior by deriving grid segments from the
same auto-skipped tick set as labels, while preserving a tick and its grid
segment when its label text is empty.

## Scope

Only the categorical branch of `layout::common::draw_frame` changes. The
existing automatic step calculation, line/bar coordinates, and single batched
`Prim::Path` representation remain in scope; temporal and scatter axes do
not change.

## Design

First compute the visible category tick indices from the automatic `step`.
For every visible index, append one vertical segment to the batched grid path
when `x_axis.grid.display` is true. Draw `Prim::Text` only when the category
label is non-empty. Thus the grid and label use the same selected tick set,
but the presence of text no longer removes the grid for an empty label.

The grid path stays as independent `M x top L x bottom` subpaths with the
existing color and line width. This retains the batching optimization and the
same geometry for line, bar, and mixed charts.

## Validation

Add a unit test with an empty category label that verifies its grid subpath is
retained while no text primitive is emitted. Keep the dense-category test to
verify that grid segments follow the selected tick set. Run the focused test,
the crate test suite, formatting, clippy, golden rendering tests, and the
committed dhat benchmark gate.
