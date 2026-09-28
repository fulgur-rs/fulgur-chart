# Chart.js Title and Subtitle Design

**Date:** 2026-09-28

**Issue:** `fulgur-chart-6qq`

## Context

Chart.js input currently reduces `options.plugins.title` to a plain optional
string. The layout layer then draws that string with its generic title defaults.
It cannot retain alignment, side placement, color, font, padding, or `fullSize`,
and `options.plugins.subtitle` is not represented. Several Chart.js chart kinds
also have separate plugin allowlists and layout functions, so adding title
behavior in a single Cartesian layout would leave non-Cartesian charts behind.

The approved scope covers every Chart.js chart kind currently accepted by the
frontend, including Cartesian, radial, pie, matrix, progress, gauge, sparkline,
treemap, word cloud, and Sankey charts. Vega-Lite and native title behavior must
remain unchanged.

## Goals

1. Preserve independent Chart.js title and subtitle options in the IR.
2. Accept string and string-array text and render every line in input order.
3. Apply `align`, `position`, `color`, `font`, `padding`, and `fullSize` to both
   plugins.
4. Reserve layout space and render titles through one shared scene-building
   stage used by every Chart.js chart kind.
5. Keep the existing general `ChartSpec.title` path for Vega-Lite and native
   charts unchanged.
6. Keep SVG, PNG, and WASM output on the existing shared Rust rendering path.

## Non-goals

- Executing scriptable JavaScript values, callbacks, or HTML/CSS title content.
- Replacing chart-kind layout algorithms or changing legend/axis layout beyond
  making room for title boxes.
- Automatically wrapping a long string. A string is one line; an array is the
  explicit list of lines.
- Changing Vega-Lite or native title defaults, geometry, or output.

## Input contract

`options.plugins.title` and `options.plugins.subtitle` accept the same typed
configuration:

```json
{
  "display": true,
  "text": ["Quarterly revenue", "North America"],
  "align": "start",
  "position": "top",
  "color": "#334155",
  "font": {
    "size": 14,
    "family": "Inter, sans-serif",
    "weight": "600",
    "style": "normal",
    "lineHeight": 1.25
  },
  "padding": { "top": 8, "bottom": 4 },
  "fullSize": true
}
```

- `display` defaults to `false`. A disabled or absent plugin reserves no space.
- `text` is a string or an array of strings. A string is one line, including an
  empty string; an empty array has no text lines. If omitted while `display` is
  true, it behaves as an empty string and still reserves one line of height.
- `align` accepts `start`, `center`, or `end`; the default is `center`.
- `position` accepts `top`, `left`, `bottom`, or `right`; the default is `top`.
  Left and right text is rotated vertically. For these positions, `align`
  follows Chart.js start/end direction along the vertical axis.
- `color` is a color string. When omitted or not parseable by the existing
  color parser, the resolved theme text color is used.
- `font` accepts `size`, `family`, `weight`, `style`, and `lineHeight` from the
  existing common font contract. `size` must be finite and positive. Weight,
  style, family, and size are emitted through styled text. Raster output uses
  the font bytes selected by the renderer; `family` is retained for SVG output
  and does not select an additional raster font.
- `lineHeight` follows Chart.js font rules: numbers are size multipliers;
  strings may be `normal`, a multiplier, pixels (`px`), `em`, or percent. An
  invalid or non-positive value falls back to `1.2 × size`.
- `padding` accepts a nonnegative number, applied to top and bottom, or an
  object containing nonnegative `top` and `bottom` values. Unspecified sides
  are zero. This matches the title plugin's documented padding dimensions.
- `fullSize` defaults to `true`. It controls the title's alignment box: true
  aligns against the full output canvas; false aligns against the remaining
  chart viewport after title margins are reserved.

Title and subtitle are independent: either may be disabled or placed on any
side. When they share a side, the main title is the outer box and the subtitle
is nearer the chart viewport. Each box uses its own font, padding, alignment,
position, and color.

The typed Chart.js schema exposes these fields for every supported chart kind.
Strict parsing recognizes the same keys and enum values as the schema. Existing
unknown-key behavior remains in force for other options.

## IR and parsing

Keep `ChartSpec.title: Option<String>` as the legacy title channel used by
Vega-Lite and native frontends. Add two independent resolved Chart.js values,
`chartjs_title` and `chartjs_subtitle`, each holding:

- `display` and ordered text lines;
- typed alignment and side position;
- resolved color;
- font size, optional family, weight, style, and resolved line height;
- top/bottom padding; and
- `full_size`.

The Chart.js parser must stop copying plugin title text into the legacy
`ChartSpec.title` field. It fills the two new fields instead. Shared schema
types and every per-kind `plugins` schema must expose both keys, including
chart kinds whose current schema does not expose title. The generic Chart.js
plugin parser and special chart parsers must resolve the same defaults and
preserve the same text shape.

Defaults follow the Chart.js title and subtitle plugins: title weight is bold
with padding 10; subtitle weight is normal with padding 0. Both use the theme
font size (12 px unless overridden), theme text color, normal style, a line
height of 1.2 times font size, centered alignment, top position, and
`fullSize: true`. Both default to hidden. The existing common renderer font
family is used when no family is supplied.

## Shared scene layout

All Chart.js kinds already pass through `layout::build_scene`. Add one shared
title stage around the existing kind dispatch:

1. Resolve visible title and subtitle boxes and calculate each box's thickness
   from its line count, resolved line height, and padding.
2. Sum thickness for boxes sharing a side and derive the chart viewport insets.
   With fixed canvas sizing, these insets reduce the available viewport. With
   plot-area sizing, the output scene grows by the title insets while retaining
   the chart viewport size.
3. Build the chart-kind scene using the reduced viewport dimensions, with the
   existing legacy title field left untouched for non-Chart.js frontends.
4. Compose the chart scene at the inset origin and add title/subtitle styled
   text in the reserved boxes. The composed scene keeps the requested outer
   dimensions and the original item order inside the chart layer.
5. Insert any theme background over the final outer dimensions, as today.

The composition representation must be renderer-neutral and preserve all
existing primitives, including paths, clipping, gradients, and optimized
circle rendering. SVG, direct raster rendering, model inspection, and bounds
validation must observe the same translated chart geometry. No per-kind title
geometry remains in the Chart.js path; non-Chart.js legacy title geometry stays
as it is.

If title boxes consume all available space, the chart viewport is clamped to
zero width or height. The scene remains bounded and valid, and text is clipped
to its reserved box/canvas rather than producing invalid coordinates.

## Validation and compatibility

Regression coverage will include:

1. Schema and parser acceptance for both plugins, string/array text, every
   supported title field, and every Chart.js chart-kind plugin allowlist.
2. Defaults, display false, empty text, independent title/subtitle settings,
   enum validation, and numeric/object padding.
3. Shared layout for top, bottom, left, and right positions; start/center/end
   alignment; multiline sizing; distinct title/subtitle font, color, and
   padding; and `fullSize` alignment behavior.
4. At least one Cartesian and one non-Cartesian chart rendered through SVG and
   direct raster paths, plus table-driven coverage proving all Chart.js kinds
   reserve title space and retain their chart content order.
5. Existing Vega-Lite and native title snapshots/goldens remain unchanged.

The implementation plan will sequence parser/schema tests, IR mapping, shared
scene composition, renderer updates, and compatibility regression checks. It
will run the repository's relevant workspace and WASM quality gates before the
PR is opened.

## References

- Chart.js title options: https://www.chartjs.org/docs/latest/configuration/title.html
- Chart.js subtitle options: https://www.chartjs.org/docs/latest/configuration/subtitle.html
- Chart.js title plugin implementation: https://github.com/chartjs/Chart.js/blob/master/src/plugins/plugin.title.js
- Chart.js subtitle plugin implementation: https://github.com/chartjs/Chart.js/blob/master/src/plugins/plugin.subtitle.js
