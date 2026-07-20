//! JSON Schema types for the chart.js v4 input DSL.
//!
//! [`ChartJsSpec`] is the root type emitted by `fulgur-chart schema --dsl chartjs`.
//! It is a `#[serde(tag = "type")]` discriminated union so that each chart kind
//! exposes only the data and options fields that are valid for it.

use super::common::{
    AxisOptions, ColorString, DataLabelsPlugin, DecimationPlugin, LegendPlugin, ScalarOrArray,
    ThemeOptions, TitlePlugin,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────
// Top-level discriminated union
// ────────────────────────────────────────────────

/// Root type for a chart.js v4 compatible spec accepted by fulgur-chart.
/// The `"type"` field selects the per-chart-kind schema variant.
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
    Matrix(MatrixSpec),
    Treemap(TreemapSpec),
    /// QuickChart's canonical name is `progressBar`; `progress` is accepted too.
    #[serde(alias = "progressBar")]
    Progress(ProgressSpec),
    Boxplot(BoxplotSpec),
    /// QuickChart-compatible sparkline: minimal line chart with no axes, labels, or legend.
    Sparkline(SparklineSpec),
    Gauge(GaugeSpec),
    #[serde(rename = "polarArea")]
    PolarArea(PieSpec),
    #[serde(rename = "radialGauge")]
    RadialGauge(RadialGaugeSpec),
    #[serde(rename = "outlabeledPie")]
    OutlabeledPie(OutlabeledPieSpec),
    #[serde(rename = "outlabeledDoughnut")]
    OutlabeledDoughnut(OutlabeledPieSpec),
    #[serde(rename = "wordCloud")]
    WordCloud(WordCloudSpec),
    Sankey(SankeySpec),
}

// ────────────────────────────────────────────────
// Bar chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarSpec {
    pub data: BarData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BarOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<BarDataset>,
}

/// Bar dataset. Supports mixed bar+line charts via the per-dataset `type` field.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BarDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Per-dataset chart type for mixed bar+line charts. Only "bar" or "line" are valid.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub dataset_type: Option<BarOrLine>,
    pub data: Vec<Option<f64>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimation: Option<DecimationPlugin>,
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
// Line chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineSpec {
    pub data: LineData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<LineOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
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
    pub data: Vec<Option<f64>>,
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

/// Area fill setting: `true`/`false` or a mode string (e.g. `"origin"`).
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

/// Plugin options shared by chart types that have no scales (pie, doughnut, radar).
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CommonPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datalabels: Option<DataLabelsPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimation: Option<DecimationPlugin>,
}

/// gauge / radialGauge が受け付ける plugins(title のみ)。
/// 単一ゲージには凡例が描けないため legend は非公開(datalabels も非対応)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GaugePlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
}

/// progress バーには凡例が描けないため legend は非公開。datalabels は % 表示制御に使用。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ProgressPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datalabels: Option<DataLabelsPlugin>,
}

// ────────────────────────────────────────────────
// Pie and doughnut charts
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PieSpec {
    pub data: PieData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<PieOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
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
// Scatter chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScatterSpec {
    pub data: ScatterData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<XYOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
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
// Bubble chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BubbleSpec {
    pub data: BubbleData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<XYOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
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
// Radar chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarSpec {
    pub data: RadarData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<RadarOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
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

// ────────────────────────────────────────────────
// Matrix chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixSpec {
    pub data: MatrixData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<MatrixOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixData {
    pub datasets: Vec<MatrixDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatrixDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<MatrixPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixPoint {
    pub x: String,
    pub y: String,
    pub v: f64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<MatrixPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

/// matrix の plugins。`CommonPlugins` を流用すると `plugins.datalabels` を schema が受理するが、
/// matrix は datalabels を描画しない(parse_matrix は theme のみ消費)ため strict パーサ
/// (check_unknown_keys_matrix)は datalabels を弾く。schema 受理→strict 拒否の危険方向パリティ
/// 破れを避けて matrix 専用に定義し datalabels を契約から外す(sankey #87 と同型)。
/// title/legend/decimation は schema・strict とも受理する(decimation は no-op)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MatrixPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimation: Option<DecimationPlugin>,
}

// ────────────────────────────────────────────────
// Treemap chart (QuickChart / chartjs-chart-treemap)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreemapSpec {
    pub data: TreemapData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<TreemapOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreemapData {
    pub datasets: Vec<TreemapDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TreemapDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Hierarchical data: either a flat array of numbers, or an array of objects
    /// grouped via `key` + `groups`.
    pub tree: TreemapTree,
    /// Numeric property name to sum. Required when `tree` holds objects (strict
    /// parsing errors without it); ignored for a flat numeric `tree`. Modeled as
    /// optional here because the untagged number/object `tree` cannot express the
    /// conditional requirement in JSON Schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Grouping property names, outermost first, defining the hierarchy levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
    // treemap は palette/depth ベースで配色し、dataset レベルの backgroundColor や
    // border(stroke)は honor しないため、これらのオプションは公開しない。
}

/// `tree`: flat numeric array, or an array of data objects (grouped via `key`/`groups`).
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TreemapTree {
    Numbers(Vec<f64>),
    Objects(Vec<serde_json::Map<String, serde_json::Value>>),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreemapOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<TreemapPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

/// treemap は凡例を描かない(各矩形がラベルを持つ)ため legend は公開しない。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TreemapPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
}

// ────────────────────────────────────────────────
// Progress bar chart (QuickChart-compatible)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressSpec {
    pub data: ProgressData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ProgressOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressData {
    /// Per-bar names, one per value in the first dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// First dataset = per-bar values; optional second dataset = per-bar max (default 100).
    pub datasets: Vec<ProgressDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<ProgressPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

// ────────────────────────────────────────────────
// Boxplot chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxplotSpec {
    pub data: BoxplotData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BoxplotOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxplotData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<BoxplotDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoxplotDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Each data point is `[min, q1, median, q3, max]` — a five-number summary.
    pub data: Vec<Option<Vec<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxplotOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

// ────────────────────────────────────────────────
// Sparkline chart (QuickChart-compatible)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparklineSpec {
    pub data: SparklineData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SparklineOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparklineData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub datasets: Vec<SparklineDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SparklineDataset {
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
}

/// sparkline が受け付ける plugins。sparkline は title/legend/datalabels を描画しないため
/// decimation のみ公開する（正直な最小 schema）。line と同じ巨大データ間引きを許可する。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SparklinePlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimation: Option<DecimationPlugin>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparklineOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<SparklinePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

// ────────────────────────────────────────────────
// Gauge chart (QuickChart chartjs-gauge: semicircle, zones + needle)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GaugeSpec {
    pub data: GaugeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<GaugeOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GaugeData {
    pub datasets: Vec<GaugeDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GaugeDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Needle value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Domain minimum (default 0; max = last threshold).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    /// Cumulative zone thresholds.
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GaugeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needle: Option<NeedleOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_label: Option<ValueLabelOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<GaugePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

/// 針のスタイル。針の形状(長さ・太さ)は QuickChart 実物に合わせた内部定数で固定して
/// おり、サイズ系オプション(*Percentage)はスキーマには公開しない。色のみ上書き可能。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NeedleOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueLabelOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
}

// ────────────────────────────────────────────────
// Radial gauge chart (QuickChart radial-gauge: full circle, fill-to-value)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialGaugeSpec {
    pub data: RadialGaugeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<RadialGaugeOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialGaugeData {
    pub datasets: Vec<RadialGaugeDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadialGaugeDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Single value [value].
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadialGaugeOptions {
    /// [min, max] domain (default [0, 100]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rounded_corners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_area: Option<CenterAreaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<GaugePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CenterAreaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
}

// ────────────────────────────────────────────────
// outlabeledPie / outlabeledDoughnut
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPieSpec {
    pub data: PieData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OutlabeledPieOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPieOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<OutlabeledPiePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPiePlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlabels: Option<OutlabelsPlugin>,
}

/// `chartjs-plugin-piechart-outlabels` 互換の引き出しラベル設定。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlabelsPlugin {
    /// ラベルテキストのテンプレート。%l=カテゴリ名, %v=値, %p=パーセント。
    /// デフォルト: "%l\n%p%"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// ラベル文字色。デフォルト: "white"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    /// ラベル背景色。省略時はスライス色を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
    /// 引き出し線の長さ(px)。デフォルト: 40
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stretch: Option<f64>,
}

// ────────────────────────────────────────────────
// WordCloud chart (QuickChart / chartjs-chart-wordcloud)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudSpec {
    pub data: WordCloudData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<WordCloudOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudData {
    pub labels: Vec<String>,
    pub datasets: Vec<WordCloudDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCloudDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Word sizes in pixels (same length as labels).
    pub data: Vec<f64>,
    /// Optional color(s) for words. Scalar = all same, array = per-word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<WordCloudElements>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<WordCloudPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudElements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<WordElementOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordElementOptions {
    /// Minimum rotation angle in degrees. Default: -90.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rotation: Option<f64>,
    /// Maximum rotation angle in degrees. Default: 0.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rotation: Option<f64>,
    /// Number of discrete rotation steps. Default: 2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_steps: Option<u32>,
    /// Padding around each word in pixels. Default: 2.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WordCloudPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
}

// ────────────────────────────────────────────────
// Sankey chart (QuickChart / chartjs-chart-sankey)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeySpec {
    pub data: SankeyData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SankeyOptions>,
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

/// sankey の options。`MatrixOptions` を流用すると `CommonPlugins` 経由で
/// `plugins.datalabels` を許してしまうが、strict パーサは title/legend のみ受理するため、
/// schema と parser の乖離(schema 受理→strict 拒否)を避けて sankey 専用に定義する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<SankeyPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

/// sankey が受理する plugins(title のみ)。legend は描画されないため契約から外す。
/// datalabels も持たない(strict パーサと一致)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyData {
    /// sankey は dataset がちょうど 1 個(parser の契約と一致)。
    #[schemars(length(min = 1, max = 1))]
    pub datasets: Vec<SankeyDataset>,
    /// chart.js 互換のため受理するが sankey では未使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

/// dataset.parsing による from/to/flow キー再マップ。
///
/// 指定したキーがある場合、入力 JSON の flow 要素はそのキーから値を読む。
/// 例: `parsing: { flow: "value" }` を与えると `{ from, to, value }` の形式で受理する。
/// 指定なしの場合は default キー名 (`from`/`to`/`flow`) を使う。
///
/// 注意: parsing 指定時は入力 JSON が本 schema の `SankeyFlow` と乖離する(schema 上は
/// 常に `from`/`to`/`flow` を要求している)。schema-driven なクライアントで parsing を
/// 使う場合、data 部分は事前検証を無効化する必要がある。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyParsing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
}

/// `parsing` は object または `false` を受理する。`false` は chartjs-chart-sankey の
/// 「remap しない」慣習で、fulgur-chart の内部 flow フォーマット({from,to,flow})が既に
/// 期待形なので parsing 未指定と等価に扱う。
///
/// variant 順に注意: `Keys` を先に置くことで空オブジェクト `{}` は `SankeyParsing`
/// (all-None) として通り、`Disabled` にフォールバックしない。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SankeyParsingSpec {
    Keys(SankeyParsing),
    Disabled(SankeyParsingDisabled),
}

/// `false` のみを受理するマーカ型。schema では `{"type":"boolean","const":false}` を
/// 出力し、schema-driven クライアントが `true` を送って runtime error になるのを防ぐ。
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
pub struct SankeyParsingDisabled;

impl TryFrom<bool> for SankeyParsingDisabled {
    type Error = &'static str;
    fn try_from(b: bool) -> Result<Self, Self::Error> {
        if b {
            Err("dataset.parsing accepts an object or `false`; `true` is not supported")
        } else {
            Ok(SankeyParsingDisabled)
        }
    }
}

impl From<SankeyParsingDisabled> for bool {
    fn from(_: SankeyParsingDisabled) -> Self {
        false
    }
}

impl JsonSchema for SankeyParsingDisabled {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SankeyParsingDisabled".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "boolean",
            "const": false,
            "description": "Explicit `false` disables parsing key remapping (equivalent to omitting `parsing`)."
        })
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<SankeyFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_from: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_to: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_mode: Option<SankeyColorModeOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_color_from: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_color_to: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ColorString>,
    // 寸法は [0, 32768](= DEFAULT_MAX_DIMENSION_PX)。parser の上限と一致させる。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 32768.0))]
    pub border_width: Option<f64>,
    /// ノードラベル色
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 32768.0))]
    pub node_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 32768.0))]
    pub node_padding: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_x: Option<SankeyModeXOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SankeySizeOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<std::collections::HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<std::collections::HashMap<String, u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsing: Option<SankeyParsingSpec>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyFlow {
    pub from: String,
    pub to: String,
    /// フロー量は非負(parser が flow < 0 を拒否するのに合わせる)。
    #[schemars(range(min = 0.0))]
    pub flow: f64,
    /// per-link 色上書き(shorthand): colorFrom/colorTo が個別に指定されない場合、
    /// この値が両端の stop 色として使われる。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    /// per-link の from 側 stop 色上書き。指定なしは dataset の colorFrom を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_from: Option<ColorString>,
    /// per-link の to 側 stop 色上書き。指定なしは dataset の colorTo を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_to: Option<ColorString>,
}

/// sankey の colorMode。enum 化することで schema が値を列挙制約し、タイポ(例 "form")を
/// schema・parser とも拒否できる(silent default を防ぐ)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SankeyColorModeOption {
    From,
    To,
    Gradient,
}

/// sankey の modeX(列配置モード)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SankeyModeXOption {
    Edge,
    Even,
}

/// sankey の size(ノード高さの算出方式)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SankeySizeOption {
    Min,
    Max,
}

#[cfg(test)]
mod tests {
    use super::{BarDataset, BoxplotDataset, ChartJsSpec, LineDataset};

    #[test]
    fn line_dataset_accepts_null_in_data() {
        let json = r#"{"data":[1,null,3]}"#;
        let d: LineDataset = serde_json::from_str(json).unwrap();
        assert_eq!(d.data, vec![Some(1.0), None, Some(3.0)]);
    }

    #[test]
    fn bar_dataset_accepts_null_in_data() {
        let json = r#"{"data":[10,null,30]}"#;
        let d: BarDataset = serde_json::from_str(json).unwrap();
        assert_eq!(d.data, vec![Some(10.0), None, Some(30.0)]);
    }

    #[test]
    fn boxplot_dataset_accepts_null_row() {
        let json = r#"{"data":[[1,2,3,4,5], null, [10,20,30,40,50]]}"#;
        let d: BoxplotDataset = serde_json::from_str(json).unwrap();
        assert_eq!(d.data.len(), 3);
        assert!(d.data[1].is_none());
    }

    /// sankey の dataset 契約(ちょうど 1 個)が生成 JSON Schema に minItems/maxItems=1
    /// として現れること。parser の `datasets.len() != 1` チェックと一致させ、schema 駆動の
    /// クライアントが 0/複数 dataset を事前に弾けるようにする。
    #[test]
    fn sankey_datasets_constrained_to_one_in_schema() {
        let schema = schemars::schema_for!(ChartJsSpec);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(
            json.contains("\"minItems\":1"),
            "sankey datasets に minItems:1 が必要"
        );
        assert!(
            json.contains("\"maxItems\":1"),
            "sankey datasets に maxItems:1 が必要"
        );
        // flow / nodeWidth 等の非負制約(parser と一致)が minimum として出ること。
        assert!(
            json.contains("\"minimum\":0"),
            "sankey の寸法/flow に minimum:0 が必要"
        );
        // 寸法上限(parser の MAX_DIMENSION_PX と一致)が maximum として出ること。
        assert!(
            json.contains("\"maximum\":32768"),
            "sankey の寸法に maximum:32768 が必要"
        );
    }
}
