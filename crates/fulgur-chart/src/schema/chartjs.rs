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
    Doughnut(PieSpec),
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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BarOrLine {
    Bar,
    Line,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarOptions {
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
pub enum IndexAxis {
    X,
    Y,
}

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
    pub scales: Option<BarScales>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<PieDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PieDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XYPoint {
    pub x: f64,
    pub y: f64,
}

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
    pub data: Vec<XYRPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

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
    pub labels: Vec<String>,
    pub datasets: Vec<RadarDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadarDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
