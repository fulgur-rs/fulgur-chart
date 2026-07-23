# Vega-Lite temporal line dogfood parity design

**Date:** 2026-07-23

**Issue:** `fulgur-chart-6an`

**Parent dependency:** `fulgur-chart-s7o` / PR #136

**Reference fixture:** `fulgur-rs/flpdf-qtest`, branch `metrics-data`

## Context

`flpdf-qtest` renders its nightly metrics with both `vl-convert` and
`fulgur-chart --dsl vegalite`. Both renderers receive the same Vega-Lite spec:

- `mark.type = "line"`
- `mark.point = true`
- `mark.interpolate = "monotone"`
- temporal x encoding
- quantitative y encoding
- nominal color encoding using `tableau10`
- axis grid with `gridOpacity = 0.15`
- a title and explicit plot dimensions of 720 × 320

The fulgur output currently preserves the data values but loses important
semantics. It treats timestamps as equally spaced categories, assigns colors in
first-seen order, ignores monotone interpolation and channel titles, draws no
temporal ticks or vertical grid, places an untitled legend above the plot, and
treats Vega-Lite plot dimensions as outer canvas dimensions.

This design closes those gaps for the dogfood spec while keeping existing
Chart.js rendering byte-stable. It does not attempt to clone Vega's SVG DOM or
font metrics.

## Goals

1. Place temporal line points according to their actual RFC 3339 timestamps.
2. Preserve Vega-Lite's nominal domain-to-color mapping and legend order.
3. Render `interpolate: "monotone"` without overshooting the input data.
4. Render temporal ticks, vertical and horizontal grid, channel titles, and a
   titled right-side legend.
5. Treat Vega-Lite width and height as plot dimensions.
6. Keep existing Chart.js and categorical Vega-Lite charts unchanged.
7. Produce deterministic SVG, PNG, WebP, and model output.
8. Reach 100% patch coverage on the final committed change.

## Non-goals

- Byte-identical or pixel-identical output with `vl-convert`
- Matching Vega's SVG grouping, ARIA attributes, CSS classes, or path ordering
- Matching browser font metrics
- Supporting every input accepted by Vega-Lite temporal fields
- Supporting arbitrary Vega transforms, expressions, data URLs, or signals
- Replacing the existing category and linear scale implementations with a
  universal Vega scale engine
- Supporting all Vega interpolation modes

## Chosen approach

Extend the existing line IR with an optional positioned x-axis mode. Category
lines continue to use their current index-derived positions. A temporal line
stores one shared, sorted x-domain because the current colored-line contract
already requires every series to contain every x value.

This is preferred over two alternatives:

1. Formatting the current category labels without changing geometry would
   leave unequal time gaps equally spaced.
2. Introducing a general category/linear/temporal scale engine would broaden
   the change far beyond the dogfood requirement and increase regression risk.

## Dependency and branch structure

PR #136 adds typed `AxisTitle`, `AxisGrid`, and `AxisBorder` IR plus the shared
axis-title and grid rendering foundation. This work is implemented on
`feat/6an-vegalite-dogfood-parity`, stacked on
`feat/s7o-axis-styling`.

The stacked branch must be rebased onto `main` after PR #136 merges. The final
PR must contain only the temporal-line parity commits relative to the merged
parent.

## Input contract

### Supported Vega-Lite fields

The typed Vega-Lite schema and strict-key allowlists gain support for:

- top-level `background`
- top-level `config.axis.grid`
- top-level `config.axis.gridOpacity`
- line mark object `point`
- line mark object `interpolate`
- encoding channel `title`
- encoding color channel `scale.scheme`
- `encoding.x.type = "temporal"` for line marks

`mark.interpolate` accepts `linear` and `monotone`. `linear` retains the current
polyline path. `monotone` selects the new monotone cubic path.

`scale.scheme` accepts `tableau10` in the first implementation. In strict mode,
an unsupported explicit scheme is rejected. In non-strict mode, it falls back
to the Vega-Lite default palette.

### Temporal value format

The first implementation accepts RFC 3339 strings, including:

- `Z`
- numeric UTC offsets
- optional fractional seconds

Parsing uses the `time` crate's well-tested RFC 3339 parser rather than a
project-local date parser. Parsed values are normalized to UTC Unix
milliseconds. This needs no timezone database and behaves identically on
native and wasm targets.

If a channel declares `type: "temporal"`, an invalid or missing timestamp is a
parse error in both strict and non-strict modes. It does not fall back to a
category. The user-facing English error includes the field name and rejected
value but not the full input document.

### Ordering and duplicates

Unique timestamps are sorted by normalized UTC value before series are built.
All colored series use that shared order.

Two distinct strings representing the same instant collapse into one x bucket.
Values with the same normalized timestamp and series are summed, matching the
existing categorical aggregation rule. The first encountered source string is
retained only as diagnostic metadata; axis labels come from generated ticks.

The existing dense colored-line requirement remains. A missing
timestamp/series pair is rejected rather than silently filled with zero.

## IR design

### Positioned x values

Add an x-position mode to `ChartSpec`:

```rust
pub enum XPositions {
    Category,
    Temporal {
        unix_millis: Vec<i64>,
    },
}
```

`Category` is the default used by every existing parser. `Temporal` must have
the same length as `ChartSpec.categories` and every line series' value vector.
The guard layer validates that invariant before layout.

The field belongs to `ChartSpec`, not `Series`, because the current line+color
contract has a shared dense x-domain. This avoids duplicating timestamps for
every series and keeps the existing primitive-count and memory bounds.

### Interpolation

Replace the line-specific interpretation of `Series.tension` with an explicit
mode while preserving tension for Chart.js:

```rust
pub enum LineInterpolation {
    Linear,
    CatmullRom { tension: f64 },
    Monotone,
}
```

The Chart.js frontend maps its current tension behavior to `Linear` or
`CatmullRom`. The Vega-Lite frontend maps `linear` and `monotone` directly.
Other chart kinds ignore the field as they ignore tension today.

This explicit enum prevents a magic tension value from pretending to be a
monotone curve.

### Size semantics

Add a size mode:

```rust
pub enum SizeMode {
    Canvas,
    PlotArea,
}
```

All existing Chart.js inputs and existing categorical Vega-Lite behavior use
`Canvas`. The supported Vega-Lite temporal line uses `PlotArea`, where
`ChartSpec.width` and `height` are the requested plot dimensions.

`Frame` gains outer canvas dimensions. In `PlotArea` mode:

- `plot_right - plot_left == spec.width`
- `plot_bottom - plot_top == spec.height`
- title, axes, labels, padding, and legend expand the outer canvas
- `line::build` uses the frame's outer dimensions for `Scene`

This mode is intentionally activated only for the newly supported temporal
line contract in this change. Other existing Vega-Lite kinds retain canvas
dimensions because changing their output sizes is outside this issue.

### Legend title

Add `legend_title: Option<String>` to `ChartSpec`. This is the smallest change
that allows `encoding.color.title` to reach the existing legend renderer
without redesigning the Chart.js legend contract.

For a temporal Vega line:

- position defaults to `LegendPos::Right`
- title defaults to the color field name when channel title is absent
- title is included in right legend width and is drawn above its entries

Existing specs set `legend_title` to `None`, preserving their output.

## Frontend conversion

The Vega-Lite frontend performs these steps:

1. Parse and validate the supported line mark and channel options.
2. Parse every temporal x value as RFC 3339 and normalize it to Unix
   milliseconds.
3. Sort and deduplicate the shared temporal domain.
4. Build every series against that domain and retain the existing dense-series
   validation.
5. Sort nominal color values lexicographically, matching Vega-Lite's default
   nominal domain ordering for this spec.
6. Assign `tableau10` colors after sorting, producing:
   - `allowlist` → `#4c78a8`
   - `candidates` → `#f58518`
   - `regressions` → `#e45756`
7. Populate x and y axis titles from channel titles, falling back to field
   names.
8. Populate the right legend and legend title from the color channel.
9. Apply axis grid visibility and opacity to both axes.
10. Set the Vega line width to 2 px. Map `point: true` to an explicit 3 px
    radius and `point: false` or absence to an explicit zero radius.

For backward compatibility, a line series with no explicit point radius keeps
the current 3 px marker. The line renderer starts honoring an explicit radius
for non-decimated lines as well as decimated lines; therefore Vega's explicit
zero suppresses points without changing existing unspecified Chart.js series.

The color-domain sorting is scoped to nominal color encodings. Existing
categorical ordering without a nominal color encoding remains first-seen and
deterministic.

## Temporal scale and ticks

### Point positioning

`Frame` contains a continuous x scale for temporal input. Each timestamp maps
linearly from the minimum and maximum Unix milliseconds to the left and right
plot edges.

A single unique timestamp is placed at the plot center. Duplicate timestamps
cannot cause division by zero because they are aggregated before layout.

### Tick generation

Tick generation is deterministic and UTC-only. The target tick count is
`clamp(floor(plot_width / 30), 2, 24)`. The generator selects the smallest
interval from a bounded table of fixed UTC intervals whose tick count does not
exceed that target. The table covers seconds through weeks, which includes the
dogfood range. Ticks align to interval boundaries in UTC.

For the current June 5 through July 22 fixture, this produces two-day grid/tick
positions. Label auto-skip may hide alternating labels while retaining every
grid line and tick. Labels use compact deterministic UTC forms:

- first visible tick in a month: `Jun 07`
- later ticks in that month: `09`, `11`, and so on
- first visible tick in a new month: `Jul 01`

Formatting is deliberately smaller than Vega's full locale-aware formatter.

### Grid and axes

The temporal x-axis renderer adds:

- vertical grid lines for every generated tick
- bottom tick marks
- auto-skipped date labels
- the x-axis title

The existing y-axis renderer adds the y-axis title through the PR #136 IR.
`config.axis.grid = false` disables both horizontal and vertical chart-area
grid lines. `gridOpacity` multiplies the Vega theme grid color alpha and applies
equally to SVG and raster output.

## Vega-style quantitative y domain

The temporal Vega line uses a separate Vega-style y-domain policy so existing
Chart.js ticks do not change:

1. Include zero when all values are positive or all are negative.
2. Add a small open-end padding based on the data span.
3. Select a decimal 1/2/5 nice step from plot height.
4. Round the padded domain outward to a half-step boundary.
5. Emit labeled ticks at full-step boundaries inside the domain.

For positive-only values, the lower domain stays at zero and the upper raw
domain receives padding equal to 5% of the data span. The target labeled tick
count is `clamp(floor(plot_height / 40), 2, 10)`. After choosing the full
1/2/5 step, the padded upper domain rounds outward to the next half-step.

For the dogfood values `0..61` at 320 px plot height, this produces an exact
domain of `0..65` with labeled ticks `0, 10, ..., 60`. Tests assert this fixture
result and the general finite/order invariants rather than claiming full Vega
scale compatibility.

## Monotone interpolation

Implement monotone cubic Hermite interpolation using x-aware secant slopes and
slope limiting equivalent to a monotone-X curve:

- x values must be nondecreasing
- finite inputs produce finite control points
- horizontal segments remain horizontal
- local extrema receive a zero tangent
- control points are limited so a segment cannot overshoot its endpoint range
- two points degrade to a straight segment
- one point produces only its marker

The generated SVG uses cubic `C` commands. The raster path parser already
supports cubic paths, so SVG, PNG, and WebP share the same geometry.

Chart.js Catmull-Rom rendering remains on its existing path.

## Rendering order

The scene order is:

1. background
2. title
3. horizontal and vertical grid
4. axis domains, ticks, labels, and titles
5. line paths
6. point markers
7. legend title and entries

Series are stored and rendered in nominal domain order. This keeps legend
order, color assignment, and line order deterministic.

## Error handling

New errors are returned before allocation-heavy layout work:

- temporal field missing or null
- temporal value is not a string
- RFC 3339 parsing fails
- temporal domain length differs from category or series length
- unsupported explicit interpolation in strict mode
- unsupported explicit color scheme in strict mode

Non-strict mode still ignores unrelated unknown keys, but it does not ignore an
invalid value for a recognized semantic field.

No error includes the full JSON input. Timestamp errors include only the field
name and offending scalar value, subject to the existing input-length guard.

## Guard and resource accounting

- Parsed temporal positions count exactly like existing categories; they do
  not increase the logical point limit.
- `unix_millis` allocation occurs only after record and category limits pass.
- Date tick generation is bounded by plot width and a hard maximum tick count.
- Monotone interpolation is linear in rendered points and runs after existing
  line decimation.
- Decimation retains x values and original category indices together so
  temporal positions cannot become detached from values or labels.

## Tests

### Frontend tests

- accepts RFC 3339 `Z`, offsets, and fractional seconds
- normalizes equivalent instants
- rejects invalid, null, missing, and non-string temporal values
- sorts temporal input independently of record order
- aggregates duplicate instants
- preserves dense colored-series validation after sorting
- sorts nominal domains and pins the three dogfood colors
- accepts `point`, `monotone`, channel titles, `tableau10`, and axis config in
  strict mode
- rejects supported-field typos and unsupported explicit values in strict mode

### Scale and layout tests

- gaps of one and three days produce a 1:3 x-distance ratio
- a singleton temporal domain maps to plot center
- tick generation is deterministic and bounded
- the dogfood date range selects two-day ticks
- labels auto-skip without removing grid lines
- `gridOpacity = 0.15` reaches scene primitives
- plot area is exactly 720 × 320 while outer scene dimensions are larger
- axis and legend titles reserve sufficient space

### Monotone geometry tests

- increasing, decreasing, constant, and local-extrema data do not overshoot
- two points degrade safely
- duplicate timestamps are aggregated before geometry
- all emitted coordinates are finite
- SVG and raster rendering both accept the generated cubic path

### End-to-end fixture

Add a reduced fixture with the same shape as the `flpdf-qtest` spec and enough
dates to exercise month rollover and uneven spacing. Its SVG snapshot pins:

- `allowlist`, `candidates`, and `regressions` color mapping
- title and both axis titles
- titled right legend
- temporal labels and vertical grid
- monotone cubic paths
- point markers
- outer dimensions derived from a 720 × 320 plot

As a final manual verification, render the full current `metrics.jsonl` data
through the same generated spec and compare it side by side with the
`vl-convert` artifact. The external evolving dataset is not vendored into unit
tests.

### Regression and coverage gates

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p fulgur-chart`
- repository patch-coverage command against the final committed `HEAD`
- patch coverage must be 100%
- existing Chart.js line snapshots must remain unchanged

## Acceptance criteria

The work is complete when:

1. The reduced dogfood fixture renders successfully in strict and non-strict
   modes.
2. Its temporal point spacing reflects actual time differences.
3. Its nominal series names, colors, and legend order match Vega-Lite.
4. Its line paths use bounded monotone cubic interpolation.
5. Its x/y axis titles, temporal ticks, grid opacity, and titled right legend
   are visible.
6. Its plot area is 720 × 320 and the outer canvas expands around it.
7. Invalid RFC 3339 values fail with a bounded English error.
8. Existing Chart.js output remains unchanged.
9. All quality gates pass with 100% final patch coverage.
