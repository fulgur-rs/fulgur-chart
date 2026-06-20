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

The API is a fluent **builder**: `FulgurChart.build(spec)` returns a builder you configure with
chainable setters and finish with `render(:svg)` / `render(:png)`.

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
svg = FulgurChart.build(spec).render(:svg)
File.write("chart.svg", svg)

# PNG (binary / ASCII-8BIT String) — write with binwrite to avoid encoding mangling
png = FulgurChart.build(spec).width(800).height(600).scale(2.0).render(:png)
File.binwrite("chart.png", png)

# Set a default format with .format, then call render with no argument
png2 = FulgurChart.build(spec).format(:png).render

# The builder is reusable and reconfigurable between renders
chart = FulgurChart.build(spec).dsl(:chartjs)
a = chart.width(400).render(:svg)
b = chart.width(1234).render(:svg)

# JSON Schema for a DSL (compact JSON String)
chartjs_schema  = FulgurChart.schema(:chartjs)
vegalite_schema = FulgurChart.schema("vegalite")

# Library version (String, e.g. "0.1.0")
FulgurChart.version
```

The DSL is auto-detected from the spec: a top-level `mark` key selects Vega-Lite, a
top-level `type` key selects chart.js. Use `.dsl(:chartjs)` / `.dsl(:vegalite)` to override.
Options accept either a Symbol or a String (`.dsl(:chartjs)` == `.dsl("chartjs")`).

### API

| Method | Returns |
| --- | --- |
| `FulgurChart.build(spec_json)` | a `FulgurChart::Builder` |
| `builder.render(format = nil)` | `String` — `:svg` → UTF-8, `:png` → binary (ASCII-8BIT). Format precedence: argument > `.format()` > default `:svg` |
| `FulgurChart.render(spec_json, format, **opts)` | low-level primitive the builder calls; same return contract |
| `FulgurChart.schema(dsl)` | JSON Schema `String` for `:chartjs` or `:vegalite` |
| `FulgurChart.version` | version `String` |

### Builder setters

Each setter returns the builder for chaining; all are optional.

| Setter | Type | Notes |
| --- | --- | --- |
| `.width(w)` / `.height(h)` | Float | Canvas size override (applied before input-limit validation). |
| `.scale(s)` | Float | Raster scale factor; raster output only (ignored when rendering `:svg`). Default `1.0`. |
| `.strict` / `.strict(bool)` | Bool | Reject unknown keys (raises `StrictError`). `.strict` ⇒ `true`. Default `false`. |
| `.dsl(d)` | `:chartjs` \| `:vegalite` (Symbol/String) | Override DSL auto-detection. |
| `.font(bytes)` | binary `String` | A TTF/OTF font to use instead of the bundled default (Noto Sans JP). |
| `.format(f)` | `:svg` \| `:png` (Symbol/String) | Default format for a terminal `render` with no argument. |

### Errors

The error hierarchy lives in the native extension under the `FulgurChart` module
(the module is `FulgurChart`, not `Fulgur`, to avoid a top-level collision with the
Fulgur PDF library when both gems are loaded in the same process):

- `FulgurChart::ParseError < StandardError` — invalid JSON, undetectable DSL, unknown format,
  input-limit violations.
- `FulgurChart::StrictError < FulgurChart::ParseError` — unknown key encountered under `.strict`.
- `FulgurChart::RenderError < StandardError` — raster rendering failure.

Note the font-error asymmetry: an invalid font raises `FulgurChart::ParseError` when rendering
`:svg` but `FulgurChart::RenderError` when rendering `:png`, because the two outputs go through
different render pipelines.

## Note: packaging

The published-gem story — source-installing the path-dependent Rust core and shipping
cross-platform prebuilt gems — is tracked as a follow-up. Today the gem builds against the
in-repo core via a Cargo path dependency, so it is intended for use from within this
repository (build from source as shown above).
