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

## Approved whole-branch final-review extension

The whole-branch review approved one final local-only fix wave after the three
tasks above. It adds four corrections without widening the supported frontend
surface or changing normal-range D3/Vega behavior.

### Bounded temporal tick generation

Temporal ticks have a hard maximum of 1,000. Clamp the width-derived desired
count to `1..=1_000` (`1` for non-finite or non-positive widths), then keep the
existing interval selection. Before allocating or iterating, count fixed ticks
with `i128` and calendar ticks by aligned calendar indices. If the selected
interval would exceed the cap, multiply its step by an integer stride and
realign against the same origin. This preserves aligned coverage of the whole
domain instead of truncating its first 1,000 ticks, including reversed domains.

### Finite extreme singleton scales

`nice_ticks` and `bounded_ticks` share one finite degenerate-domain expansion.
The ordinary `[value, value + 1]` result remains when `+1` advances. Otherwise
the domain expands inward by a representable scale-relative step: positive
`f64::MAX` keeps the upper endpoint and negative `-f64::MAX` keeps the lower
endpoint. The resulting domain, step, ticks, and `LinearScale` endpoint mapping
remain finite, ordered, positive-width, and bounded by the existing
`MAX_TICK_INTERVALS = 1_000`.

### Alignment-aware PlotArea title overflow

PlotArea overflow is tracked independently for left/right and top/bottom.
Horizontal X titles reserve all excess on the right for Start, all excess on
the left for End, and half per side for Center. Rotated Y titles reserve all
excess on top for Start, all excess on bottom for End, and half per side for
Center. Centered chart titles remain symmetric. Vertical legend overflow also
remains symmetric and combines with the corresponding title overflow using
`max`, while requested plot width and height remain exact.

### Shared supported-title model semantics

`temporal_plot_right_legend_title` is the crate-internal single predicate for
the supported temporal PlotArea + Right title. Common layout uses it to
activate title-only legends, and the model uses it to decide when empty series
names still count as legend entries. Unsupported categorical Canvas titles do
not change model counts. Without a supported title, the existing count of
non-empty series names remains unchanged.

For this final fix wave, verify focused RED/GREEN evidence, temporal, scale,
common-layout, model/inspect, and temporal Vega-Lite render suites, plus
`cargo fmt --all -- --check` and `git diff --check`. Update the implementation
plan and write the SDD report. This amendment is local-only: do not push,
mutate Beads, reply to or resolve GitHub threads, or merge the PR.
