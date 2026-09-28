# Chart.js Tick Styling Fields Design

## Goal

Chart.js 4.5.1 の `options.scales.{x,y}.grid` にある
`tickColor`、`tickWidth`、`tickLength` を、型付き schema から IR と既存の
tick 描画経路まで伝搬する。`drawTicks` の fulgur 固有既定値 `false` は維持する。

## Evidence

Chart.js 4.5.1 の axis styling contract では、`tickColor` 未指定時は grid
color、`tickWidth` 未指定時は grid line width を継承し、`tickLength` の既定値は
8px である。

参照:
<https://www.chartjs.org/docs/latest/axes/styling.html>

現状の fulgur-chart は次の値を固定または流用しており、入力フィールドが描画へ
届かない。

- tick color: `AxisGrid::color`
- tick width: `AxisGrid::line_width`
- tick length: 各 layout の `TICK_LEN = 4.0`

## Scope

- `GridLineOptions.tick_color` と `tick_width` を型付き static JSON subset にする。
- `AxisGrid` に tick 専用の color、width、length を追加する。
- `axis_grid_from` で Chart.js の継承規則と既定値を解決する。
- 既に tick を描く `layout/common.rs`、`layout/scatter.rs`、
  `layout/bar.rs::build_horizontal` の各経路へ反映する。
- Vega-Lite temporal 軸の既存描画と snapshot を維持する。
- RED -> GREEN -> REFACTOR の順で変更する。

次は対象外とする。

- 新しい軸や位置への tick 描画追加
- `tickBorderDash` / `tickBorderDashOffset`
- scriptable callback の実行
- per-tick 配列の全要素描画。既存の static subset と同様、先頭値だけを採用する
- 未追跡の `docs/plans/2026-07-14-chartjs-compat-gap.md` の変更またはコミット

## Schema

`GridLineOptions` の既存フィールドを次の型へ変更する。

```rust
pub tick_color: Option<ScalarOrArray<ColorString>>,
pub tick_width: Option<ScalarOrArray<f64>>,
pub tick_length: Option<f64>,
```

`tickColor` と `tickWidth` は、既存の `color` と `lineWidth` と同じく static
scalar/array を受ける。配列は schema 互換のため受理するが、現在の描画 subset
では先頭要素だけを使用する。未知キーの `deny_unknown_fields` 方針は変えない。

## IR and Default Resolution

`AxisGrid` をフラットに拡張する。

```rust
pub struct AxisGrid {
    pub display: bool,
    pub color: Option<Color>,
    pub line_width: f64,
    pub draw_ticks: bool,
    pub tick_color: Option<Color>,
    pub tick_width: Option<f64>,
    pub tick_length: f64,
}
```

`AxisGrid::default()` は次を唯一の既定値源とする。

- `display = true`
- `color = None`
- `line_width = 1.0`
- `draw_ticks = false`
- `tick_color = None`
- `tick_width = None`
- `tick_length = 8.0`

`draw_ticks = false` は既存 snapshot を守る意図的な Chart.js との差異である。
`tick_color = None` と `tick_width = None` は「未解決」ではなく、それぞれ grid
color と grid line width を継承する契約を表す。`AxisGrid` に小さな解決メソッドを
置き、全 layout が同じ規則を使う。

```rust
pub fn resolved_tick_color(&self, fallback_grid_color: Color) -> Color {
    self.tick_color.or(self.color).unwrap_or(fallback_grid_color)
}

pub fn resolved_tick_width(&self) -> f64 {
    self.tick_width.unwrap_or(self.line_width)
}
```

`axis_grid_from` は先に `let defaults = AxisGrid::default()` を作り、各未指定値を
そのフィールドから取得する。これにより `AxisGrid::default()` と parser の
ハードコードが将来ずれることを防ぐ。`tickColor` と `tickWidth` の配列は既存の
grid field と同じく先頭値だけを変換する。

## Rendering

各既存 tick path は次の3値を使用する。

- stroke: `resolved_tick_color(theme.grid_color)`
- stroke width: `resolved_tick_width()`
- geometry length: `tick_length`

対象 path:

1. `common::draw_frame` の categorical/linear y-axis tick
2. `common::draw_frame` の temporal x-axis tick
3. `scatter::build` の x/y-axis tick
4. `bar::build_horizontal` の linear x-axis tick

category x-axis など、現状 tick を生成していない位置には新しい primitive を
追加しない。

## Vega-Lite Boundary

Vega-Lite temporal 軸は現在、grid opacity を tick へ伝搬せず、tick を theme text
color と 4px length で描く。この契約は Chart.js の grid field とは別の
frontend semantics なので維持する。

`temporal_axis_grid` は `AxisGrid` を構築するとき、次を明示する。

- `tick_color = Some(theme.text_color)`
- `tick_length = 4.0`
- `tick_width = None`。`line_width = 1.0` を継承する

これにより layout は共通の解決メソッドを使いながら、既存の
`grid_opacity_does_not_fade_temporal_tick_marks` contract と snapshots を保てる。

## Error Handling

型が不正な `tickColor`、`tickWidth`、`tickLength` は typed schema の
deserialization error とする。色文字列の解釈失敗、負値や非有限値に対する新しい
validation policy はこの issue では追加せず、既存の grid field 方針を維持する。

## Testing

TDD で次を順に証明する。

1. schema が scalar/array の `tickColor` と `tickWidth`、scalar の
   `tickLength` を型付きで保持する。
2. `AxisGrid::default()` が `draw_ticks=false` と `tick_length=8.0` を返し、
   color/width の継承メソッドが正しく解決する。
3. `axis_grid_from` が explicit scalar/array 値を IR へ流し、未指定値を
   `AxisGrid::default()` と継承規則から解決する。
4. common y-axis tick が独立した color、width、length を `Prim::Line` に反映する。
5. scatter の x/y-axis tick と horizontal bar の x-axis tick が同じ contract を
   使う。
6. Vega-Lite temporal tick は grid opacity の影響を受けず、既存の 4px geometry
   と snapshots を維持する。
7. targeted tests、`cargo test -p fulgur-chart`、fmt、clippy、fresh patch coverage
   100% を通す。

## Compatibility

`drawTicks` 未指定の Chart.js spec は引き続き tick を描かないため、既存の通常
snapshot は変わらない。`drawTicks: true` を明示した spec は、Chart.js 4.5.1
互換の 8px length と grid style 継承へ変わる。`tickColor`、`tickWidth`、
`tickLength` を明示した spec は、それぞれ独立して描画へ反映される。
