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
    /// QuickChart's canonical name is `progressBar`; `progress` is accepted too.
    #[serde(alias = "progressBar")]
    Progress(ProgressSpec),
    Boxplot(BoxplotSpec),
    /// QuickChart-compatible sparkline: minimal line chart with no axes, labels, or legend.
    Sparkline(SparklineSpec),
    Gauge(GaugeSpec),
    #[serde(rename = "radialGauge")]
    RadialGauge(RadialGaugeSpec),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
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
