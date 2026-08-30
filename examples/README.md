# examples

Sample specs for fulgur-chart, the SVGs rendered from them by the real CLI, and a
report HTML intended for Fulgur. These cover every supported chart type and the main
features.

## Directory

- `specs/` … input specs (JSON). chart.js v4-compatible by default; the `vegalite*.json`
  files are Vega-Lite (`--dsl vegalite`).
  - Chart types:
    - `bar.json` … bar chart (vertical, monthly revenue)
    - `bar-horizontal.json` … horizontal bar (`indexAxis: "y"`)
    - `stacked-bar.json` … stacked bar (`scales.x.stacked`; stacking follows the index axis)
    - `line.json` … line chart (two series, smoothed with `tension`)
    - `area.json` … area chart (line with `"fill": true`)
    - `pie.json` … pie chart (auto-colored per slice)
    - `doughnut.json` … doughnut chart (legend on the right)
    - `scatter.json` … scatter plot (`{x, y}` point data, two series)
    - `bubble.json` … bubble chart (`{x, y, r}` — radius as a third dimension)
    - `radar.json` … radar chart (multivariate, two series)
    - `mixed.json` … mixed chart (bar + line via per-dataset `type`)
    - `matrix.json` … matrix (heatmap) chart (`{x, y, v}` point data, color gradient)
  - Features:
    - `datalabels.json` … data labels (`plugins.datalabels.display`)
    - `theme.json` … theme override (`options.theme`, dark palette)
  - Vega-Lite subset input (`--dsl vegalite`):
    - `vegalite.json` … `mark: "bar"`, grouped by `color`
    - `vegalite-line.json` … `mark: "line"`, multi-series by `color`
    - `vegalite-temporal-line.json` … `mark: "line"` with `x.type: "temporal"`
    - `vegalite-area-stacked.json` … `mark: "area"`, stacked by `color`
    - `vegalite-scatter.json` … `mark: "point"` → scatter, grouped by `color`
    - `vegalite-arc.json` … `mark: "arc"` → pie, via `theta`/`color`
    - `vegalite-rect-heatmap.json` … `mark: "rect"` → heatmap, via `x`/`y`/`color`
  - Jsonnet input (`.jsonnet` files are evaluated before parsing):
    - `bar.jsonnet` … bar chart using `local` variables and comments
    - `line-generated.jsonnet` … sine wave generated with `std.range` / `std.map`
- `out/` … the SVGs rendered from most of those specs by the CLI (committed); a couple of
  the newer Vega-Lite specs (`vegalite-temporal-line.json`, `vegalite-rect-heatmap.json`)
  don't have a corresponding `out/` file yet
- `report.html` … a minimal gallery embedding the generated SVGs with `<img>`

## Regenerating the SVGs

From the repository root, run each spec through the CLI to regenerate `out/`. Output is
deterministic (byte-identical for identical input), so regenerating produces no diff.

The chart.js specs (everything except `vegalite`) can be generated together:

```sh
for n in bar bar-horizontal stacked-bar line area pie doughnut \
         scatter bubble radar mixed matrix datalabels theme; do
  cargo run -q -p fulgur-chart-cli -- render "examples/specs/$n.json" -o "examples/out/$n.svg"
done
```

Jsonnet specs are detected by `.jsonnet` extension and evaluated automatically:

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.jsonnet -o examples/out/bar.jsonnet.svg
cargo run -q -p fulgur-chart-cli -- render examples/specs/line-generated.jsonnet -o examples/out/line-generated.svg
```

Vega-Lite input needs `--dsl vegalite`:

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite.json -o examples/out/vegalite.svg --dsl vegalite
cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite-area-stacked.json -o examples/out/vegalite-area-stacked.svg --dsl vegalite
```

For PNG, add `--format png` (optionally `--scale 2` for a resolution multiplier):

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json -o examples/out/bar.png --format png --scale 2
```

To render several specs at once, use `--out-dir` for batch generation:

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json examples/specs/pie.json --out-dir examples/out/
```

## Rendering to PDF with Fulgur

`report.html` is plain HTML that references the generated SVGs by relative path. Run it
through [Fulgur](https://github.com/fulgur-rs) and the SVGs are embedded into the PDF as
vectors.

```sh
fulgur render -o report.pdf report.html
```

Bundling the same Noto Sans JP on the Fulgur side keeps the glyph shapes of in-chart text
consistent.
