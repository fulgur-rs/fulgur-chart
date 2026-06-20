# Changelog

このプロジェクトの主な変更点を記録します。
フォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に従い、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [Unreleased]

## [0.1.1](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.0...fulgur-chart-cli-v0.1.1) - 2026-06-20

### Added

- *(progress)* add Progress variant to JSON schema
- auto-detect DSL when --dsl is omitted
- add detect_dsl for DSL auto-detection

### Fixed

- address review feedback on inspect model + compat tooling
- *(progress)* expose progressBar alias in JSON schema
- *(progress)* address AI review feedback
- suppress dead_code warning on detect_dsl (wired in next task)

### Other

- Merge remote-tracking branch 'origin/main' into feat/chartjs-compat-uob
- *(progress)* add example spec, README and CHANGELOG entries
- bypass SVG string for PNG rendering via direct tiny-skia scene renderer
- add readme and documentation fields to Cargo.toml
- add missing doc comments to bring coverage above 80%
- use IgnoredAny in detect_dsl to avoid full JSON allocation
- cargo fmt
- CLI integration tests for DSL auto-detection
- cargo fmt

### Security

- add InputLimits struct and series×categories product check
- add input limits to prevent DoS from untrusted specs

## [0.2.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.1.0...fulgur-chart-v0.2.0) - 2026-06-20

### Added

- *(compat)* address review feedback — cartesian axes, role-aware cross-check, error handling
- *(compat)* semantic diff engine + cross-language rgba fixture
- *(model)* attach linear/category axes for cartesian charts
- *(model)* build meta/series/counts from IR
- *(model)* add chart model module with rgba normalization
- add palette_background_color with alpha=0.5 (mirrors chart.js v4)
- *(scale)* add suggestedMin/suggestedMax to AxisSpec and wire to value_domain
- *(frontend)* parse matrix chart type from chartjs JSON
- *(schema)* add MatrixSpec types to chartjs schema
- *(ir)* add ChartKind::Matrix and layout/matrix stub
- apply Vega-Lite default theme (Tableau10) to Vega-Lite specs

### Fixed

- *(model)* cover ChartKind::Progress in chart_type_name
- pie fill alpha=1.0 to match chart.js v4 colorizeDoughnutDataset
- remove alpha multipliers in renderers; fill alpha now set by resolve_colors
- resolve_colors uses alpha=0.5 for fill, 1.0 for stroke (chart.js v4)
- align scatter and stacked-bar domains with chart.js
- address Codex follow-up comments and CI fmt
- parameterize value_domain and fix horizontal bar begin_at_zero
- address AI review feedback on nice-ticks PR
- *(scale)* wire suggested_min/max to scatter axis_domain and add value_domain tests
- *(scale)* align nice_ticks target_count with chart.js maxTicksLimit=11 (10 intervals)
- *(frontend)* wire border_color to Series.stroke in parse_matrix
- address AI review feedback
- *(frontend)* detect matrix type before strict key check
- *(frontend)* align label field naming in MatrixRawDataset
- *(schema)* align MatrixDataset with other dataset types
- *(raster_direct)* arc_segment alpha should use d/4 not d/2
- CI format check and publish dry-run failures

### Other

- Merge remote-tracking branch 'origin/main' into feat/chartjs-compat-uob
- *(model)* share ir::color_at, drop dead fmt_alpha branch, pin scatter/horizontal snapshots
- *(model)* pin inspect model snapshots for bar/pie/line
- cargo fmt
- strengthen cycle test to compare full Color equality
- add vegalite domainMin/Max note and scale.rs regenerate command
- *(scale)* add chart.js v4 compatibility pin tests for nice_ticks
- *(scale)* clarify nice_ticks target_count semantics and fix stale comment
- apply cargo fmt to ir.rs and frontend_chartjs.rs
- *(matrix)* add render tests and snapshot
- *(pie)* rustfmt the regression test assertions
- *(pie)* pin chart.js-conformant start angle and clockwise direction
- glyph path cache + x-axis label auto-skip
- bypass SVG string for PNG rendering via direct tiny-skia scene renderer
- add readme and documentation fields to Cargo.toml
- cargo fmt

### Security

- add InputLimits struct and series×categories product check
- add input limits to prevent DoS from untrusted specs

### Added

- `progress` チャートタイプ（QuickChart 互換のプログレスバー）に対応。角丸トラック
  + ソリッド前景、中央のパーセンテージ表示、任意の 2 つ目 dataset による per-bar の
  max 上書きをサポート。

## [0.1.0] - 2026-06-17

### Added

- 棒グラフ（縦 / 横）・折れ線グラフ・エリアチャート・円グラフ・ドーナツグラフに対応。
- chart.js v4 互換のデータ専用・静的サブセットの入力に対応。
- SVG / PNG の出力に対応（PNG は `--scale` で解像度倍率を指定可能）。
- `render` サブコマンドを持つ CLI（ファイル / 標準入力・標準出力のパイプ、`--strict`）。
- 決定的な出力（同一入力なら byte-identical）。
- Noto Sans JP フォントを同梱（システムフォントは読み込まない）。

[0.1.0]: https://github.com/fulgur-rs/fulgur-chart/releases/tag/v0.1.0
