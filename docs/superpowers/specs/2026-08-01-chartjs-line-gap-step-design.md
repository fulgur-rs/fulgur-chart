# Chart.js Line Gap and Step Design

## Scope

Implement Beads issue `fulgur-chart-nif` for root `type: "line"` charts. Mixed-chart
line datasets are excluded: they use a distinct schema and layout path and would otherwise
silently gain only partial support.

## Public input and IR

`schema::chartjs::LineDataset` accepts optional camel-case `spanGaps` and `stepped` fields.
`spanGaps` is boolean-only, matching the issue contract; numeric maximum-gap handling is out
of scope. `stepped` is an untagged value of `false`, `true`, `"before"`, `"after"`, or
`"middle"`; `true` maps to step-before and `false` means no step interpolation.

The frontend maps these into explicit IR fields: a boolean gap policy and an optional step
mode. The IR does not depend on the schema type. When step mode is present it takes precedence
over the existing tension/interpolation setting, as Chart.js specifies.

## Rendering

The existing valid-point scan remains the source of truth for null and non-finite values.
With the default or `spanGaps: false`, category-index discontinuities split a line and area
into independent segments. With `spanGaps: true`, all valid points form one segment, so a
null does not break the line or its fill.

For each adjacent pair, step-before emits the horizontal leg then the vertical leg at the
right point, step-after emits the vertical leg then the horizontal leg at the left point,
and step-middle changes level at the x midpoint. The same expanded points are used for both
the stroked polyline and the area polygon. Linear, Catmull-Rom, and monotone behavior remains
byte-for-byte unchanged when `stepped` is absent or false.

Existing decimation remains per segment. This preserves its current bounded-resource behavior;
when explicitly enabled with steps, the step shape is formed from the retained representative
points, just as the current renderer forms interpolation from decimated points.

## Tests

Add RED tests before production changes for:

1. schema round-trip acceptance of boolean and named stepped forms, and rejection of invalid
   step strings;
2. frontend mapping of `spanGaps` and the four stepped meanings into IR;
3. default gap splitting versus `spanGaps: true` joining in rendered primitives;
4. exact before, after, and middle step coordinates; and
5. stepped precedence over nonzero tension plus area-path consistency.

Run focused unit tests for schema, frontend, and line layout during development, then the
locked package test suite, formatting, Clippy, diff check, and changed-line coverage before
completion.
