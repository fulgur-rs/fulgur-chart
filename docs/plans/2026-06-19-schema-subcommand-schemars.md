# schema サブコマンド schemars 化実装計画

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `schemars` の `#[derive(JsonSchema)]` を使い、chartjs/vegalite の入力 DSL を union 型で正確に表現した JSON Schema を自動生成する。

**Architecture:** `fulgur-chart` クレートに `pub mod schema` モジュールを追加し、チャート種別ごとに専用の data/options 構造体を定義した discriminated union 型を置く。CLI の `run_schema` はその型を `schemars::schema_for!()` に渡して出力する。手書き文字列定数は削除する。

**Tech Stack:** Rust 2024、schemars 1.2、serde/serde_json、clap 4

---

## 設計メモ

### chartjs: `#[serde(tag = "type")]` による discriminated union

```
ChartJsSpec (enum, tag = "type")
├── "bar"      → BarSpec      { data: BarData,      options?: BarOptions }
├── "line"     → LineSpec     { data: LineData,      options?: LineOptions }
├── "pie"      → PieSpec      { data: PieData,      options?: PieOptions }
├── "doughnut" → DoughnutSpec { data: PieData,      options?: PieOptions }
├── "scatter"  → ScatterSpec  { data: ScatterData,  options?: XYOptions }
├── "bubble"   → BubbleSpec   { data: BubbleData,   options?: XYOptions }
└── "radar"    → RadarSpec    { data: RadarData,    options?: RadarOptions }
```

各チャートの data.datasets のアイテム型も専用化:
- Bar/Line/Radar: `data: number[]`
- Scatter: `data: {x, y}[]`
- Bubble: `data: {x, y, r}[]`

Bar の dataset は `"type": "bar" | "line"` で混合チャートに対応。

Bar の options のみ `indexAxis` と `scales.{x,y}.stacked` を持つ。
Pie/Doughnut の options は `scales` を持たない。

### vegalite: `mark` による discriminated union

```
VegaLiteSpec (enum, serde untagged over mark string)
```

vegalite は `mark` がトップレベルの文字列なので `#[serde(tag)]` を直接使えない。
代わりに各バリアントが `mark: Const<"bar">` 等の const string フィールドを持つ
`oneOf` として表現する。

実装方針: `schemars::transform` か、各バリアントに `mark` フィールドを直接持たせる
untagged enum を使い、schemars の `schema_with` で `const` 制約を付ける。

シンプルに行くなら各バリアントの struct に `mark: BarMark` (enum with single variant)
を含める方式が最もメンテしやすい。

---

## Task 1: schemars を依存に追加

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/fulgur-chart/Cargo.toml`

**Step 1:** workspace の `[workspace.dependencies]` に追記

```toml
schemars = { version = "1.2", features = ["preserve_order"] }
```

**Step 2:** `crates/fulgur-chart/Cargo.toml` の `[dependencies]` に追記

```toml
schemars = { workspace = true }
```

**Step 3:** ビルドが通ることを確認

```bash
cargo build --package fulgur-chart
```

Expected: コンパイル成功

**Step 4:** コミット

```bash
git add Cargo.toml crates/fulgur-chart/Cargo.toml Cargo.lock
git commit -m "chore: add schemars 1.2 dependency"
```

---

## Task 2: schema モジュールの骨格と共通型を作成

**Files:**
- Create: `crates/fulgur-chart/src/schema/mod.rs`
- Create: `crates/fulgur-chart/src/schema/common.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1:** `lib.rs` に `pub mod schema;` を追加

```rust
pub mod schema;
```

**Step 2:** `schema/mod.rs` を作成（再エクスポートのみ）

```rust
pub mod chartjs;
pub mod vegalite;
pub use chartjs::ChartJsSpec;
pub use vegalite::VegaLiteSpec;
```

**Step 3:** `schema/common.rs` に共通型を作成

```rust
//! chartjs / vegalite 双方で使う共通スキーマ型。
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// CSS 色文字列（"#rrggbb"、"rgba(...)" 等）。スキーマ上は string。
pub type ColorString = String;

/// スカラまたは配列。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ScalarOrArray<T> {
    One(T),
    Many(Vec<T>),
}

/// options.theme に対応する視覚トークン上書きオブジェクト。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<Vec<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
    /// ラベル基準フォントサイズ(px)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
}

/// plugins.title。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TitlePlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// plugins.legend。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LegendPlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<LegendPosition>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LegendPosition {
    Top,
    Bottom,
    Left,
    Right,
}

/// plugins.datalabels。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DataLabelsPlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
}

/// options.scales.{x,y} の軸オプション。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// 軸タイトル設定（現在 IR 未マップ）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<serde_json::Value>,
    /// グリッド設定（現在 IR 未マップ）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begin_at_zero: Option<bool>,
}
```

**Step 4:** ビルド確認

```bash
cargo build --package fulgur-chart
```

Expected: 成功（schema/chartjs.rs・schema/vegalite.rs が存在しないためエラーになる。
Task 3 で作成するまでは `mod.rs` の `pub mod chartjs;` 等をコメントアウトしておく）

**Step 5:** コミット

```bash
git add crates/fulgur-chart/src/schema/ crates/fulgur-chart/src/lib.rs
git commit -m "feat(schema): add schema module skeleton with common types"
```

---

## Task 3: chartjs スキーマ型を実装（共通 dataset フィールド + カテゴリ系）

**Files:**
- Create: `crates/fulgur-chart/src/schema/chartjs.rs`

chart ごとの dataset・options 型を定義する。ファイルが長くなるが、
型をまとめると IDE のジャンプ・検索が容易なので 1 ファイルにする。

**Step 1:** `schema/chartjs.rs` を作成（bar/line/radar 向けの型から）

```rust
//! chart.js v4 spec の JSON Schema 型定義。
//!
//! [`ChartJsSpec`] が `fulgur-chart schema --dsl chartjs` で出力するトップ型。
//! `#[serde(tag = "type")]` による discriminated union で、chart 種別ごとに
//! 有効な data・options のみを保持する。

use super::common::{
    AxisOptions, ColorString, DataLabelsPlugin, LegendPlugin, ScalarOrArray, ThemeOptions,
    TitlePlugin,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────
// トップレベルの discriminated union
// ────────────────────────────────────────────────

/// fulgur-chart が受け付ける chart.js v4 互換 spec のルート型。
/// `"type"` フィールドで各チャート種別の専用スキーマに分岐する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ChartJsSpec {
    Bar(BarSpec),
    Line(LineSpec),
    Pie(PieSpec),
    Doughnut(PieSpec),       // Doughnut は Pie と同一構造
    Scatter(ScatterSpec),
    Bubble(BubbleSpec),
    Radar(RadarSpec),
}

// ────────────────────────────────────────────────
// 棒グラフ (bar)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarSpec {
    pub data: BarData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BarOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarData {
    /// カテゴリ軸ラベル。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<BarDataset>,
}

/// bar/line 混合チャート対応。`type` で系列ごとの描画種別を切り替える。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BarDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 混合チャート用。"bar" または "line" のみ有効。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub dataset_type: Option<BarOrLine>,
    /// カテゴリ軸の値。labels と同じ長さ。
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    /// line 系列として描画する場合のテンション(0=直線)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    /// line 系列として描画する場合の塗りつぶし。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillSpec>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BarOrLine { Bar, Line }

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarOptions {
    /// "y" を指定すると横棒グラフになる。
    #[serde(rename = "indexAxis", skip_serializing_if = "Option::is_none")]
    pub index_axis: Option<IndexAxis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<BarPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IndexAxis { X, Y }

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datalabels: Option<DataLabelsPlugin>,
}

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BarScales {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<AxisOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<AxisOptions>,
}

// ────────────────────────────────────────────────
// 折れ線グラフ (line)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineSpec {
    pub data: LineData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<LineOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<LineDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point_radius: Option<f64>,
}

/// line のエリア塗り指定。true/false またはモード文字列("origin" 等)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FillSpec {
    Bool(bool),
    Mode(String),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,  // 再利用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

/// plugins: title / legend / datalabels（scales なし種別で共用）。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CommonPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datalabels: Option<DataLabelsPlugin>,
}

// ────────────────────────────────────────────────
// 円グラフ / ドーナツ (pie / doughnut)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PieSpec {
    pub data: PieData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<PieOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PieData {
    /// スライスラベル。datasets[0].data と同じ長さ。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<PieDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PieDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 各スライスの値。
    pub data: Vec<f64>,
    /// スライスごとの色（配列推奨）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

/// pie/doughnut の options。scales を持たない。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PieOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

// ────────────────────────────────────────────────
// 散布図 (scatter)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScatterSpec {
    pub data: ScatterData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<XYOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScatterData {
    pub datasets: Vec<ScatterDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScatterDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 各点の座標。
    pub data: Vec<XYPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point_radius: Option<f64>,
}

/// scatter 用の点座標。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XYPoint {
    pub x: f64,
    pub y: f64,
}

/// scatter / bubble 共用の options。indexAxis・stacked は不要。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XYOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

// ────────────────────────────────────────────────
// バブルチャート (bubble)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BubbleSpec {
    pub data: BubbleData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<XYOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BubbleData {
    pub datasets: Vec<BubbleDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BubbleDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 各バブルの座標と半径。
    pub data: Vec<XYRPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

/// bubble 用の点座標 + 半径。`r` 省略時はデフォルト半径を使う。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XYRPoint {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
}

// ────────────────────────────────────────────────
// レーダーチャート (radar)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarSpec {
    pub data: RadarData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<RadarOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarData {
    /// スポーク(軸)ラベル。各系列の values と同じ長さ。
    pub labels: Vec<String>,
    pub datasets: Vec<RadarDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadarDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 非負値のみ。負値はエラー。
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillSpec>,
}

/// radar の options。indexAxis/stacked/scales は不要。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
```

**Step 2:** ビルド確認

```bash
cargo build --package fulgur-chart
```

Expected: 成功

**Step 3:** コミット

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "feat(schema): add chartjs discriminated union schema types"
```

---

## Task 4: vegalite スキーマ型を実装

**Files:**
- Create: `crates/fulgur-chart/src/schema/vegalite.rs`

vegalite は `mark` フィールドの値でチャート種別を区別する。
`#[serde(tag = "mark")]` は文字列フィールドとの相性が悪いため、
各バリアントに `mark` フィールドを直接持たせた untagged enum を使う。

**Step 1:** `schema/vegalite.rs` を作成

```rust
//! Vega-Lite サブセットの JSON Schema 型定義。

use super::common::ThemeOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// fulgur-chart が受け付ける Vega-Lite サブセット spec のルート型。
/// `mark` フィールドの値で各チャート種別に分岐する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VegaLiteSpec {
    Bar(VlBarSpec),
    Line(VlLineSpec),
    Point(VlPointSpec),
    Arc(VlArcSpec),
}

// ────────────────────────────────────────────────
// 共通ヘルパー
// ────────────────────────────────────────────────

/// data.values: インライン JSON オブジェクトの配列。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlData {
    pub values: Vec<serde_json::Value>,
}

/// エンコーディングチャネル（field + 省略可能な type ヒント）。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlChannel {
    /// data.values の各レコードで参照するフィールド名。
    pub field: String,
    /// 型ヒント("quantitative", "nominal" 等)。現在は動作に影響しない。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
}

/// VL の title: 文字列または {text: ...} オブジェクト。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlTitle {
    Text(String),
    Obj { text: String },
}

// ────────────────────────────────────────────────
// mark 定数型: schemars が const enum として出力するために 1 バリアントの enum を使う
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkBar   { Bar }

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkLine  { Line }

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkPoint { Point }

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkArc   { Arc }

// ────────────────────────────────────────────────
// 棒グラフ (mark: "bar")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlBarSpec {
    #[schemars(description = "チャート種別。\"bar\" 固定。")]
    pub mark: MarkBar,
    pub data: VlData,
    pub encoding: VlBarEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlBarEncoding {
    /// カテゴリ軸フィールド。
    pub x: VlChannel,
    /// 数値軸フィールド。
    pub y: VlChannel,
    /// 色分けフィールド（省略可）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
}

// ────────────────────────────────────────────────
// 折れ線グラフ (mark: "line")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlLineSpec {
    pub mark: MarkLine,
    pub data: VlData,
    pub encoding: VlBarEncoding,  // bar と同じ encoding 構造
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

// ────────────────────────────────────────────────
// 散布図 (mark: "point")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlPointSpec {
    pub mark: MarkPoint,
    pub data: VlData,
    pub encoding: VlPointEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlPointEncoding {
    /// X 軸数値フィールド。
    pub x: VlChannel,
    /// Y 軸数値フィールド。
    pub y: VlChannel,
    /// 色分けフィールド（省略可）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
}

// ────────────────────────────────────────────────
// 円グラフ (mark: "arc")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlArcSpec {
    pub mark: MarkArc,
    pub data: VlData,
    pub encoding: VlArcEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlArcEncoding {
    /// スライス値フィールド(theta 優先、なければ y)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<VlChannel>,
    /// カテゴリフィールド(color 優先、なければ x)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<VlChannel>,
}
```

**Step 2:** `mod.rs` の `pub mod chartjs;` / `pub mod vegalite;` コメントを外す

**Step 3:** ビルド確認

```bash
cargo build --package fulgur-chart
```

Expected: 成功

**Step 4:** コミット

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs crates/fulgur-chart/src/schema/mod.rs
git commit -m "feat(schema): add vegalite discriminated union schema types"
```

---

## Task 5: CLI を schemars 出力に切り替える

**Files:**
- Modify: `crates/fulgur-chart-cli/Cargo.toml`
- Modify: `crates/fulgur-chart-cli/src/main.rs`

**Step 1:** CLI の依存に `schemars` を追加（schema_for! マクロのため）

```toml
# crates/fulgur-chart-cli/Cargo.toml [dependencies]
schemars = { workspace = true }
```

**Step 2:** `run_schema` を schemars 出力に書き換え、文字列定数を削除

```rust
fn run_schema(args: SchemaArgs) {
    let json = match args.dsl.as_str() {
        "chartjs" => {
            let schema = schemars::schema_for!(fulgur_chart::schema::ChartJsSpec);
            serde_json::to_string_pretty(&schema).expect("schema serialization failed")
        }
        "vegalite" => {
            let schema = schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec);
            serde_json::to_string_pretty(&schema).expect("schema serialization failed")
        }
        other => {
            eprintln!("error: unsupported DSL '{other}' (supported: chartjs, vegalite)");
            std::process::exit(1);
        }
    };
    println!("{json}");
}
```

`SCHEMA_CHARTJS` と `SCHEMA_VEGALITE` 定数を完全に削除する。

**Step 3:** ビルド確認

```bash
cargo build --package fulgur-chart-cli
```

Expected: 成功

**Step 4:** 動作確認

```bash
# 有効な JSON が出力されるか
cargo run --package fulgur-chart-cli -- schema | python3 -m json.tool > /dev/null && echo "OK"
cargo run --package fulgur-chart-cli -- schema --dsl vegalite | python3 -m json.tool > /dev/null && echo "OK"
cargo run --package fulgur-chart-cli -- schema --dsl unknown; echo "exit: $?"
```

Expected:
```
OK
OK
error: unsupported DSL 'unknown' (supported: chartjs, vegalite)
exit: 1
```

**Step 5:** コミット

```bash
git add crates/fulgur-chart-cli/Cargo.toml crates/fulgur-chart-cli/src/main.rs Cargo.lock
git commit -m "feat(schema): switch CLI schema output to schemars-derived types"
```

---

## Task 6: CLI テストを追加

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1:** 既存テストの末尾に以下を追加

```rust
#[test]
fn schema_chartjs_is_valid_json() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
    // $schema フィールドが存在する
    assert!(v.get("$schema").is_some(), "missing $schema");
    // oneOf が存在する（discriminated union）
    let has_one_of = v.get("oneOf").is_some()
        || v.get("anyOf").is_some()
        || v.get("allOf").is_some();
    assert!(has_one_of, "expected union schema");
}

#[test]
fn schema_vegalite_is_valid_json() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema", "--dsl", "vegalite"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let _: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
}

#[test]
fn schema_unknown_dsl_exits_1() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema", "--dsl", "unknown"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}
```

**Step 2:** テスト実行

```bash
cargo test --package fulgur-chart-cli schema
```

Expected: 3 tests pass

**Step 3:** コミット

```bash
git add crates/fulgur-chart-cli/tests/cli.rs
git commit -m "test: add schema subcommand integration tests"
```

---

## Task 7: 全テストを通す

**Step 1:** 全テストを実行

```bash
cargo test
```

Expected: 全テスト pass

**Step 2:** 問題があれば修正してコミット

```bash
# 問題がなければ追加コミット不要
# 修正があれば:
git add <changed files>
git commit -m "fix: <what you fixed>"
```

---

## 注意事項・既知の課題

### schemars と serde の属性の互換性

- `#[serde(deny_unknown_fields)]` は schemars 1.x で `additionalProperties: false` として出力される
- `#[serde(tag = "type")]` は internally tagged enum として `oneOf` を生成する
- `#[serde(untagged)]` は vegalite の mark union に使用

### serde_json::Value フィールド

`AxisOptions.title` / `AxisOptions.grid` は `serde_json::Value` 型のため、
schemars は `{}` (任意オブジェクト) として出力する。これは意図した動作。

### `#[serde(rename = "$schema")]` 

`$schema` フィールドは serde の rename で対処できる。
schemars も rename を尊重するので特別な対処は不要。

### vegalite の mark が object 形式の場合

Vega-Lite の mark は `"bar"` 文字列だけでなく `{"type": "bar"}` オブジェクトも受け付ける。
現在の実装では文字列形式のみスキーマに含める。
オブジェクト形式が重要なら後の issue で対応する。
