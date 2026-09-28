# Vega-Lite Trail Mark Design

Status: Approved for implementation
Beads issue: `fulgur-chart-gro`

## Goal

Support Vega-Lite `mark: "trail"` for categorical and temporal line data. A trail is a line whose width varies along its path according to `encoding.size`. Existing line rendering, input behavior, and line mark defaults remain unchanged.

Vega-Lite describes trail as a variable-width line mark whose size channel controls width. Its default continuous width range is 1–4 pixels (`minStrokeWidth` to `maxStrokeWidth`). See the [trail mark documentation](https://vega.github.io/vega-lite/docs/trail.html) and [scale documentation](https://vega.github.io/vega-lite/docs/scale.html).

## Supported Input

Accept either `"mark": "trail"` or `"mark": {"type": "trail"}`. The supported encoding channels are:

- `x`: categorical or temporal, using the same supported channel forms as `mark: "line"`.
- `y`: quantitative, using the same validation and aggregation behavior as `mark: "line"`.
- `color`: optional categorical grouping, using the existing line palette and ordering.
- `size`: optional quantitative field. Omitted or `null` means no size encoding. In an object, only `field` and `type` are accepted; `type` may be omitted or set to `"quantitative"`, and all referenced values must be finite numbers.

When `size` is omitted, this implementation uses a uniform width of 1 pixel. When present, values from all color groups are mapped together, linearly to the default 1–4 pixel range using the minimum and maximum of the size values after the line-compatible aggregation described below. A constant size domain maps to the midpoint, 2.5 pixels. Compute this mapping so all finite inputs, including domains whose subtraction would overflow, still produce finite widths in the 1–4 pixel range. Negative finite values are valid inputs because the size channel is mapped as a quantitative scale rather than treated as a literal width.

For repeated categorical `(x, color)` or temporal `(timestamp, color)` keys, preserve the existing line aggregation: sum `y` values and sum `size` values independently before building the aligned series. Reject a non-finite aggregate. This retains the current one-point-per-key line model and gives every aggregated point a single corresponding width.

The mark object accepts only `type`; interpolation and point-overlay options are unsupported. If supplied, they produce an explicit error in strict and non-strict parsing. Reject any other trail mark property or size-channel property beyond the accepted `field` and `type` in both modes so the parser never silently ignores trail options. Do not support `mark.point`, `mark.interpolate`, stacking, or additional size scale options in this issue. `data.url` remains unsupported as for the other Vega-Lite marks.

## Internal Model and Parsing

- Add `ChartKind::Trail` and route it through the shared Cartesian line layout while reporting `"trail"` from the semantic model.
- Add a per-point trail-width vector to `Series`. Its entries align with the series' category or temporal domain and remain empty for non-trail series.
- Add trail string/object types and a trail spec variant to the generated Vega-Lite schema. Include `encoding.size` for categorical and temporal trail encodings.
- Extend strict parsing so trail has an explicit mark and channel allow-list. Keep the generated schema and strict-parser acceptance aligned; non-strict parsing must still reject malformed or unsupported trail options rather than silently dropping them.
- Reuse line's category discovery, temporal parsing, color grouping, title/axis resolution, missing-data and duplicate-key behavior. Keep the existing `mark: "line"` result byte-for-byte unchanged for inputs that do not use trail.
- Validate `encoding.size.field`, optional `type`, record values, and the finiteness of aggregated values before storing widths. Guard the width vector with the same series/domain limits used for line data.

## Rendering

Keep trail out of the constant-width stroke path. For each gap-separated segment that remains after existing line decimation, construct one closed outline path whose left and right edges are offset from the centerline by half of the interpolated point width. Use the existing line coordinate mapping and clip the filled shape to the plot rectangle.

Use the series fill color, including the existing palette selection for `encoding.color`, and do not add a stroke or point markers. Linear interpolation is used between adjacent data points. Segment endpoints use butt caps. At interior vertices, use a bounded miter join and fall back to a bevel where a stable miter cannot be formed; this keeps acute turns finite and prevents unbounded geometry. Ignore zero-length edges when deriving tangents, collapse consecutive duplicate center coordinates while retaining the widest width at that location, and emit no path if fewer than two distinct coordinates remain. A single-point segment produces no filled trail, matching the line renderer's lack of point markers by default.

Apply width lookup to the same retained point indices selected by line decimation, so a retained y value always keeps its corresponding width. Do not add a trail-specific decimation algorithm. Gaps continue to split the path, and clipped/out-of-range values follow the existing line plot clipping behavior.

## Documentation and Verification

- Add `trail` to the README's supported Vega-Lite marks and explain its optional quantitative size encoding and default 1–4 pixel range.
- Add categorical and temporal trail examples, including color grouping and a size field, with committed PNG goldens in the existing fixed golden registry. Extend its render helper to select the Vega-Lite parser for specs with a top-level `mark` and keep Chart.js parsing for the other fixtures.
- Add schema and parser tests for both mark forms, categorical and temporal x, color grouping, optional size, size type/value failures, unsupported mark options, duplicate-key aggregation, and non-finite aggregate rejection.
- Add rendering tests for per-point widths, constant-size mapping, finite mapping for extreme size domains, fills and palette colors, butt caps, joins at acute/repeated points, gaps, clipping, single-point segments, and width alignment after decimation.
- Run the relevant Rust test suite, golden verification, formatting, and Clippy before creating the PR; merge after required CI succeeds.

## Out of Scope

- Changing `mark: "line"` rendering or aggregation.
- Trail interpolation modes, point overlays, stacking, configurable min/max stroke widths, size scale domains, or custom scale ranges.
- URL data and transforms beyond the existing inline `data.values` subset.
