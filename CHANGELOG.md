# Changelog

このプロジェクトの主な変更点を記録します。
フォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に従い、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [Unreleased]

## [0.1.21](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.20...fulgur-chart-cli-v0.1.21) - 2026-08-30

### Other

- *(vegalite)* end-to-end stacked area coverage + example

## [0.13.2](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.13.1...fulgur-chart-v0.13.2) - 2026-08-30

### Added

- *(vegalite)* parse mark:"area", wire stacked + Series.area
- *(schema)* Vega-Lite area mark types (temporal + categorical)
- *(layout)* stacked area geometry (line.rs)
- *(layout)* stacked y-domain for ChartKind::Line
- *(layout)* render logarithmic x-axis for horizontal bar
- *(layout)* render logarithmic y-axis (major/minor grid, log-aware labels)
- *(num)* add fmt_num_log for wide-magnitude logarithmic tick labels
- *(layout)* compute logarithmic axis domain (zero substitution, negative exclusion)
- *(scale)* add log_ticks (major/minor decade tick generation)
- *(frontend)* parse scales.{x,y}.type=logarithmic into ScaleKind, mask negatives
- *(schema)* accept options.scales.{x,y}.type as an opaque string
- *(ir)* add ScaleKind::{Linear,Logarithmic} to AxisSpec (no behavior change)
- *(chart)* render gapped and stepped line series
- *(chartjs)* parse line gap and step options
- *(chartjs)* draw categorical x-axis grids
- *(line)* add monotone interpolation
- *(layout)* render temporal Vega-Lite axes
- *(vegalite)* convert temporal lines to positioned IR
- *(schema)* type Vega-Lite temporal line options
- *(vegalite)* add deterministic temporal scales
- *(ir)* add positioned line contracts
- *(schema)* accept documented Chart.js v1-noop axis fields
- *(layout)* apply axis styling to scatter/bar-horizontal/boxplot
- *(layout)* render x-axis title and expand plot bottom
- *(layout)* render y-axis title (rotated) and expand plot_left
- *(layout)* honor AxisBorder and grid.draw_ticks in draw_frame
- *(layout)* honor AxisGrid display/color/line_width in draw_frame
- *(chartjs)* wire scales.{x,y}.{title,grid,border} into IR
- *(chartjs)* add axis_title_from/axis_grid_from/axis_border_from helpers
- *(scene)* add dash pattern support to Prim::Line
- *(ir)* add AxisTitle/AxisGrid/AxisBorder sub-structs
- *(schema)* swap AxisOptions title/grid to typed structs, add border
- *(schema)* add typed AxisTitle/GridLine/AxisBorder options

### Fixed

- *(line)* apply series step_mode to stacked area's near edge too
- *(vegalite)* strict mode must reject background/config on categorical area
- *(vegalite)* note is_area/temporal_line ordering is not load-bearing
- *(vegalite)* temporal area strict mode must accept title/scale
- *(layout)* document log-scale + stacked Line/Area gap, file tracker
- *(schema)* document why MarkAreaObject stays a single type
- *(layout)* stacked data-label OOB panic and line_points/build parity
- *(layout)* correct value_domain stacked-branch comment
- *(ir)* correct stacked doc-comment citation, tighten two assertions
- *(clippy)* replace chunks_exact(4) with as_chunks for clippy 1.98
- *(ci)* satisfy Rust 1.98 Clippy
- *(scale)* cap log_ticks_within tick count with MAX_TICK_INTERVALS
- *(scale)* cap log_ticks tick count with MAX_TICK_INTERVALS
- *(chartjs)* map logarithmic axes against the tight data domain, not decade-rounded ticks (P1)
- *(chartjs)* address 3 log-scale review findings on PR #144
- *(layout)* treat zero as a gap (not a floor-clamped point) on logarithmic lines
- *(layout)* apply beginAtZero to logarithmic axes when min sits on a decade boundary
- *(layout)* make data-label formatting scale-aware for logarithmic axes
- *(layout)* honor suggested_min/suggested_max when a log axis has no positive data
- *(num)* switch fmt_num_log to scientific notation beyond a magnitude threshold
- *(layout)* dim minor gridlines on logarithmic axes for readability
- *(frontend)* reject logarithmic value axis combined with stacked bars
- *(model)* report logarithmic axes correctly in ChartModel introspection
- *(layout)* compute stacked horizontal data-label midpoint in pixel space
- *(layout)* correct false invariant in log_value_domain overflow comment
- *(scale)* correct misleading overflow comment on log_ticks exponent clamp, document invariants
- *(chartjs)* keep narrow horizontal plots inside canvas
- *(chartjs)* preserve horizontal plot width for narrow layouts
- *(chartjs)* handle horizontal tick edge padding
- *(chartjs)* keep horizontal tick labels inside canvas
- *(chart)* align stepped geometry and allocation
- *(chart)* correct stepped line corners
- *(chartjs)* reject ignored line options
- *(chartjs)* reject line options for mixed charts
- *(chart)* omit duplicate stepped vertices
- *(chartjs)* scope line options to line roots
- *(vegalite)* retain scatter x grids
- *(chartjs)* address categorical grid reviews
- *(chartjs)* retain grid for empty category tick
- *(chartjs)* draw horizontal bar x-axis border
- *(temporal)* select millisecond ticks by target
- *(scale)* align deduplicated tick steps
- *(model)* align supported legend counts
- *(layout)* contain aligned plot titles
- *(scale)* expand extreme singletons inward
- *(temporal)* bound generated ticks
- *(guard)* validate effective marker radii
- *(layout)* scope legend title activation
- *(vegalite)* require temporal color fields
- *(vegalite)* validate line types in both modes
- *(raster)* bound marker geometry safely
- *(guard)* validate per-point radii
- *(guard)* bound explicit point radii
- *(raster)* normalize text rotation angles
- *(raster)* render rotated text
- *(scale)* interpolate extreme pixel ranges safely
- *(scale)* map extreme mixed domains safely
- *(vegalite)* validate temporal config container
- *(layout)* match D3 monotone tangents
- *(vegalite)* guard group labels before cloning
- *(vegalite)* validate temporal axis config
- *(scale)* bound Vega tick fallbacks
- *(scale)* preserve tiny Vega domains
- *(model)* omit irregular temporal step
- *(render)* preserve caller input limits
- *(layout)* contain PlotArea chart titles
- *(layout)* contain centered PlotArea x titles
- *(vegalite)* validate temporal view config
- *(vegalite)* validate temporal channel titles
- *(vegalite)* preserve temporal axis semantics
- *(vegalite)* validate non-strict grid option
- *(layout)* contain first temporal tick label
- *(line)* offset labels by marker radius
- *(vegalite)* format numeric groups like ECMAScript
- *(vegalite)* canonicalize numeric color groups
- *(vegalite)* break numeric color order ties
- *(vegalite)* preserve typed color groups
- *(vegalite)* sort numeric color domains
- *(vegalite)* reject non-finite temporal aggregates
- *(vegalite)* accept nullable temporal options
- *(layout)* contain tall PlotArea legends
- *(layout)* avoid temporal coordinate overflow
- *(vegalite)* require strict line color fields
- *(schema)* split temporal and categorical lines
- *(vegalite)* close scene boundary gaps
- *(vegalite)* address second review round
- *(vegalite)* address temporal line review gaps
- *(model)* scope plot area expansion to lines
- *(vegalite)* close temporal review gaps
- *(model)* preserve categorical line dimensions
- *(layout)* scope temporal legend titles
- *(vegalite)* bound strict validation errors
- *(layout)* 逆転ドメインは hard bound を基準に展開する
- *(layout)* データ欠如時の片側 suggestion と f64::MAX 近傍の縮退を扱う
- *(layout)* 動径比率をオーバーフロー安全にし hard max の境界リングを補う
- *(chartjs)* 非 radial チャートの scales.r を kind 確定まで生値で保持する
- *(layout)* 動径ドメイン解決を共通化し hard bound と縮退の扱いを chart.js に揃える
- *(layout)* 動径ドメインを「側ごと」に解決し hard bound を優先する
- *(raster)* fall back to solid when dash pattern exceeds tiny-skia limit
- *(raster)* double odd-length stroke dash to match SVG semantics
- *(layout)* align Y-title Start/End to Chart.js semantics (bottom-to-top read)
- *(chartjs)* allow non-x/y scale axes in non-strict mode
- *(schema)* allow unmodeled Chart.js scale fields in non-strict mode
- *(chartjs)* allow border under strict mode + strengthen axis options test

### Other

- cargo fmt
- Merge remote-tracking branch 'origin/main' into feat/vegalite-stacked-area
- *(vegalite)* end-to-end stacked area coverage + example
- *(layout)* cover the stacked area near-edge polygon close
- *(ir)* add stacked field to ChartKind::Line
- *(scale)* satisfy cargo fmt for log_ticks guard test
- *(chartjs)* update log_value_domain's begin_at_zero doc for the general decade-floor rule
- *(scale)* rename log_ticks test to match what it actually verifies
- *(golden)* add bar_logarithmic golden PNG regression case
- *(frontend)* pin scatter log-scope exclusion, tighten radial type-key error assertion
- *(frontend)* integration-level coverage for logarithmic scale parsing
- *(layout)* update stale Task-12-pending comment now that model.rs handles the sentinel
- *(scale)* update ValueScale::Log doc now that Task 9 makes it reachable
- *(num)* strengthen fmt_num_log tests per code review
- *(layout)* fix value_domain doc contract, cite fulgur-chart-bap, add widen test
- *(scale)* pin log_ticks structural invariants (bracketing, ascending order, exact powers of ten)
- *(scale)* fix MAX_LOG_DECADES value in log_ticks clamp comment (308, not 309)
- *(frontend)* cover logarithmic scale scoping for Line and Mixed chart kinds
- *(scale)* remove internal plan task-number reference from ValueScale doc comment
- *(scale)* wrap LinearScale in ValueScale (no-op for linear path)
- *(ir)* note which chart kinds consume AxisSpec.scale_kind
- *(chart)* distinguish cubic paths from grids
- *(chart)* avoid copying unstepped area points
- *(chart)* cover stepped line edge cases
- *(chartjs)* verify line option schema parity
- *(chart)* restore pie golden scope
- *(chartjs)* batch categorical x grids
- *(chartjs)* strengthen horizontal border assertions
- *(chartjs)* update horizontal bar border snapshots
- cover final patch boundaries
- strengthen final review boundaries
- *(guard)* restore marker radius boundaries
- *(layout)* cover complete legend activation frame
- *(vegalite)* cover temporal color field types
- *(raster)* cover invalid circle bounds
- *(ir)* update radial fixture after rebase
- *(vegalite)* update D3 monotone snapshot
- *(layout)* pin equal-spacing monotone path
- *(vegalite)* cover defensive scale fallbacks
- *(render)* clarify limits validation scope
- *(layout)* assert PlotArea legend text anchors
- *(vegalite)* cover typed color ordering
- *(vegalite)* cover nullable x channel type
- *(vegalite)* close review patch coverage
- *(vegalite)* cover temporal review boundaries
- *(legend)* assert right legend entries
- *(vegalite)* close dogfood patch coverage
- *(vegalite)* satisfy final clippy gate
- *(vegalite)* cover temporal dogfood parity
- *(bench)* gate PNG memory allocations
- *(bench)* define PNG memory targets
- *(radial)* 両側 hard で矛盾するドメイン指定をカバー
- Merge branch 'main' into feat/6z6-radial-scale
- Merge pull request #136 from fulgur-rs/feat/s7o-axis-styling
- final polish
- *(chartjs)* axis title/grid/border integration fixtures
- *(raster)* assert dashed line rasterizes different pixels from solid
- *(ir)* swap AxisSpec title/grid to typed sub-structs, add border
- *(schema)* cover camelCase deserialization for GridLineOptions

## [0.1.20](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.19...fulgur-chart-cli-v0.1.20) - 2026-07-14

### Other

- updated the following local packages: fulgur-chart

### Added

- *(sankey)* accept `dataset.parsing: false` as a no-op (chartjs-chart-sankey compat)

## [0.1.19](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.18...fulgur-chart-cli-v0.1.19) - 2026-07-11

### Other

- release

## [0.13.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.12.0...fulgur-chart-v0.13.0) - 2026-07-11

### Added

- *(sankey)* dataset.parsing for from/to/flow key remap
- *(sankey)* per-link color / colorFrom / colorTo overrides
- *(sankey)* accept hoverColorFrom/hoverColorTo as no-op
- *(vegalite)* add parse_with_limits API for caller-supplied InputLimits
- *(layout)* implement vega_rect grid renderer
- *(vegalite)* strict validation for mark: "rect"
- *(vegalite)* aggregate mean/sum for rect color channel
- *(vegalite)* nominal color for mark: "rect" + parse_rect_kind extraction
- *(vegalite)* parse mark: "rect" with quantitative color
- *(schema)* add Rect variant to Vega-Lite JSON Schema
- *(ir)* add ChartKind::VegaRect variant for Vega-Lite rect mark
- *(vegalite)* add Circle variant to JSON Schema (mark: "circle")
- *(vegalite)* accept `mark: "circle"` in frontend parser
- *(chart)* add decimate_segments with per-segment budget proration (fulgur-chart-vzd)
- *(chart)* decimate huge sparklines (single-segment, auto-on)
- *(chart)* advertise decimation in sparkline JSON schema (parity)
- *(scene)* add has_opaque_background() predicate (a7c)

### Fixed

- *(sankey)* collapse hoverColor if-let nesting for clippy
- *(sankey)* parsing-mapped color-key collision short-circuits color read
- *(sankey)* strict-validate parsing keys, treat null per-link color as absent
- *(vegalite)* reject non-string rect axis/color type hints in strict
- *(vegalite)* tighten strict type validation and schema for rect
- *(vegalite)* three-tier mean fallback for extreme cancellation
- *(vegalite)* address Codex round-4 review on PR #126
- *(vegalite)* add pre-allocation guard in build_rect for oversized inputs
- *(vegalite)* address Codex round-2 review on PR #126
- *(vegalite)* address AI review feedback on PR #126
- *(vegalite)* reject quantitative color with non-numeric values + polish
- *(vegalite)* apply mark-specific encoding allow-list in strict mode
- *(vegalite)* forbid extra properties on object-form mark specs
- *(vegalite)* accept `{"type": "<mark>"}` object form in all mark schemas
- *(chart)* widen LTTB budget multiply to u64 to avoid wasm32 overflow (fulgur-chart-vzd)
- *(chart)* prorate LTTB samples across gap segments (fulgur-chart-vzd)

### Other

- *(sankey)* tighten strict-typo assertion to require unknown key name
- *(vegalite)* snapshot golden for mark: "rect" heatmap
- *(vegalite)* pin unknown-mark fall-through under strict + add plan
- *(vegalite)* tighten strict allow-list review nits
- Merge pull request #124 from fulgur-rs/feat/vl-circle-mark
- *(vegalite)* pin structural shape rejection + tighten circle SVG smoke
- *(vegalite)* add SVG smoke test for mark: "circle"
- *(vegalite)* switch circle section comment to English + doc VlCircleEncoding
- *(chart)* add sparkline_large cases to membench baseline
- *(chart)* add sparkline_large decimation cases
- *(chart)* strengthen sparkline decimation tests (area fire-path, bezier-count proxy, rename)
- *(raster)* explain why f32 coverage comparison is sound (a7c review)
- pin opaque-bg zero-partial-alpha invariant (a7c)
- *(webp)* skip alpha demultiply scan on opaque background (a7c)
- *(png)* skip alpha demultiply scan on opaque background (a7c)

## [0.1.19](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.18...fulgur-chart-cli-v0.1.19) - 2026-07-10

### Other

- release

## [0.13.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.12.0...fulgur-chart-v0.13.0) - 2026-07-10

### Added

- *(sankey)* dataset.parsing for from/to/flow key remap
- *(sankey)* per-link color / colorFrom / colorTo overrides
- *(sankey)* accept hoverColorFrom/hoverColorTo as no-op
- *(vegalite)* add parse_with_limits API for caller-supplied InputLimits
- *(layout)* implement vega_rect grid renderer
- *(vegalite)* strict validation for mark: "rect"
- *(vegalite)* aggregate mean/sum for rect color channel
- *(vegalite)* nominal color for mark: "rect" + parse_rect_kind extraction
- *(vegalite)* parse mark: "rect" with quantitative color
- *(schema)* add Rect variant to Vega-Lite JSON Schema
- *(ir)* add ChartKind::VegaRect variant for Vega-Lite rect mark
- *(vegalite)* add Circle variant to JSON Schema (mark: "circle")
- *(vegalite)* accept `mark: "circle"` in frontend parser
- *(chart)* add decimate_segments with per-segment budget proration (fulgur-chart-vzd)
- *(chart)* decimate huge sparklines (single-segment, auto-on)
- *(chart)* advertise decimation in sparkline JSON schema (parity)
- *(scene)* add has_opaque_background() predicate (a7c)

### Fixed

- *(sankey)* collapse hoverColor if-let nesting for clippy
- *(sankey)* parsing-mapped color-key collision short-circuits color read
- *(sankey)* strict-validate parsing keys, treat null per-link color as absent
- *(vegalite)* reject non-string rect axis/color type hints in strict
- *(vegalite)* tighten strict type validation and schema for rect
- *(vegalite)* three-tier mean fallback for extreme cancellation
- *(vegalite)* address Codex round-4 review on PR #126
- *(vegalite)* add pre-allocation guard in build_rect for oversized inputs
- *(vegalite)* address Codex round-2 review on PR #126
- *(vegalite)* address AI review feedback on PR #126
- *(vegalite)* reject quantitative color with non-numeric values + polish
- *(vegalite)* apply mark-specific encoding allow-list in strict mode
- *(vegalite)* forbid extra properties on object-form mark specs
- *(vegalite)* accept `{"type": "<mark>"}` object form in all mark schemas
- *(chart)* widen LTTB budget multiply to u64 to avoid wasm32 overflow (fulgur-chart-vzd)
- *(chart)* prorate LTTB samples across gap segments (fulgur-chart-vzd)

### Other

- *(sankey)* tighten strict-typo assertion to require unknown key name
- *(vegalite)* snapshot golden for mark: "rect" heatmap
- *(vegalite)* pin unknown-mark fall-through under strict + add plan
- *(vegalite)* tighten strict allow-list review nits
- Merge pull request #124 from fulgur-rs/feat/vl-circle-mark
- *(vegalite)* pin structural shape rejection + tighten circle SVG smoke
- *(vegalite)* add SVG smoke test for mark: "circle"
- *(vegalite)* switch circle section comment to English + doc VlCircleEncoding
- *(chart)* add sparkline_large cases to membench baseline
- *(chart)* add sparkline_large decimation cases
- *(chart)* strengthen sparkline decimation tests (area fire-path, bezier-count proxy, rename)
- *(raster)* explain why f32 coverage comparison is sound (a7c review)
- pin opaque-bg zero-partial-alpha invariant (a7c)
- *(webp)* skip alpha demultiply scan on opaque background (a7c)
- *(png)* skip alpha demultiply scan on opaque background (a7c)

## [0.1.19](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.18...fulgur-chart-cli-v0.1.19) - 2026-07-08

### Other

- updated the following local packages: fulgur-chart

### Added

- *(decimate)* sparkline を間引き対象に追加 (単一セグメント・auto-on、`decimation.enabled:false` で無効化)
- *(decimate)* JSON schema の SparklineOptions に plugins.decimation を追加 (schema↔strict parity)

### Fixed

- *(matrix)* `options.plugins.datalabels` を matrix の schema 契約から除外 (`MatrixPlugins` 導入)。matrix は datalabels を描画しないため、schema 受理→strict 拒否のパリティ破れを解消 (sankey #87 と同型)
- *(decimate)* LTTB の samples 予算をセグメント長で按分し、gap 多数セグメント時の per-segment 予算超過を解消 (合計を samples+3×セグメント数 以下に上限化)

## [0.1.18](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.17...fulgur-chart-cli-v0.1.18) - 2026-07-01

### Added

- *(png)* 既定圧縮をライブラリ全体で Balanced に統一

### Other

- release

## [0.12.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.11.5...fulgur-chart-v0.12.0) - 2026-07-01

### Added

- *(decimate)* threshold 超過 line のマーカーを既定抑制 (pointRadius で復活)
- *(decimate)* line::build に間引きを配線 (セグメント先行分割)
- *(decimate)* JSON schema に DecimationPlugin を追加 (CommonPlugins/BarPlugins)
- *(decimate)* strict parser で options.plugins.decimation を許可
- *(decimate)* chartjs frontend で decimation を parse し IR へ解決
- *(decimate)* 設定型と発動判定 resolve / 単一セグメント間引き decimate_one を追加
- *(decimate)* LTTB デシメーションを追加
- *(decimate)* 列ごと min/max デシメーションを追加
- *(raster)* stamp cache を描画ループへ配線 + フォールバック
- *(raster)* 手書き premultiplied source-over blit
- *(raster)* B=8 サブピクセル stamp ビルダ(fill+stroke)
- *(raster)* 連続均一マーカー run 検出
- *(png)* 既定圧縮をライブラリ全体で Balanced に統一
- *(png)* demultiply 高速化 + 圧縮プリセット (fast/balanced/high)

### Fixed

- *(decimate)* Codex 指摘に対応 (単点gapのマーカー保持 / matrix schema↔strict parity)
- *(raster)* 巨大な有限座標での blit 桁あふれを i64 計算で回避 (codex review)
- *(raster)* AI レビュー対応 — 堅牢性4点
- AI レビュー対応 — compression を起動時設定に、ほか

### Other

- release
- *(decimate)* membench baseline を更新 (line_large_decimated 追加, 現行 alloc を反映)
- *(decimate)* line_points の doc コメントを修正 (build はレンダ点を独立計算・間引き)
- *(decimate)* bench 変種を追加し CHANGELOG に互換性乖離を記載
- *(decimate)* 決定性・no-op サニティ・SVG↔PNG 一致・新規 golden を追加
- *(decimate)* gap あり巨大系列の間引き回帰テストを追加し LTTB の per-segment samples を明記
- *(decimate)* schema の algorithm を enum 化し value レベルの parity を閉じる
- *(decimate)* resolve の戻り値検証を強化し lttb dispatch テストとコメントを追加
- *(raster)* WebP validly 検証を実デコードまで強化 (coderabbit review)
- Merge remote-tracking branch 'origin/main' into perf/raster-stamp-cache
- *(raster)* レビュー対応 — WebP stamp 経路の決定性 + scale=2 許容テスト
- *(raster)* stamp 経路の決定性ゲート(native↔wasm)
- Merge pull request #100 from fulgur-rs/perf/png-demultiply-fast

## [0.1.18](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-cli-v0.1.17...fulgur-chart-cli-v0.1.18) - 2026-07-01

### Added

- *(png)* 既定圧縮をライブラリ全体で Balanced に統一

## [0.12.0](https://github.com/fulgur-rs/fulgur-chart/compare/fulgur-chart-v0.11.5...fulgur-chart-v0.12.0) - 2026-07-01

### Added

- *(decimate)* threshold 超過 line のマーカーを既定抑制 (pointRadius で復活)
- *(decimate)* line::build に間引きを配線 (セグメント先行分割)
- *(decimate)* JSON schema に DecimationPlugin を追加 (CommonPlugins/BarPlugins)
- *(decimate)* strict parser で options.plugins.decimation を許可
- *(decimate)* chartjs frontend で decimation を parse し IR へ解決
- *(decimate)* 設定型と発動判定 resolve / 単一セグメント間引き decimate_one を追加
- *(decimate)* LTTB デシメーションを追加
- *(decimate)* 列ごと min/max デシメーションを追加
- *(raster)* stamp cache を描画ループへ配線 + フォールバック
- *(raster)* 手書き premultiplied source-over blit
- *(raster)* B=8 サブピクセル stamp ビルダ(fill+stroke)
- *(raster)* 連続均一マーカー run 検出
- *(png)* 既定圧縮をライブラリ全体で Balanced に統一
- *(png)* demultiply 高速化 + 圧縮プリセット (fast/balanced/high)

### Fixed

- *(decimate)* Codex 指摘に対応 (単点gapのマーカー保持 / matrix schema↔strict parity)
- *(raster)* 巨大な有限座標での blit 桁あふれを i64 計算で回避 (codex review)
- *(raster)* AI レビュー対応 — 堅牢性4点
- AI レビュー対応 — compression を起動時設定に、ほか

### Other

- *(decimate)* membench baseline を更新 (line_large_decimated 追加, 現行 alloc を反映)
- *(decimate)* line_points の doc コメントを修正 (build はレンダ点を独立計算・間引き)
- *(decimate)* bench 変種を追加し CHANGELOG に互換性乖離を記載
- *(decimate)* 決定性・no-op サニティ・SVG↔PNG 一致・新規 golden を追加
- *(decimate)* gap あり巨大系列の間引き回帰テストを追加し LTTB の per-segment samples を明記
- *(decimate)* schema の algorithm を enum 化し value レベルの parity を閉じる
- *(decimate)* resolve の戻り値検証を強化し lttb dispatch テストとコメントを追加
- *(raster)* WebP validly 検証を実デコードまで強化 (coderabbit review)
- Merge remote-tracking branch 'origin/main' into perf/raster-stamp-cache
- *(raster)* レビュー対応 — WebP stamp 経路の決定性 + scale=2 許容テスト
- *(raster)* stamp 経路の決定性ゲート(native↔wasm)
- Merge pull request #100 from fulgur-rs/perf/png-demultiply-fast

### Added

- line/area: 巨大データ（点数 > プロット幅×4）を既定で自動デシメーション（min-max）＋
  マーカー抑制。Chart.js は decimation を既定無効にするため出力が異なる（意図的な互換性乖離）。
  無効化は `options.plugins.decimation.enabled=false`、マーカー復活は `pointRadius` 明示。
  `algorithm` は `min-max`（既定）/ `lttb`。

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
