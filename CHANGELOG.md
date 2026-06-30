# Changelog

このプロジェクトの主な変更点を記録します。
フォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に従い、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [Unreleased]

### Changed

- PNG: 大データのマーカー描画を高速化。同一 appearance の円マーカーが 128 点以上連続する場合、マーカーを一度だけラスタライズした stamp を各点へ手書き source-over blit で転写する（`scatter_large` 実測 ~4.9× 高速化、`line_large` もマーカー分高速化）。
  - 出力は視覚的に同等（サブピクセル位置量子化 ≤1/8px、tiny-skia と byte 一致の合成）だが、**128 点以上のマーカー図では PNG の byte 出力が変わり得る**（出力をハッシュ/ピン留めしている場合は再生成が必要）。chart.js 互換性（意味モデル・SVG・色）と native↔wasm 決定性は維持。
  - マーカー 128 点未満のチャート・bubble（点ごと半径）・点ごとに色が変わるチャートは従来どおり `fill_path` で描画され、出力は不変。

## [0.1.17](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.16...fulgur-chart-cli-v0.1.17) - 2026-06-28

### Other

- Merge pull request #87 from fulgur-rs/feat/sankey
- Merge pull request #88 from fulgur-rs/feat/webp

## [0.11.5](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.11.4...fulgur-chart-v0.11.5) - 2026-06-28

### Added

- add render_chart_to_webp() lossless via image crate

### Fixed

- revert is_finite() guard — let +Inf scale hit area error per contract
- demultiply alpha before WebP encode, add axis limit check, fix scale Inf
- use English error messages and remove redundant as_deref()

### Other

- Merge pull request #87 from fulgur-rs/feat/sankey
- extract scene_to_pixmap() for PNG/WebP sharing
- add image crate dependency for WebP encoding

## [0.1.16](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.15...fulgur-chart-cli-v0.1.16) - 2026-06-28

### Fixed

- *(clippy)* allow mutable_key_type for FxHashSet<IStr> in stdlib patch
- *(security)* address AI review feedback on Jsonnet sandbox

### Security

- *(cli)* sandbox Jsonnet imports and disable std.parseYaml

## [0.1.15](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.14...fulgur-chart-cli-v0.1.15) - 2026-06-28

### Added

- *(cli)* support .jsonnet files in batch mode and add syntax-error tests
- *(cli)* add Jsonnet support to inspect subcommand
- *(cli)* auto-detect .jsonnet extension and evaluate via jrsonnet
- *(cli)* reject --jsonnet with file path (use .jsonnet extension instead)
- *(cli)* add --jsonnet flag and evaluate_jsonnet_snippet for stdin

### Fixed

- *(lint)* collapse nested if in CLI (clippy::collapsible_if)
- *(cli)* address Codex Review feedback
- *(cli)* guard --jsonnet in batch mode, fix tempdir isolation, add inspect flag test

### Other

- *(cli)* replace non-runnable Jsonnet stdin example
- *(cli)* fix Jsonnet help inaccuracies
- *(cli)* improve --help for Jsonnet input
- Merge pull request #82 from fulgur-rs/feat/jsonnet-input
- *(cli)* verify .jsonnet import resolution and .libsonnet direct input rejection
- *(cli)* add jrsonnet-evaluator dependency

## [0.11.4](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.11.3...fulgur-chart-v0.11.4) - 2026-06-28

### Fixed

- *(lint)* collapse nested if blocks (clippy::collapsible_if)

### Other

- Merge pull request #82 from fulgur-rs/feat/jsonnet-input

## [0.1.14](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.13...fulgur-chart-cli-v0.1.14) - 2026-06-27

### Other

- add wordCloud to supported chart types in README

## [0.11.3](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.11.2...fulgur-chart-v0.11.3) - 2026-06-27

### Added

- *(layout)* implement wordcloud archimedean spiral placement
- *(frontend)* add wordCloud parser
- *(guard)* add wordcloud word count and label byte validation
- *(schema)* add WordCloudSpec and schema roundtrip test
- *(ir)* add WordEntry and ChartKind::WordCloud
- *(scene)* add rotate_deg to Prim::Text for SVG transform support

### Fixed

- *(wordcloud)* reject multiple datasets explicitly
- *(wordcloud)* address coderabbit review
- *(wordcloud)* address AI review feedback
- *(wordcloud)* handle +90deg vertical, clarify step_idx intent
- *(guard)* tighten PCT_LEN_BOUND from 32 to 3 to avoid false positives
- *(guard)* use struct-init syntax in tests to satisfy clippy field_reassign_with_default
- *(guard)* reject outlabeledPie when aggregate expanded outlabel text exceeds limit

### Other

- *(wordcloud)* improve coverage for guard, strict mode, and layout
- add wordCloud to supported chart types in README
- *(wordcloud)* verify example spec renders end-to-end
- *(wordcloud)* add render tests and example spec
- add WordCloud stub arms to fix exhaustive match build errors
- *(svg)* add rotate_deg transform test; note raster unsupported
- *(guard)* pre-analyze outlabel template once to avoid O(N×T) in validator

## [0.1.13](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.12...fulgur-chart-cli-v0.1.13) - 2026-06-25

### Other

- updated the following local packages: fulgur-chart

## [0.1.12](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.11...fulgur-chart-cli-v0.1.12) - 2026-06-25

### Other

- updated the following local packages: fulgur-chart

## [0.1.11](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.10...fulgur-chart-cli-v0.1.11) - 2026-06-24

### Fixed

- *(schema)* add treemap variant to ChartJsSpec; README type list

### Other

- release
- Merge pull request #63 from fulgur-rs/feat/chart-cli-npm-publish
- Merge pull request #62 from fulgur-rs/feat/treemap-chart
- *(treemap)* document tree/key/groups shape and key requirement

## [0.11.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.10.0...fulgur-chart-v0.11.0) - 2026-06-24

### Added

- *(layout)* implement squarified treemap with depth color, captions, labels
- *(frontend)* parse treemap type with tree/key/groups hierarchy
- *(ir)* add TreeNode, Series.tree, ChartKind::Treemap

### Fixed

- *(treemap)* drop unsupported legend option (Codex)
- *(treemap)* handle non-finite areas; drop backgroundColor (Codex)
- *(treemap)* overflow-safe squarify; tighter caption threshold; drop border opts (Codex)
- *(treemap)* cap numeric tree rows; keep children when group cell too short (Codex)
- *(guard)* accept treemap leaf at exactly max depth (coderabbit)
- *(treemap)* address AI review (DoS guards, schema/strict parity, perf)
- *(schema)* add treemap variant to ChartJsSpec; README type list
- *(treemap)* cap groups depth to prevent parser stack overflow (DoS); guard short-rect captions

### Other

- release
- Merge pull request #63 from fulgur-rs/feat/chart-cli-npm-publish
- Merge pull request #62 from fulgur-rs/feat/treemap-chart
- *(treemap)* document tree/key/groups shape and key requirement
- *(render)* add treemap end-to-end render and snapshot tests

## [0.1.11](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.10...fulgur-chart-cli-v0.1.11) - 2026-06-24

### Fixed

- *(schema)* add treemap variant to ChartJsSpec; README type list

### Other

- Merge pull request #63 from fulgur-rs/feat/chart-cli-npm-publish
- Merge pull request #62 from fulgur-rs/feat/treemap-chart
- *(treemap)* document tree/key/groups shape and key requirement

## [0.11.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.10.0...fulgur-chart-v0.11.0) - 2026-06-24

### Added

- *(layout)* implement squarified treemap with depth color, captions, labels
- *(frontend)* parse treemap type with tree/key/groups hierarchy
- *(ir)* add TreeNode, Series.tree, ChartKind::Treemap

### Fixed

- *(treemap)* drop unsupported legend option (Codex)
- *(treemap)* handle non-finite areas; drop backgroundColor (Codex)
- *(treemap)* overflow-safe squarify; tighter caption threshold; drop border opts (Codex)
- *(treemap)* cap numeric tree rows; keep children when group cell too short (Codex)
- *(guard)* accept treemap leaf at exactly max depth (coderabbit)
- *(treemap)* address AI review (DoS guards, schema/strict parity, perf)
- *(schema)* add treemap variant to ChartJsSpec; README type list
- *(treemap)* cap groups depth to prevent parser stack overflow (DoS); guard short-rect captions

### Other

- Merge pull request #63 from fulgur-rs/feat/chart-cli-npm-publish
- Merge pull request #62 from fulgur-rs/feat/treemap-chart
- *(treemap)* document tree/key/groups shape and key requirement
- *(render)* add treemap end-to-end render and snapshot tests

## [0.1.10](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.9...fulgur-chart-cli-v0.1.10) - 2026-06-23

### Other

- updated the following local packages: fulgur-chart

## [0.1.9](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.8...fulgur-chart-cli-v0.1.9) - 2026-06-22

### Other

- add crates.io badge for the fulgur-chart library crate
- add Codecov coverage reporting for the Rust core

## [0.9.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.8.0...fulgur-chart-v0.9.0) - 2026-06-22

### Added

- split Bar.stacked into placement_stacked + value_stacked
- *(wasm)* drop usvg/resvg, depend on tiny-skia directly

### Fixed

- address AI review feedback

### Other

- add crates.io badge for the fulgur-chart library crate
- add Codecov coverage reporting for the Rust core
- cargo fmt common.rs (matches! macro wrap)
- cargo fmt (matches! macro line-length wrap)

## [0.1.8](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.7...fulgur-chart-cli-v0.1.8) - 2026-06-22

### Other

- release
- clarify stacked detection follows the index axis

## [0.8.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.7.0...fulgur-chart-v0.8.0) - 2026-06-22

### Fixed

- *(line)* fix label index mismatch and gap handling after filter_map
- *(compat)* address review P2 issues (chart-wide colors, legend, missing pts)
- exclude is_progress from colors_plugin_skips (review)
- *(compat)* align color and axis defaults with chart.js v4 behavior
- *(scatter)* fmt, assert_eq upgrades, add suggested_max tests

### Other

- release
- Merge pull request #41 from fulgur-rs/feat/compat-colors-axes-fix
- fix rustfmt and clippy lint in compat color/axis fix
- Merge pull request #37 from fulgur-rs/feat/geometry-compat
- Merge pull request #38 from fulgur-rs/feat/scatter-axis-domain-tests
- *(scatter)* axis_domain の suggestedMin/Max 単体テストを追加

## [0.1.8](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.7...fulgur-chart-cli-v0.1.8) - 2026-06-22

### Other

- clarify stacked detection follows the index axis

## [0.8.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.7.0...fulgur-chart-v0.8.0) - 2026-06-22

### Fixed

- *(line)* fix label index mismatch and gap handling after filter_map
- *(compat)* address review P2 issues (chart-wide colors, legend, missing pts)
- exclude is_progress from colors_plugin_skips (review)
- *(compat)* align color and axis defaults with chart.js v4 behavior
- *(scatter)* fmt, assert_eq upgrades, add suggested_max tests

### Other

- Merge pull request #41 from fulgur-rs/feat/compat-colors-axes-fix
- fix rustfmt and clippy lint in compat color/axis fix
- Merge pull request #37 from fulgur-rs/feat/geometry-compat
- Merge pull request #38 from fulgur-rs/feat/scatter-axis-domain-tests
- *(scatter)* axis_domain の suggestedMin/Max 単体テストを追加

## [0.1.7](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.6...fulgur-chart-cli-v0.1.7) - 2026-06-22

### Other

- updated the following local packages: fulgur-chart

## [0.1.6](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.5...fulgur-chart-cli-v0.1.6) - 2026-06-21

### Other

- release

## [0.6.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.5.1...fulgur-chart-v0.6.0) - 2026-06-21

### Other

- release
- Merge pull request #29 from fulgur-rs/refactor/remove-svg-to-png
- remove svg_to_png in favour of raster_direct

## [0.1.6](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.5...fulgur-chart-cli-v0.1.6) - 2026-06-21

### Other

- updated the following local packages: fulgur-chart

## [0.1.5](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.4...fulgur-chart-cli-v0.1.5) - 2026-06-21

### Fixed

- *(readme)* use absolute URLs for crates.io image links

### Other

- Merge pull request #27 from fulgur-rs/fix/readme-crates-io-image-links

## [0.5.1](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.5.0...fulgur-chart-v0.5.1) - 2026-06-21

### Fixed

- *(readme)* use absolute URLs for crates.io image links

### Other

- Merge pull request #27 from fulgur-rs/fix/readme-crates-io-image-links

## [0.1.4](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.3...fulgur-chart-cli-v0.1.4) - 2026-06-21

### Other

- updated the following local packages: fulgur-chart

## [0.1.3](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.2...fulgur-chart-cli-v0.1.3) - 2026-06-21

### Added

- *(gauge)* add Gauge/RadialGauge variants to JSON schema

### Fixed

- *(gauge)* reject plugins.legend for gauge/radialGauge in schema + strict
- address AI review feedback on gauge/radialGauge

### Other

- apply rustfmt to CLI help attributes
- *(cli)* add examples and exit-code docs to --help
- Merge pull request #22 from fulgur-rs/feat/gauge-radialgauge
- *(gauge)* add example specs, README and CHANGELOG entries
- update README with new chart types, CLI options, and Ruby binding

## [0.4.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.3.0...fulgur-chart-v0.4.0) - 2026-06-21

### Added

- *(gauge)* add Gauge/RadialGauge variants to JSON schema
- *(gauge)* strict unknown-key validation + options.theme support
- *(gauge)* value label with rounded background
- *(gauge)* semicircle color zones + needle
- *(gauge)* radialGauge rounded caps + center value text
- *(gauge)* radialGauge track ring + clamped value arc
- *(gauge)* add deterministic ring_segment_path helper
- *(gauge)* add RadialGauge/Gauge ChartKind, dedicated parse path, layout scaffold
- add sparkline to chart_type_name in model.rs
- add layout/sparkline.rs and dispatch in mod.rs
- parse "sparkline" type to ChartKind::Sparkline
- add ChartKind::Sparkline variant to ir
- *(scene)* add stroke/stroke_width to Prim::Circle; update all callers

### Fixed

- *(gauge)* reject plugins.legend for gauge/radialGauge in schema + strict
- *(gauge)* broadcast scalar zone color; reject dataset borders in strict
- *(gauge)* adapt to Series.box_points and Prim::Circle stroke after rebase
- address AI review feedback on gauge/radialGauge
- *(num)* fmt_num never emits inf for huge finite values
- *(schema)* gauge plugins expose only title/legend (no datalabels) to match parser
- *(gauge)* scale radialGauge center value with inner radius for QuickChart fidelity
- *(gauge)* keep value label on-canvas by reserving a bottom label band
- *(gauge)* center value text baseline must scale with rendered size
- address Codex review P2 feedback on sparkline
- address AI review feedback
- *(scatter)* correct stroke fallback to rgba(0,0,0,0.1) when backgroundColor is set
- *(svg)* align Circle stroke attr order with Path; add stroke SVG test
- *(scatter)* derive stroke from backgroundColor when borderColor is absent

### Other

- Merge pull request #22 from fulgur-rs/feat/gauge-radialgauge
- *(gauge)* document strict validator as intentional lenient union; drop unused param
- *(gauge)* add example specs, README and CHANGELOG entries
- *(gauge)* edge cases, determinism, snapshots, PNG regression
- *(gauge)* name needle/cutout constants; guard non-finite needle value
- update README with new chart types, CLI options, and Ruby binding
- harden sparkline Z/C assertions per coderabbit feedback
- add render_sparkline tests with snapshot
- *(svg)* write Circle attrs directly to output, avoid temp allocation
- apply rustfmt
- *(scatter)* add missing test for no-backgroundColor + no-borderColor case

### Added

- `gauge` チャートタイプ（QuickChart 互換の chartjs-gauge）に対応。累積閾値から成る
  半円の色帯ゾーン + value を指す針 + 値ラベルを描画。`options.needle` /
  `options.valueLabel` で設定でき、JS の `valueLabel.formatter` は丸めた数値で代替。
- `radialGauge` チャートタイプ（QuickChart 互換の radial-gauge）に対応。トラックリング上に
  value まで塗りつぶす全円の弧 + 中央の値テキストを描画。`options.domain` / `trackColor` /
  `centerPercentage` / `roundedCorners` で設定でき、JS の `centerArea.text` は丸めた数値で代替。

## [0.1.2](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.1...fulgur-chart-cli-v0.1.2) - 2026-06-21

### Other

- updated the following local packages: fulgur-chart

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
