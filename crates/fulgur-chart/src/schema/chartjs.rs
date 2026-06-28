//! JSON Schema types for the chart.js v4 input DSL.
//!
//! [`ChartJsSpec`] is the root type emitted by `fulgur-chart schema --dsl chartjs`.
//! It is a `#[serde(tag = "type")]` discriminated union so that each chart kind
//! exposes only the data and options fields that are valid for it.

use super::common::{
    AxisOptions, ColorString, DataLabelsPlugin, LegendPlugin, ScalarOrArray, ThemeOptions,
    TitlePlugin,
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
// Line chart
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
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
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
    pub data: Vec<Vec<f64>>,
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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparklineOptions {
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
    /// キャンバス幅 (px)。省略時は fulgur のデフォルト。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// キャンバス高さ (px)。省略時は fulgur のデフォルト。
    #[serde(skip_serializing_if = "Option::is_none")]
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
    pub options: Option<MatrixOptions>, // plugins(title/legend) + theme を共用
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyData {
    pub datasets: Vec<SankeyDataset>,
    /// chart.js 互換のため受理するが sankey では未使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
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
    /// "from" | "to" | "gradient"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    /// ノードラベル色
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_padding: Option<f64>,
    /// "edge" | "even"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_x: Option<String>,
    /// "min" | "max"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<std::collections::HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<std::collections::HashMap<String, u32>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyFlow {
    pub from: String,
    pub to: String,
    pub flow: f64,
}
