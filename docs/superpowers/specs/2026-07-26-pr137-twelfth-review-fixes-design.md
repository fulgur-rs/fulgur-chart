# PR #137 Twelfth Review Fixes Design

## Context

Three current PR #137 review threads identify places where validation or layout
activation is broader or narrower than the supported rendering semantics:

- `PRRT_kwDOS-i3R86T1swv`: non-strict temporal lines accept a present
  `encoding.color` object without a `field`, then silently discard its title and
  scale while rendering one ungrouped series;
- `PRRT_kwDOS-i3R86T1sww`: any `legend_title` currently activates the common
  legend, even though titles are rendered only for temporal PlotArea charts
  with a right legend;
- `PRRT_kwDOS-i3R86T1swx`: marker-radius validation scans every stored radius,
  including values ignored by the selected chart kind or overridden by a
  bubble point radius.

The fixes must preserve the strict/non-strict unknown-key boundary, existing
supported temporal legends, and the hard stop for marker radii that can reach a
rendered `Prim::Circle`.

## Temporal color channel shape

Add a shared semantic validator for a present, non-null temporal line color
channel. It requires:

- `encoding.color` to be an object;
- `encoding.color.field` to exist and be a string.

Call it from the common temporal-line path before the color scheme and temporal
series are built. Both strict and non-strict modes therefore return the same
recognized-shape errors. Keep strict-only allow-list checks in
`check_line_keys`; non-strict mode continues to tolerate unknown future keys.
A missing or null color channel remains valid and produces an ungrouped line.

## Supported legend activation

Make `has_legend` depend on either a non-empty series name or
`temporal_plot_right_legend_title(spec)`. A title therefore activates a legend
only in the one context where common layout can reserve and draw it:
temporal x positions, PlotArea sizing, and a right legend.

Named categorical series retain their existing legends. An unnamed categorical
series with only an unsupported title emits neither a legend band nor blank
swatches. The existing temporal dogfood legend title continues to reserve a
right band and precede the series entries.

## Effective marker-radius validation

Select radius candidates with the same chart-kind and point-source semantics as
layout, then apply the existing finite/maximum checks:

- line: a series `pointRadius` is relevant only when the line has a finite
  value at a rendered category;
- scatter: a series `pointRadius` is relevant only when the series has a point
  with finite x/y coordinates; `Point::r` is ignored;
- bubble: for each point with finite x/y coordinates, `Point::r` wins; the
  series `pointRadius` is relevant only when that point has no `r`;
- all other chart kinds, including bar, pie, and mixed: stored marker radii are
  not consumed and are ignored by this safety check.

Retain the existing error text and source distinction:
`pointRadius must be finite and no greater than 32768` for a selected dataset
radius and `point.r must be finite and no greater than 32768` for a selected
point radius. This rejects dangerous radii before Scene creation while allowing
unused or overridden values.

## Tests and completion

Use red-green tests for every review reproduction and preservation boundary:

- temporal color without `field` fails identically in strict and non-strict
  modes, while missing/null color and non-strict unknown keys remain accepted;
- an unnamed categorical chart with only a title has the same frame and scene
  as its no-title baseline, while the supported temporal right title still
  activates a legend;
- unused bar/pie radii, scatter `Point::r`, and overridden bubble dataset radii
  pass, while effective line/scatter/bubble radii retain the existing errors.

After focused and workspace verification, require 100% committed-HEAD changed
line coverage, push the branch and Beads state, reply to and resolve only the
three exact threads, fresh-fetch all review state, require zero unresolved
threads, and watch CI to terminal green. Do not merge the PR.
