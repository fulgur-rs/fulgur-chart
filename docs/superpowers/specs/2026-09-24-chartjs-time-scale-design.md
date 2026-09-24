# Chart.js Time and Timeseries Scale Design

**Date:** 2026-09-24

**Issue:** `fulgur-chart-tu4`

**Dependencies:** `fulgur-chart-pof` and `fulgur-chart-rxf` (both merged)

## Context

The Chart.js frontend currently treats ordinary line/bar labels as categories,
uses numeric values for scatter and bubble coordinates, and models only linear
and logarithmic value axes. Chart.js `time` and `timeseries` axis types are not
recognized. The repository already has UTC RFC 3339 parsing and calendar tick
generation in `temporal.rs`, used by Vega-Lite temporal line charts, but those
facilities are not connected to Chart.js axis configuration or general chart
layouts.

The issue asks for `options.scales.{x,y}.type` values `time` and `timeseries`,
the `time` options `unit`, `parser`, `displayFormats`, `round`, and `minUnit`,
string and epoch-millisecond inputs, and automatically aligned time ticks.
Support must use per-axis Chart.js configuration because the two axes may have
different scale types and parsing rules.

## Goals

1. Accept `time` and `timeseries` independently on Cartesian x and y axes.
2. Place `time` values according to elapsed UTC time and place distinct
   `timeseries` values at equal intervals in chronological order.
3. Parse ISO-8601/RFC 3339 strings and finite numeric epoch milliseconds, with
   an explicit, bounded strftime-style parser subset for `time.parser`.
4. Implement `unit`, `minUnit`, `round`, and per-unit `displayFormats`.
5. Generate bounded ticks aligned to UTC calendar boundaries, including
   quarters, and retain currently supported tick-count controls.
6. Support temporal index axes and temporal numeric axes for line, bar, and
   mixed charts, plus temporal x/y coordinates for scatter and bubble charts.
7. Preserve null gaps and preserve input point order; timestamps affect
   coordinates, not dataset ordering or aggregation.
8. Keep every chart without an explicit temporal axis unchanged.

## Non-goals

- Executing JavaScript date-adapter callbacks or loading third-party adapters.
- Locale databases, daylight-saving-time transitions, or local-time parsing;
  all input without an explicit offset and all generated ticks use UTC.
- Supporting temporal scales for pie, doughnut, polar, radar, treemap,
  word-cloud, or matrix charts.
- Adding arbitrary parsing keys, axis bounds, timezone, or adapter-specific
  options beyond those named in the issue.
- Reordering dataset points, combining duplicate timestamps, or filling nulls.
- Replacing the existing numeric, logarithmic, or category scale behavior.

## Chosen approach

Add a temporal scale mode and options to the existing per-axis IR, then route
all temporal coordinates through a shared axis-scale mapper used by horizontal
and vertical layouts. Generalize the existing line-only temporal position
contract so a category/index axis can carry parsed temporal positions. Values
already stored as per-point numeric coordinates (including scatter/bubble and
the value axis of a line/bar chart) remain attached to their points and are
interpreted as epoch milliseconds only when that axis is temporal.

This keeps parsing at the Chart.js frontend boundary and projection/tick
behavior in shared temporal/layout code. It avoids a second date parser and
does not change categorical charts. It also avoids putting date formatting in
the raster or SVG backends: both consume the same generated axis labels and
coordinates.

## Input contract

### Axis type and options

Either `options.scales.x` or `options.scales.y` may set:

```json
{
  "type": "time",
  "time": {
    "unit": "day",
    "minUnit": "hour",
    "parser": "%Y-%m-%d",
    "round": "day",
    "displayFormats": { "day": "%Y-%m-%d" }
  }
}
```

`type` accepts exactly `time` or `timeseries` for temporal axes. The supported
units are `millisecond`, `second`, `minute`, `hour`, `day`, `week`, `month`,
`quarter`, and `year`. `minUnit` is the smallest unit the automatic tick
selector may choose; omitted `minUnit` means `millisecond`. `unit`, when
present, fixes the calendar unit used for ticks, while the tick step may grow
to keep the output within the existing tick limits. `round`, when present,
rounds each parsed data timestamp down to the beginning of that UTC unit
before scale-domain calculation and positioning. `displayFormats` overrides
only labels for the units it names; other units use deterministic built-in
English UTC formats.

The time object accepts only `unit`, `minUnit`, `parser`, `round`, and
`displayFormats`. `time` is valid only with a `time` or `timeseries` axis.
Invalid enum values, wrong value types, unknown keys, and JavaScript callbacks
are errors instead of silently ignored settings.

`displayFormats` keys must be supported unit names and values must be strings.
Output formatting supports `%Y`, `%y`, `%m`, `%b`, `%B`, `%d`, `%a`, `%A`,
`%H`, `%I`, `%M`, `%S`, `%.f`, `%p`, `%z`, and `%%`; month and weekday names
are deterministic English abbreviations/full names. An unsupported directive
is a validation error.

### Timestamp values

On a temporal axis, strings and JSON numbers are parsed as follows:

- Without `time.parser`, accept ISO-8601 calendar dates (`YYYY-MM-DD`) and
  date-times (`YYYY-MM-DD[T ]HH:mm:ss`, optional fractional seconds, and an
  optional `Z` or numeric UTC offset). Offset-free values mean UTC.
- With `time.parser`, parse strings using the declared format. The supported
  directives are `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%.f`, `%z`, and `%%`.
  `%Y-%m-%d` is the date-only form; omitted time fields default to midnight,
  and an omitted offset means UTC. `%z` accepts `Z`, `+HHMM`, or `+HH:MM`.
  `%Y` is a four-digit year and `%.f` accepts one through nine fractional
  second digits; parsed sub-millisecond precision is truncated.
- Finite JSON numbers are epoch milliseconds. Values outside the JavaScript
  date range (±8.64e15 milliseconds) are rejected; fractional milliseconds
  are truncated toward zero, matching JavaScript date time clipping.
- `null` remains a missing data value and is never parsed as a timestamp.
  Existing null-gap behavior is retained for line, bar, and mixed datasets.

Malformed strings, non-finite/out-of-range numbers, or missing required
coordinates produce a bounded error naming the axis/field and rejected value.
They do not fall back to category labels. The parser option applies only to
strings; numeric epoch milliseconds bypass format parsing.

### Chart data shapes

- For line, bar, and mixed charts using the index axis, `data.labels` provide
  the temporal index values. `indexAxis: "y"` places those values on y;
  otherwise they are on x. Dataset values continue to align with labels by
  array index.
- If a temporal axis is used as a value axis, each non-null dataset value is
  interpreted as a timestamp on that axis.
- Scatter and bubble object points accept string or numeric `x` and `y` values
  for whichever axes are temporal. Existing numeric coordinates remain valid
  on linear axes. Bubble radius handling is unchanged.
- Mixed datasets sharing a temporal axis use that axis's same scale and
  parsing options; chart type does not change timestamp interpretation.
- Duplicate timestamps remain duplicate points at the same coordinate. The
  input dataset order is preserved, so line segments follow the same order as
  before. Timeseries spacing is based on the sorted unique timestamp domain.

## Scale and tick semantics

### `time`

The coordinate is proportional to elapsed milliseconds between the temporal
domain minimum and maximum. Irregular gaps remain irregular. If the domain has
one unique timestamp, it is placed at the center of the plot area.

### `timeseries`

Collect all non-null timestamps used by that axis, sort and deduplicate them,
and map the first and last timestamp to the scale endpoints with equal spacing
between adjacent unique values. Repeated timestamps map to the same position.
Generated tick values are mapped through piecewise-linear interpolation of
that lookup table. A one-value domain is centered.

### Ticks

Automatic ticks use UTC-aligned calendar boundaries. Tick selection aims for
the existing temporal spacing target of approximately one tick per 40 plot
pixels, respects `minUnit` and `unit`, and remains bounded by the existing
maximum tick count. Calendar advancement handles month, quarter, and year
boundaries rather than approximating them as fixed millisecond durations.
Weeks start on Sunday, matching the repository's existing temporal tick
generator. Quarters start in January, April, July, and October. Existing
supported tick controls such as maximum tick count continue to apply.

Generated labels use `time.displayFormats[unit]` when supplied, otherwise
stable built-in UTC labels chosen for the selected unit and range. The same
label and position data feed SVG, raster, and model output.

## IR and layout changes

- Extend axis scale IR with `Time` and `Timeseries` modes and a typed
  `TimeOptions` value; keep the options axis-local.
- Generalize the current `XPositions::Temporal` representation to temporal
  positions on either shared index axis. Validate lengths, timestamp range,
  and option invariants at the normal guard boundary.
- Keep value-axis coordinates in the existing per-dataset values/points and
  make the temporal mapper consume them when that axis is temporal.
- Add a common temporal scale mapper that calculates domain, forward
  projection, inverse tick positions, and generated labels. Use it from line,
  bar, mixed, scatter, and bubble layouts.
- Preserve existing grouped-bar width controls. For a temporal index axis,
  derive the minimum positive sample spacing from neighboring distinct
  timestamps, then apply the existing per-dataset bar width controls and
  limits.
- Extend `temporal.rs` with quarter boundaries, constrained unit selection,
  configured rounding, and the supported parser/formatter directives. Reuse
  its existing bounded tick generation and error-fragment handling.
- Keep the current Vega-Lite temporal line behavior as the default temporal
  configuration; Chart.js options override it only when explicitly supplied.

## Validation and compatibility

The Chart.js schema and runtime key validation will expose exactly the
supported axis/time fields. The frontend must reject unsupported temporal
options regardless of whether an axis is x or y. Schema generation must show
the same accepted enum and object keys as runtime parsing.

Focused tests will cover the parser formats and errors, epoch milliseconds,
UTC offsets and dates before 1970, rounding, unit/minUnit constraints, quarter
ticks, bounded tick output, elapsed versus equal spacing, x/y orientation,
null gaps, duplicate timestamps, grouped temporal bars, scatter/bubble points,
mixed charts, and unsupported settings. Regression tests will confirm that
Chart.js inputs with no temporal scale retain current SVG/model output.

The implementation plan will sequence these tests before behavior changes and
include schema/example documentation updates, full workspace tests, and the
existing native/WASM quality gates applicable to this repository.
