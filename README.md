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
- Gauge chart (QuickChart-style; semicircle with colored zones, needle, value label)
- Radial gauge chart (QuickChart-style; full circle fill-to-value with center value text)

## Supported chart.js subset

Supports a data-only, static subset:

- `type` — `bar` / `line` / `pie` / `doughnut` / `scatter` / `bubble` / `radar` / `matrix` / `treemap` / `boxplot` / `progress` / `gauge` / `radialGauge` (QuickChart's `progressBar` is also accepted as an alias)
- `data.labels`
- `data.datasets[]` — `label` / `data` (numeric array; `{x,y}` / `{x,y,r}` for scatter/bubble; `{x,y,v}` for matrix; nested `[min,q1,median,q3,max]` arrays for boxplot) / `backgroundColor` / `borderColor` / `borderWidth` / `fill` / `tension` / `pointRadius` / `type` (per-dataset type for mixed charts)
- For `progress` (alias `progressBar`), `datasets[0].data` holds each bar's value; an optional second dataset's `data` overrides the per-bar max (default 100). The percentage label is shown by default and can be hidden with `options.plugins.datalabels.display: false`.
- For `gauge`, `datasets[0].data` holds cumulative zone thresholds, `value` is the needle value, and `backgroundColor` is the per-zone colors (`minValue` sets the lower bound). Configure with `options.needle` / `options.valueLabel`. The value label falls back to the rounded value (JS `valueLabel.formatter` is not executed).
- For `radialGauge`, `datasets[0].data` holds a single value drawn as a fill-to-value arc on a track ring. Configure with `options.domain` / `options.trackColor` / `options.centerPercentage` / `options.roundedCorners` / `options.centerArea` (`displayText` / `fontSize`). The center value text falls back to the rounded value (JS `centerArea.text` is not executed).
- For `treemap`, `datasets[0].tree` holds the hierarchical data: either a flat numeric array, or an array of objects with `key` (the numeric property to sum — **required** for object trees) and `groups` (grouping property names, outermost first) defining the nesting levels. Cells are colored from the palette by depth; dataset-level `backgroundColor` / `borderColor` / `borderWidth` and `options.plugins.legend` are not used. `options.plugins.title` and `options.theme` apply.
- `options.indexAxis`
- `options.plugins.title` / `options.plugins.legend` (`position`: top/bottom/left/right; `legend` does not apply to `gauge` / `radialGauge`)
- `options.plugins.datalabels` (`display` — renders a value label at each data point)
- `options.scales` (`stacked` — read from the index axis, matching chart.js; `suggestedMin` / `suggestedMax` and a subset of other options)
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

Supported subset: `mark` (`bar` / `line` / `point` → scatter / `arc` → pie), inline
`data.values`, and `encoding` fields `x` / `y` / `color` / `theta`. The Tableau10 color
palette is applied automatically to Vega-Lite specs. Input is converted to a shared
intermediate representation, so output determinism and Fulgur integration are identical
to chart.js input.

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
