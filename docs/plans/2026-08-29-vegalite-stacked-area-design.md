# Vega-Lite: stacked area chart 対応 Design

**beads:** fulgur-chart-boo(「Vega-Lite: area mark の実装」を本件用に拡張)

## 背景・スコープ

Vega-Lite の `mark: "area"` は現状 fulgur-chart 未実装。area mark 自体の実装と積み上げ(stack)対応は本家 Vega-Lite の仕様上不可分なので同時に行う。

- `color` channel を持つ area は既定で積み上げ(`stack: "zero"` 相当)。
- `encoding.y.stack: null` で積み上げを明示的に解除できる。
- `color` channel なしの単系列 area は積み上げ対象がなく常に非積み上げ。

対応する x 軸型は既存の line mark と同じく2系統:
- temporal area(`VlTemporalLineSpec` 相当): x が RFC3339 timestamp
- categorical area(`VlCategoricalLineSpec` 相当): x が nominal/ordinal カテゴリ

積み上げ順序は既存の stacked bar(`ChartKind::Bar { value_stacked: true }`)に倣い、`series[0]` を最下段とする(color の distinct 値の first-seen 順)。

### スコープ外(別 issue で追う)

- chart.js 側 `fill: 'stack'` 等の高度な塗り分け(`fulgur-chart-hdf`)
- 複数スタックグループ(`stackGroup`)(`fulgur-chart-nhb`)
- area + point マーカーの重ね描画

## Schema (`src/schema/vegalite.rs`)

`MarkLine*` と同型の `MarkArea*` を追加(裸文字列 `"area"` / `{"type":"area"}` を両方許可)。`VlTemporalAreaSpec` / `VlCategoricalAreaSpec` を新設し `VegaLiteSpec` enum に追加する。

`encoding.y.stack` は3状態を区別する:

| JSON | 意味 |
|---|---|
| キーなし | color があれば既定で積み上げ(`"zero"` 相当) |
| `"stack": "zero"` | 明示的に積み上げ |
| `"stack": null` | 積み上げ無効化 |

`null` と「キー省略」を区別する必要があるため、素の `Option<T>` ではなく専用の enum(`#[serde(default)]` で欠損時 zero 扱い、untagged で `null`/`"zero"` を判定)で実装する。serde の挙動は実装時に実機確認する。

例(カテゴリ x・積み上げ):
```json
{
  "mark": "area",
  "data": { "values": [
    {"month": "Jan", "kind": "A", "sales": 10},
    {"month": "Jan", "kind": "B", "sales": 15},
    {"month": "Feb", "kind": "A", "sales": 12},
    {"month": "Feb", "kind": "B", "sales": 9}
  ]},
  "encoding": {
    "x": {"field": "month", "type": "ordinal"},
    "y": {"field": "sales", "type": "quantitative"},
    "color": {"field": "kind", "type": "nominal"}
  }
}
```

## IR とフロントエンド変換

**IR (`src/ir.rs`)**: `ChartKind::Line`(unit variant)を `Line { stacked: bool }` に変更する。area かどうかは既存どおり `Series.area: bool` が担い、`stacked` はチャート全体に効く配置情報なので Bar と同じ置き場所にする。

`ChartKind::Line` にマッチする既存箇所(`layout/mod.rs`, `layout/common.rs` の複数箇所, `layout/line.rs`, テストの `spec.kind = ChartKind::Line` 代入)を `Line { .. }` / `Line { stacked: false }` へ機械的に直す。chart.js フロントエンド(`frontend/chartjs.rs`)は常に `stacked: false` を設定し、既存の line/area golden(SVG/PNG snapshot)は完全に不変とする。

**フロントエンド変換 (`src/frontend/vegalite.rs`)**:
- `parse_mark` に `"area"` を追加。`build_categorical` / `build_temporal_line` は area/line で共有し、`Series.area` フラグと `series_type` のみ切り替える。
- `stacked = color_field.is_some() && y.stack != Disabled`。line mark では常に `false`(本家 VL でも line は stack 対象外)。

**再利用性の裏付け(調査済み)**: `build_categorical`(カテゴリ×color の値行列、欠損は 0 埋め)と `build_temporal_line`(timestamp×color、欠損は明示エラーで拒否)は共に「全 series が全 x 位置で値を持つ」ことを既に保証している。積み上げ幾何計算側で新たな欠損穴埋めロジックを書く必要はない。

## 描画(layout)側

**値域計算 (`layout/common.rs::value_domain`)**: 既存の `ChartKind::Bar { value_stacked: true, .. }` 分岐(カテゴリごとの正値和/負値和で上下限)と同じロジックを `ChartKind::Line { stacked: true }` にも適用する。

**ジオメトリ (`layout/line.rs::build`)**: `stacked` のときはカテゴリごとの累積値を先に計算する:
- `bottom[k] = Σ series[0..idx].values[k]`
- `top[k] = bottom[k] + series[idx].values[k]`

面ポリゴンは「`top` を左→右」+「`bottom` を右→左」で閉じる(baseline 固定の現行 `append_area_points` とは別経路)。線(stroke)とマーカーは `top` の位置に描画する(本家 Vega-Lite / chart.js の見た目と一致)。積み上げ時は全系列が全カテゴリで値を持つ前提により、gap 分割・decimation・step mode の扱いは大幅に単純化できる。既存の非積み上げ経路とどこまでコード共有するかは実装時に整理する。

凡例・パレット色は `build_categorical`/`build_temporal_line` が既に付与するものを流用する。

## テスト計画

- schema 単体: `MarkArea*` の裸文字列/オブジェクト両形式、`y.stack` の3状態。
- frontend 単体(`tests/frontend_vegalite.rs`): カテゴリ/temporal area の積み上げ有無、`color` 省略時は常に非積み上げ、`stack: null` で明示解除、単系列 area は従来の line 経路と同一結果。
- layout/snapshot(`tests/render_line.rs` または新規 `render_vega_area.rs`): 積み上げ面グラフの SVG snapshot、非積み上げ area との視覚差分、`value_domain` が積み上げ高さを正しく反映すること。
- 回帰確認: 既存 line/area(chart.js 経路)の golden が完全不変。`cargo test -p fulgur-chart` 全体 green + `cargo clippy -p fulgur-chart --all-targets` クリーン。
- examples: `examples/specs/vegalite-area-stacked.json` を追加。

## 不採用案

- **stacked を per-series フラグにする案**: chart 全体に効く配置情報なので Bar と同様 ChartKind 側に置くべきで、不採用。
- **`y.stack` を独自キー名にする案**(serde の null/absent 判別を避けるため): 本家 Vega-Lite 準拠を優先するとの方針(ユーザー確認済み)により不採用。
