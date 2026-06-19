# examples

Sample specs for fulgur-chart, the SVGs rendered from them by the real CLI, and a
report HTML intended for Fulgur. These cover every supported chart type and the main
features.

## Directory

- `specs/` … input specs (JSON). chart.js v4-compatible by default; only `vegalite.json` is Vega-Lite.
  - Chart types:
    - `bar.json` … bar chart (vertical, monthly revenue)
    - `bar-horizontal.json` … horizontal bar (`indexAxis: "y"`)
    - `stacked-bar.json` … stacked bar (`scales.y.stacked`)
    - `line.json` … line chart (two series, smoothed with `tension`)
    - `area.json` … area chart (line with `"fill": true`)
    - `pie.json` … pie chart (auto-colored per slice)
    - `doughnut.json` … doughnut chart (legend on the right)
    - `scatter.json` … scatter plot (`{x, y}` point data, two series)
    - `bubble.json` … bubble chart (`{x, y, r}` — radius as a third dimension)
    - `radar.json` … radar chart (multivariate, two series)
    - `mixed.json` … mixed chart (bar + line via per-dataset `type`)
  - Features:
    - `datalabels.json` … data labels (`plugins.datalabels.display`)
    - `theme.json` … theme override (`options.theme`, dark palette)
    - `vegalite.json` … Vega-Lite subset input (`--dsl vegalite`)
- `out/` … the SVGs rendered from those specs by the CLI (committed)
- `report.html` … a minimal gallery embedding the generated SVGs with `<img>`

## Regenerating the SVGs

From the repository root, run each spec through the CLI to regenerate `out/`. Output is
deterministic (byte-identical for identical input), so regenerating produces no diff.

The chart.js specs (everything except `vegalite`) can be generated together:

```sh
for n in bar bar-horizontal stacked-bar line area pie doughnut \
         scatter bubble radar mixed datalabels theme; do
  cargo run -q -p fulgur-chart-cli -- render "examples/specs/$n.json" -o "examples/out/$n.svg"
done
```

Vega-Lite input needs `--dsl vegalite`:

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite.json -o examples/out/vegalite.svg --dsl vegalite
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
