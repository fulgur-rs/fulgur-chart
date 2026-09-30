# fulgur-chart

[![CI](https://github.com/fulgur-rs/fulgur-chart/actions/workflows/ci.yml/badge.svg)](https://github.com/fulgur-rs/fulgur-chart/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/fulgur-rs/fulgur-chart/graph/badge.svg)](https://codecov.io/gh/fulgur-rs/fulgur-chart)
[![crates.io: fulgur-chart](https://img.shields.io/crates/v/fulgur-chart.svg?label=fulgur-chart)](https://crates.io/crates/fulgur-chart)
[![crates.io: fulgur-chart-cli](https://img.shields.io/crates/v/fulgur-chart-cli.svg?label=fulgur-chart-cli)](https://crates.io/crates/fulgur-chart-cli)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A CLI that generates static SVG / PNG charts from a chart.js v4–compatible JSON spec
(a side project of [Fulgur](https://github.com/fulgur-rs)).

<p align="center">
  <img src="https://raw.githubusercontent.com/fulgur-rs/fulgur-chart/main/docs/images/bar.svg" width="32%" alt="Bar chart">
  <img src="https://raw.githubusercontent.com/fulgur-rs/fulgur-chart/main/docs/images/line.svg" width="32%" alt="Line chart">
  <img src="https://raw.githubusercontent.com/fulgur-rs/fulgur-chart/main/docs/images/pie.svg" width="32%" alt="Pie chart">
</p>

## Why

Generates deterministic charts — byte-identical output for the same input — without
a browser or JavaScript. Combined with Fulgur, the resulting SVG can be embedded as
a vector graphic in a PDF. Re-generating reports in CI produces no diff, making it
easy to keep figures under version control.

## Installation

### npx (ゼロインストール)

```sh
npx @fulgur-rs/chart-cli render chart.json -o chart.svg
```

Node.js 18+ が必要。`npm install` 時（または `npx` 初回呼び出し時）に
対応プラットフォームのバイナリが optionalDependencies として自動選択される。  
`@fulgur-rs/chart-cli` が npm に未公開の場合は、下記の Cargo を使用してください。

### Cargo

```sh
cargo install fulgur-chart-cli
```

インストール後は `fulgur-chart` コマンドが使えるようになる。

### ソースからビルド (開発向け)

```sh
cargo install --path crates/fulgur-chart-cli
```

## Usage

Prepare a minimal chart.js spec (`chart.json`):

```json
{
  "type": "bar",
  "data": {
    "labels": ["Jan", "Feb", "Mar"],
    "datasets": [
      { "label": "Revenue (k$)", "data": [120, 200, 150], "backgroundColor": "#36a2eb" }
    ]
  },
  "options": {
    "plugins": { "title": { "display": true, "text": "Monthly Revenue" } }
  }
}
```

Generate SVG / PNG:

```sh
# SVG (default)
fulgur-chart render chart.json -o chart.svg

# PNG (--scale sets the resolution multiplier; 2 doubles the pixel dimensions)
fulgur-chart render chart.json -o chart.png --format png --scale 2
```

Use `-` for stdin / stdout piping:

```sh
cat chart.json | fulgur-chart render - -o - > chart.svg
```

Key options:

- `--format svg|png` — Output format. Inferred from the output extension (`.png` → png; otherwise / stdout → svg) when omitted.
- `--width <px>` / `--height <px>` — Override canvas dimensions (default 800 × 450).
- `--scale <factor>` — PNG resolution multiplier (default 1.0).
- `--font <path>` — Replace the font used for measurement, SVG, and PNG (default: bundled Noto Sans JP).
- `--out-dir <dir>` — Output directory for batch generation (see below).
- `--dsl chartjs|vegalite` — Input DSL. Auto-detected when omitted: a top-level `mark` key selects Vega-Lite; a top-level `type` key selects chart.js.
- `--strict` — Treat unknown / unsupported keys as errors (silently ignored by default).

```sh
# Override dimensions and detect unknown keys with --strict
fulgur-chart render chart.json -o chart.svg --width 1024 --height 576 --strict
```

### Batch generation

Render multiple specs at once (useful for generating report figures in CI).
Each input `X.json` is written to `<out-dir>/X.<ext>` (output is byte-identical per file).

```sh
fulgur-chart render specs/*.json --out-dir out/            # each → out/<name>.svg
fulgur-chart render specs/*.json --out-dir out/ --format png
```

### Other subcommands

```sh
# Print the JSON Schema for an input DSL (useful for validation tooling)
fulgur-chart schema chartjs
fulgur-chart schema vegalite

# Inspect the semantic model (IR + layout) for a spec — pretty JSON
fulgur-chart inspect chart.json
```

## Supported chart types

- Bar chart (vertical / horizontal; horizontal via `options.indexAxis: "y"`)
- Stacked bar chart (`stacked: true` on the index axis: `scales.x` for vertical, `scales.y` for horizontal)
- Line chart
- Area chart (`datasets[].fill: true` on a line dataset)
- Pie chart
- Doughnut chart
- Scatter plot (`{x, y}` point data)
- Bubble chart (`{x, y, r}` point data)
- Radar chart
- Mixed chart (per-dataset `type`, e.g. bar + line)
- Progress bar chart (QuickChart-style; horizontal fill bar with centered percentage)
- Matrix chart / heatmap (`{x, y, v}` point data; cells shaded by interpolating between two colors)
- Box plot chart (5-number summary: `type: "boxplot"`, `data` as nested arrays `[min, q1, median, q3, max]`)
- Violin charts (`type: "violin"` / `"horizontalViolin"`; each category contains a raw sample array, for example `[[2, 3, 4, 8], [1, 2, 5]]`)
- Gauge chart (QuickChart-style; semicircle with colored zones, needle, value label)
- Radial gauge chart (QuickChart-style; full circle fill-to-value with center value text)

## Supported chart.js subset

Supports a data-only, static subset:

- `type` — `bar` / `line` / `pie` / `doughnut` / `scatter` / `bubble` / `radar` / `matrix` / `treemap` / `boxplot` / `violin` / `horizontalViolin` / `progress` / `gauge` / `radialGauge` / `wordCloud` / `sankey` (QuickChart's `progressBar` is also accepted as an alias for `progress`)
- `data.labels`
- `data.datasets[]` — `label` / `data` (numeric array; `{x,y}` / `{x,y,r}` for scatter/bubble; `{x,y,v}` for matrix; nested `[min,q1,median,q3,max]` arrays for boxplot; nested raw sample arrays such as `[[2,3,4],[1,2,5]]` for violin). Temporal axes also accept ISO date strings and epoch milliseconds in index labels, value arrays, and scatter/bubble coordinates. Other dataset options include `backgroundColor` / `borderColor` / `borderWidth` / `fill` / `tension` / `pointRadius` / `type` (per-dataset type for mixed charts).
- For `progress` (alias `progressBar`), `datasets[0].data` holds each bar's value; an optional second dataset's `data` overrides the per-bar max (default 100). The percentage label is shown by default and can be hidden with `options.plugins.datalabels.display: false`.
- For `gauge`, `datasets[0].data` holds cumulative zone thresholds, `value` is the needle value, and `backgroundColor` is the per-zone colors (`minValue` sets the lower bound). Configure with `options.needle` / `options.valueLabel`. The value label falls back to the rounded value (JS `valueLabel.formatter` is not executed).
- For `radialGauge`, `datasets[0].data` holds a single value drawn as a fill-to-value arc on a track ring. Configure with `options.domain` / `options.trackColor` / `options.centerPercentage` / `options.roundedCorners` / `options.centerArea` (`displayText` / `fontSize`). The center value text falls back to the rounded value (JS `centerArea.text` is not executed).
- For `treemap`, `datasets[0].tree` holds the hierarchical data: either a flat numeric array, or an array of objects with `key` (the numeric property to sum — **required** for object trees) and `groups` (grouping property names, outermost first) defining the nesting levels. Cells are colored from the palette by depth; dataset-level `backgroundColor` / `borderColor` / `borderWidth` and `options.plugins.legend` are not used. `options.plugins.title` and `options.theme` apply.
- For `wordCloud`, `data.labels` holds the words and `datasets[0].data` holds the corresponding font sizes (numeric). `datasets[0].color` (string or string array) sets per-word fill colors. Configure rotation with `options.elements.word` (`minRotation` / `maxRotation` / `rotationSteps` / `padding`). Up to 500 words are rendered.
- For `sankey`, `datasets[0].data` holds the flow links as `{from, to, flow}` objects (node names are derived from `from` / `to`). Ribbon color is set by `colorMode`: `gradient` (default) blends `colorFrom` → `colorTo` along each link, while `from` / `to` paint a single solid color. Node columns are laid out left-to-right; tune with `nodeWidth` / `nodePadding`, `modeX` (column placement) and `size` (node height basis). Override per-node values with `labels` (display text), `priority` (vertical ordering) and `column` (forced column index). `options.plugins.title` and `options.theme` apply.
- `options.indexAxis`
- `options.plugins.title` / `options.plugins.legend` (`position`: top/bottom/left/right; `legend` does not apply to `gauge` / `radialGauge`)
- `options.plugins.datalabels` (`display` — renders a value label at each data point)
- `options.scales` (`stacked` — read from the index axis, matching chart.js; `suggestedMin` / `suggestedMax` and a subset of other options). Cartesian line, bar, mixed, scatter, and bubble charts accept `type: "time"` or `"timeseries"` independently on x/y. Temporal axes use UTC; `time.unit`, `minUnit`, `parser` (a bounded strftime subset), `round`, and per-unit `displayFormats` are supported.
- `options.theme` (extension; see below)

Dynamic JavaScript features (`callback` / `animation` / `interaction` / plugin scripts)
are not supported. **Unknown keys are silently ignored by default**; use `--strict` to
detect them as errors.

## Themes (`options.theme`)

chart.js v4 default colors and styles are used as a baseline. `options.theme` overrides
the appearance (this is an extension key not present in chart.js itself; omit it to use
the defaults).

- `palette` — Array of color strings for automatic dataset / slice coloring
- `gridColor` / `textColor` — Grid line color / text color
- `backgroundColor` — Canvas background (transparent by default)
- `fontSize` — Base font size for labels (px)

Colors accept `#rgb` / `#rrggbb` / `rgb()` / `rgba()` / `hsl()` / `hsla()` / CSS color names.

## Vega-Lite input (`--dsl vegalite`)

In addition to chart.js specs, a minimal Vega-Lite subset is accepted as input:

```sh
# Explicit
fulgur-chart render chart.vl.json -o chart.svg --dsl vegalite

# Auto-detected (top-level "mark" key selects Vega-Lite)
fulgur-chart render chart.vl.json -o chart.svg
```

Supported subset: `mark` (`bar` / `line` / `area` / `trail` / `point` → scatter / `circle` → scatter /
`square` → square scatter / `arc` → pie / `rect` → heatmap / `text` / `tick` / `image` / `geoshape` / `rule` / `errorbar` / `errorband`),
inline `data.values`, and `encoding` fields `x` / `y` / `color` / `theta` / `shape` / `size` / `opacity` / `url` / `text`;
`point` and `square` support quantitative `size` mapped to marker area, while `trail` uses quantitative `size`
for a variable line width (1–4 px by default, or a uniform 1 px when omitted). `area` stacks by default when `color` is present
(`encoding.y.stack: null` to disable), matching Vega-Lite. Geoshape accepts Feature arrays and
single Feature/FeatureCollection values directly; ordinary records put a GeoJSON Geometry,
Feature, or FeatureCollection in `encoding.shape.field`. It supports all 16 Vega-Lite v6
projections with automatic fitting. URL data and TopoJSON are not supported for geoshape. The Tableau10 color
palette is applied automatically to categorical Vega-Lite encodings. Input is converted to a
shared intermediate representation, so output determinism and Fulgur integration are identical
to chart.js input.

### Layer and concat composition

`layer`, `hconcat`, and `vconcat` can combine nested views while keeping the order of marks in
the input. Layer nodes inherit inline `data.values` and `encoding` by channel; child data replaces
the inherited rows, and a child channel replaces only that channel's inherited mapping. Concat
nodes inherit data but cannot declare a shared encoding.

Layers share positional and non-positional scales and guides by default. Concat views use
independent `x` and `y` scales and axes by default, with shared color/size scales and legends.
`resolve` can select supported shared or independent scale, axis, and legend behavior; incompatible
shared domains and unsupported resolution combinations return path-qualified errors. URL data,
`transform`, `facet`, `repeat`, and general `concat` are rejected. `arc` and `geoshape` can be used
in concat views, but cannot share a Cartesian layer frame. Composition uses the same Scene renderer
in native and WASM builds. SVG keeps image references; PNG and WebP report the existing image-mark
unsupported error if any composed view contains an image.

Examples: [bar and line layer](examples/specs/vegalite-layer.json) and
[nested concat with an inner layer](examples/specs/vegalite-nested-concat.json).

### Text marks

Text marks accept inline `data.values` with quantitative `x` and `y`, either `encoding.text.field`,
`encoding.text.value`, or a constant `mark.text`. They render as standalone charts and as leaves in
Cartesian `layer` views. `encoding.color` accepts a nominal field or constant value; `encoding.size`
and `encoding.opacity` accept quantitative fields or constant values. Quantitative size fields map
font size to 8–40 px and opacity fields map to 0.3–0.8. Mark properties include `font`, `fontSize`,
`fontWeight`, `fontStyle`, `align`, `baseline`, `angle`, `dx`, and `dy`.
Raster output uses the supplied font face for text marks. An explicit `mark.font` must match that
font's family; the bundled Noto Sans JP font also accepts `sans-serif`. Incompatible families
return an error instead of being silently ignored. SVG output retains the requested CSS family.

Categorical or temporal positions, custom text formats, conditions, multiline labels, truncation,
URL data, and transforms are rejected. Native and WASM use the same Scene for SVG and PNG output.
See the [text mark example](examples/specs/vegalite_text.json).

### Tick marks

`tick` accepts inline `data.values` and draws one short rectangle per record. Use `orient: "horizontal"`
or `"vertical"`, and encode at least one of `x` or `y` as quantitative, temporal, nominal, or ordinal;
an omitted position is centered in the plot. Mark and encoding color, size, and opacity are supported.
Quantitative size fields map to the tick size range, and opacity fields map to 0.3–0.8 by default.
`config.tick.bandSize` sets the default length (otherwise 3/4 of the oriented discrete step), and
`config.tick.thickness` defaults to 1 px. Inline values are required; URL data, transforms, aggregation,
binning, and unsupported channels are rejected. Tick marks use the shared Scene in native and WASM
rendering. See the [tick mark example](examples/specs/vegalite-tick.json).

### Image marks

`image` requires a mark object with positive `width` and `height`, quantitative `encoding.x` and
`encoding.y`, and either a field or value in `encoding.url`. URLs must use
`http`, `https`, or `data:image`. The core keeps each URL as an SVG `<image>` reference and does
not fetch it. SVG output preserves the reference; PNG and WebP rendering return an unsupported
format error for image marks. See the [image fixture](examples/specs/vegalite-image.json).

### Rule marks

`rule` accepts inline `data.values`. A lone `encoding.x` or `encoding.y`, without its matching
secondary channel, draws a plot-spanning vertical or horizontal line. A ranged rule can use `x` / `x2`
without `y`, or `y` / `y2` without `x`; its missing orthogonal position is placed at the plot center.
When both `x` and `y` are supplied, omitted `x2` or `y2` endpoints collapse to their primary
position. Positions can be categorical, quantitative, or temporal.
`mark.color`, `mark.opacity`, `mark.strokeWidth`, `mark.strokeDash`, and `mark.clip`, plus a
categorical `encoding.color`, are supported. Transformations and aggregation are rejected. Rule
marks reject category values of different JSON types that stringify to the same label (for example,
numeric `1` and string `"1"`), including shared composition scales where a rule participates. They
can appear in Cartesian layers and share the layer's scales. See the
[rule mark example](examples/specs/vegalite-rule.json).

### Error bars and bands

`errorbar` and `errorband` accept a string mark or an object with `type`, and require inline
`data.values` records.

- **Raw samples:** use one quantitative `x` or `y` field as the measured value. `mark.extent` accepts
  `stderr` (default; sample standard deviation divided by √n), `stdev` (sample standard deviation),
  `ci` (a 95% bootstrap interval), or `iqr` (type-7 first and third quartiles). The other positional
  channel is optional and supplies the independent coordinate; set `mark.orient` to `horizontal` or
  `vertical` if both axes are quantitative and the measured axis is ambiguous.
- **Pre-aggregated ranges:** use `encoding.x` with `x2` or `encoding.y` with `y2` for lower and upper
  endpoints. Alternatively, use `x` / `y` as the center with `xError` / `xError2` or
  `yError` / `yError2` as offsets; a missing `*Error2` channel makes the range symmetric. Raw and
  pre-aggregated inputs cannot be mixed.
- **Channels and styles:** categorical field-based `encoding.color` and `encoding.detail` split
  series; `encoding.color.value` or `mark.color` sets a constant color. Constant opacity is available
  through `mark.opacity` or `encoding.opacity.value`. `errorbar` draws `rule` by default and adds endpoint
  `ticks` when requested. `errorband` fills `band` by default (opacity 0.3) and draws `borders` when
  requested. Style objects accept `color`, `fill`, `stroke`, `opacity`, `strokeWidth`, and
  `strokeDash`; `rule` and `ticks` also accept `size`. Both marks support `clip` (default `true`);
  error bands also support `interpolate` and `tension`.
- **Renderers:** native SVG and PNG and the WASM binding use the same rendering scene.

Examples: [raw errorbar](examples/specs/vegalite-errorbar-raw.json),
[pre-aggregated errorbar](examples/specs/vegalite-errorbar-preaggregated.json),
[raw errorband](examples/specs/vegalite-errorband-raw.json), and
[pre-aggregated errorband](examples/specs/vegalite-errorband-preaggregated.json).
URL data, transforms, and interactive tooltips or selections are not supported.

For example, a quantitative choropleth can provide GeoJSON features directly in `data.values`:

```json
{
  "mark": "geoshape",
  "data": {
    "values": {
      "type": "FeatureCollection",
      "features": [
        {
          "type": "Feature",
          "properties": { "density": 24 },
          "geometry": {
            "type": "Polygon",
            "coordinates": [[[0, 0], [8, 0], [8, 5], [0, 5], [0, 0]]]
          }
        }
      ]
    }
  },
  "encoding": { "color": { "field": "density", "type": "quantitative" } }
}
```

The [geoshape fixture](examples/specs/vegalite_geoshape.json) demonstrates a choropleth with a
polygon hole and native/WASM rendering coverage.

Render the categorical trail example with the CLI:

```sh
fulgur-chart render examples/specs/vegalite-trail-categorical.json -o trail.svg --dsl vegalite
```

## Ruby binding

An in-repository Ruby gem (`crates/bindings/ruby`) wraps the same rendering core via a
Rust native extension (magnus / rb-sys). It is build-from-source only (not yet published
to RubyGems) and requires a Rust toolchain.

```sh
cd crates/bindings/ruby
bundle install && bundle exec rake   # compile extension + run tests
```

```ruby
require "fulgur_chart"
svg = FulgurChart.build(spec_json).width(800).height(450).render(:svg)
png = FulgurChart.build(spec_json).scale(2.0).render(:png)
```

See [`crates/bindings/ruby/README.md`](crates/bindings/ruby/README.md) for the full API reference.

## Fulgur integration

Embed the generated SVG in HTML with `<img>` and render to PDF with Fulgur:

```html
<img src="out/bar.svg" alt="Monthly Revenue">
```

```sh
fulgur render -o report.pdf report.html
```

See [`examples/report.html`](examples/report.html) for a minimal example.
Bundling the same Noto Sans JP font on the Fulgur side ensures chart text glyphs match.

## Determinism

The same input spec always produces byte-identical output. Only the bundled Noto Sans JP
font is used; system fonts are never loaded.

## Roadmap

The following are not yet implemented (candidates for future support):

- Value labels on radar chart axes; data labels on scatter / radar
- Dual-axis mixed charts (separate left/right y-scales); mixing with horizontal / stacked bars
- Vega-Lite URL data, `transform`, and `aggregate` (currently inline `data.values` only)
- Font subsetting (binary size reduction)

## License

Code is dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).

The bundled Noto Sans JP font is distributed under the
[SIL Open Font License 1.1](crates/fulgur-chart/assets/fonts/LICENSE-NotoSansJP.txt)
and is included as-is from the upstream [notofonts / noto-cjk](https://github.com/notofonts/noto-cjk)
distribution.
