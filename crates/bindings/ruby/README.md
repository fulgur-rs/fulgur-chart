# fulgur_chart (Ruby)

Ruby binding for [fulgur-chart](https://github.com/fulgur-rs/fulgur-chart) — render
chart.js v4 / Vega-Lite JSON specs to deterministic SVG/PNG via a Rust native extension
([magnus](https://github.com/matsadler/magnus) / [rb-sys](https://github.com/oxidize-rb/rb-sys)).

## Requirements

- Ruby >= 3.0
- A Rust toolchain (`cargo`) — the gem builds a native extension at install time.

## Build / test from source

```sh
cd crates/bindings/ruby
bundle install
bundle exec rake          # compile the native extension + run the test suite
```

`bundle exec rake` runs the `compile` task (which builds the Rust extension) followed by
the minitest suite.

## Usage

```ruby
require "fulgur_chart"

spec = <<~JSON
  {
    "type": "bar",
    "data": {
      "labels": ["a", "b", "c"],
      "datasets": [{ "data": [1, 3, 2] }]
    }
  }
JSON

# SVG (UTF-8 String)
svg = FulgurChart.render_svg(spec)
File.write("chart.svg", svg)

# PNG (binary / ASCII-8BIT String) — write with binwrite to avoid encoding mangling
png = FulgurChart.render_png(spec)
File.binwrite("chart.png", png)

# render_image: format-dispatched raster output (currently "png" only) — binary String
png2 = FulgurChart.render_image(spec, format: "png")

# JSON Schema for a DSL (compact JSON String)
chartjs_schema  = FulgurChart.schema("chartjs")
vegalite_schema = FulgurChart.schema("vegalite")

# Library version (String, e.g. "0.1.0")
FulgurChart.version
```

The DSL is auto-detected from the spec: a top-level `mark` key selects Vega-Lite, a
top-level `type` key selects chart.js. Pass `dsl:` to override detection.

### API

| Method | Returns |
| --- | --- |
| `FulgurChart.render_svg(spec_json, **opts)` | SVG `String` (UTF-8) |
| `FulgurChart.render_png(spec_json, **opts)` | PNG `String` (binary / ASCII-8BIT) |
| `FulgurChart.render_image(spec_json, format:, **opts)` | raster `String` (binary); `format:` is required, only `"png"` is supported |
| `FulgurChart.schema(dsl)` | JSON Schema `String` for `"chartjs"` or `"vegalite"` |
| `FulgurChart.version` | version `String` |

### RenderOptions

All render methods accept the following keyword options (all optional unless noted):

| Option | Type | Notes |
| --- | --- | --- |
| `width` | Float | Canvas width override (applied before input-limit validation). |
| `height` | Float | Canvas height override. |
| `scale` | Float | Raster scale factor; raster output only (ignored by `render_svg`). Default `1.0`. |
| `strict` | Bool | Reject unknown keys in the spec (raises `StrictError`). Default `false`. |
| `dsl` | `"chartjs"` \| `"vegalite"` | Override DSL auto-detection. |
| `font` | binary `String` | A TTF/OTF font to embed/use instead of the bundled default. |

### Errors

The error hierarchy lives in the native extension under the `Fulgur` module
(`FulgurChart` is an alias of `Fulgur`):

- `Fulgur::ParseError < StandardError` — invalid JSON, undetectable DSL, input-limit
  violations.
- `Fulgur::StrictError < Fulgur::ParseError` — unknown key encountered under `strict: true`.
- `Fulgur::RenderError < StandardError` — raster rendering failure.

Note the font-error asymmetry: an invalid font raises `Fulgur::ParseError` on the SVG path
(`render_svg`) but `Fulgur::RenderError` on the image path (`render_png` / `render_image`),
because the two outputs go through different render pipelines.

## Note: packaging

The published-gem story — source-installing the path-dependent Rust core and shipping
cross-platform prebuilt gems — is tracked as a follow-up. Today the gem builds against the
in-repo core via a Cargo path dependency, so it is intended for use from within this
repository (build from source as shown above).
