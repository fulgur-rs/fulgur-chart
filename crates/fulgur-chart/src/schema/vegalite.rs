//! JSON Schema types for the Vega-Lite subset input DSL.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Root type for a Vega-Lite subset spec accepted by fulgur-chart.
/// The `mark` field value selects the per-chart-kind schema variant.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VegaLiteSpec {
    ErrorBar(VlErrorBarSpec),
    ErrorBand(VlErrorBandSpec),
    Bar(VlBarSpec),
    TemporalTrail(VlTemporalTrailSpec),
    CategoricalTrail(VlCategoricalTrailSpec),
    TemporalLine(VlTemporalLineSpec),
    CategoricalLine(VlCategoricalLineSpec),
    TemporalArea(VlTemporalAreaSpec),
    CategoricalArea(VlCategoricalAreaSpec),
    Point(VlPointSpec),
    Circle(VlCircleSpec),
    Square(VlSquareSpec),
    Arc(VlArcSpec),
    Rect(VlRectSpec),
    GeoShape(Box<VlGeoShapeSpec>),
}

// ────────────────────────────────────────────────
// Common helpers
// ────────────────────────────────────────────────

/// Inline data: an array of JSON objects (data.values).
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlData {
    pub values: Vec<serde_json::Value>,
}

/// GeoJSON data may be an inline record array, a Feature array, or one FeatureCollection.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<VlGeoDataFormat>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoDataFormat {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub format_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

/// An encoding channel: a data field reference with an optional type hint.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlChannel {
    /// Name of the field in each record of data.values.
    pub field: String,
    /// Type hint (e.g. "quantitative", "nominal"). Rendering infers types from data;
    /// point/square size additionally requires "quantitative" when this hint is present.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
}

/// Vega-Lite title: either a plain string or a `{text: ...}` object.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlTitle {
    Text(String),
    Obj { text: String },
}

// ────────────────────────────────────────────────
// Mark constant types
//
// Each mark comes in two forms: a bare string ("bar") and an object with
// a `type` key (`{"type": "bar"}`). The `Mark*Name` enums pin the accepted
// literal, and the `Mark*` untagged wrappers accept either form so the
// generated JSON Schema matches what `parse_mark` in frontend/vegalite.rs
// already accepts.
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkBarName {
    Bar,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkBarObject {
    #[serde(rename = "type")]
    pub mark_type: MarkBarName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkBar {
    String(MarkBarName),
    Object(MarkBarObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkLineName {
    Line,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlLineInterpolation {
    Linear,
    Monotone,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkLineObject {
    #[serde(rename = "type")]
    pub mark_type: MarkLineName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolate: Option<VlLineInterpolation>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkLine {
    String(MarkLineName),
    Object(MarkLineObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkTrailName {
    Trail,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkTrailObject {
    #[serde(rename = "type")]
    pub mark_type: MarkTrailName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkTrail {
    String(MarkTrailName),
    Object(MarkTrailObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkCategoricalLineObject {
    #[serde(rename = "type")]
    pub mark_type: MarkLineName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkCategoricalLine {
    String(MarkLineName),
    Object(MarkCategoricalLineObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkPointName {
    Point,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkPointObject {
    #[serde(rename = "type")]
    pub mark_type: MarkPointName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkPoint {
    String(MarkPointName),
    Object(MarkPointObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkCircleName {
    Circle,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkCircleObject {
    #[serde(rename = "type")]
    pub mark_type: MarkCircleName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkCircle {
    String(MarkCircleName),
    Object(MarkCircleObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkSquareName {
    Square,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkSquareObject {
    #[serde(rename = "type")]
    pub mark_type: MarkSquareName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkSquare {
    String(MarkSquareName),
    Object(MarkSquareObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkArcName {
    Arc,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkArcObject {
    #[serde(rename = "type")]
    pub mark_type: MarkArcName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkArc {
    String(MarkArcName),
    Object(MarkArcObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkRectName {
    Rect,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkRectObject {
    #[serde(rename = "type")]
    pub mark_type: MarkRectName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkRect {
    String(MarkRectName),
    Object(MarkRectObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkGeoShapeName {
    Geoshape,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkGeoShapeObject {
    #[serde(rename = "type")]
    pub mark_type: MarkGeoShapeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(rename = "strokeWidth", skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkGeoShape {
    String(MarkGeoShapeName),
    Object(MarkGeoShapeObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkAreaName {
    Area,
}

// `point`(area+point 重ね描き)は意図的に未対応(design doc 参照)。interpolate は
// temporal/categorical 共通の型として保持し、categorical area では builder が Linear 固定
// なので strict parser 側で追加制限する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkAreaObject {
    #[serde(rename = "type")]
    pub mark_type: MarkAreaName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolate: Option<VlLineInterpolation>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkArea {
    String(MarkAreaName),
    Object(MarkAreaObject),
}

// ────────────────────────────────────────────────
// Error bars and bands (composite marks)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkErrorBarName {
    Errorbar,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkErrorBandName {
    Errorband,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlErrorExtent {
    Stderr,
    Stdev,
    Ci,
    Iqr,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlErrorOrient {
    Horizontal,
    Vertical,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum VlErrorBandInterpolation {
    Linear,
    LinearClosed,
    Step,
    StepBefore,
    StepAfter,
    Basis,
    BasisOpen,
    BasisClosed,
    Cardinal,
    CardinalOpen,
    CardinalClosed,
    Bundle,
    Monotone,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorBarPartStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(rename = "strokeWidth", skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub stroke_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub opacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub size: Option<f64>,
    #[serde(rename = "strokeDash", skip_serializing_if = "Option::is_none")]
    pub stroke_dash: Option<Vec<f64>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorBandPartStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(rename = "strokeWidth", skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub stroke_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub opacity: Option<f64>,
    #[serde(rename = "strokeDash", skip_serializing_if = "Option::is_none")]
    pub stroke_dash: Option<Vec<f64>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlErrorBarPart {
    Flag(bool),
    Style(VlErrorBarPartStyle),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlErrorBandPart {
    Flag(bool),
    Style(VlErrorBandPartStyle),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkErrorBarObject {
    #[serde(rename = "type")]
    pub mark_type: MarkErrorBarName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<VlErrorExtent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orient: Option<VlErrorOrient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub opacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<VlErrorBarPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<VlErrorBarPart>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkErrorBar {
    String(MarkErrorBarName),
    Object(MarkErrorBarObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkErrorBandObject {
    #[serde(rename = "type")]
    pub mark_type: MarkErrorBandName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<VlErrorExtent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orient: Option<VlErrorOrient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub opacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<VlErrorBandPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub borders: Option<VlErrorBandPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolate: Option<VlErrorBandInterpolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub tension: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkErrorBand {
    String(MarkErrorBandName),
    Object(MarkErrorBandObject),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlErrorAxisType {
    Quantitative,
    Nominal,
    Ordinal,
    Temporal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorAxisChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlErrorAxisType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorRangeChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorColorFieldChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlCategoricalType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorColorValueChannel {
    pub value: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlErrorColorChannel {
    Field(VlErrorColorFieldChannel),
    Value(VlErrorColorValueChannel),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorOpacityChannel {
    #[schemars(range(min = 0.0, max = 1.0))]
    pub value: f64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorMarkEncoding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<VlErrorAxisChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<VlErrorAxisChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x2: Option<VlErrorRangeChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y2: Option<VlErrorRangeChannel>,
    #[serde(rename = "xError", skip_serializing_if = "Option::is_none")]
    pub x_error: Option<VlErrorRangeChannel>,
    #[serde(rename = "xError2", skip_serializing_if = "Option::is_none")]
    pub x_error2: Option<VlErrorRangeChannel>,
    #[serde(rename = "yError", skip_serializing_if = "Option::is_none")]
    pub y_error: Option<VlErrorRangeChannel>,
    #[serde(rename = "yError2", skip_serializing_if = "Option::is_none")]
    pub y_error2: Option<VlErrorRangeChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlErrorColorChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<VlErrorOpacityChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorBarSpec {
    pub mark: MarkErrorBar,
    pub data: VlData,
    pub encoding: VlErrorMarkEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlErrorBandSpec {
    pub mark: MarkErrorBand,
    pub data: VlData,
    pub encoding: VlErrorMarkEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

// ────────────────────────────────────────────────
// Bar chart (mark: "bar")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlBarSpec {
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
    pub x: VlChannel,
    pub y: VlChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
}

// ────────────────────────────────────────────────
// Temporal line chart (mark: "line")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalLineSpec {
    pub mark: MarkLine,
    pub data: VlData,
    pub encoding: VlTemporalLineEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalLineEncoding {
    pub x: VlTemporalXChannel,
    pub y: VlTemporalYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlTemporalColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlTemporalType {
    Temporal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlCategoricalType {
    Nominal,
    Ordinal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlQuantitativeType {
    Quantitative,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlStackMode {
    Zero,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalXChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: VlTemporalType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlColorScheme {
    Tableau10,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlColorScale {
    pub scheme: VlColorScheme,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalColorChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlCategoricalType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<VlColorScale>,
}

// ────────────────────────────────────────────────
// Categorical line chart (mark: "line")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalLineSpec {
    pub mark: MarkCategoricalLine,
    pub data: VlData,
    pub encoding: VlCategoricalLineEncoding,
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
pub struct VlCategoricalLineEncoding {
    pub x: VlCategoricalXChannel,
    pub y: VlCategoricalYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlCategoricalColorChannel>,
}

// ────────────────────────────────────────────────
// Trail chart (mark: "trail")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalTrailSpec {
    pub mark: MarkTrail,
    pub data: VlData,
    pub encoding: VlTemporalTrailEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalTrailEncoding {
    pub x: VlTemporalXChannel,
    pub y: VlTemporalYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlTemporalColorChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<VlTrailSizeChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalTrailSpec {
    pub mark: MarkTrail,
    pub data: VlData,
    pub encoding: VlCategoricalTrailEncoding,
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
pub struct VlCategoricalTrailEncoding {
    pub x: VlCategoricalXChannel,
    pub y: VlCategoricalYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlCategoricalColorChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<VlTrailSizeChannel>,
}

/// Quantitative trail size channel. Unlike point/square size, this maps to path width.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTrailSizeChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalXChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlCategoricalType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalColorChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlCategoricalType>,
}

// ────────────────────────────────────────────────
// Temporal area chart (mark: "area")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaSpec {
    pub mark: MarkArea,
    pub data: VlData,
    pub encoding: VlTemporalAreaEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaEncoding {
    pub x: VlTemporalXChannel,
    pub y: VlTemporalAreaYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlTemporalColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<VlStackMode>,
}

// ────────────────────────────────────────────────
// Categorical area chart (mark: "area")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalAreaSpec {
    pub mark: MarkArea,
    pub data: VlData,
    pub encoding: VlCategoricalAreaEncoding,
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
pub struct VlCategoricalAreaEncoding {
    pub x: VlCategoricalXChannel,
    pub y: VlCategoricalAreaYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlCategoricalColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalAreaYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<VlStackMode>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<VlViewConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<VlAxisConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlViewConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<()>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlAxisConfig {
    pub grid: Option<bool>,
    #[schemars(range(min = 0.0, max = 1.0))]
    pub grid_opacity: Option<f64>,
}

// ────────────────────────────────────────────────
// Scatter plot (mark: "point")
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
    pub x: VlChannel,
    pub y: VlChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<VlPointSizeChannel>,
}

/// Quantitative point/square size channel. Omitted `type` is inferred from the data.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlPointSizeChannel {
    /// Name of the numeric field mapped to point area.
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
}

// ────────────────────────────────────────────────
// Circle plot (mark: "circle")
//
// Always-filled-circle variant of the point mark. `shape` is intentionally
// omitted so that if point ever grows a shape channel, circle stays
// shape-free by structure.
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCircleSpec {
    pub mark: MarkCircle,
    pub data: VlData,
    pub encoding: VlCircleEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

/// Encoding for `mark: "circle"`. No `shape` channel — see section note.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCircleEncoding {
    pub x: VlChannel,
    pub y: VlChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
}

// ────────────────────────────────────────────────
// Square plot (mark: "square")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlSquareSpec {
    pub mark: MarkSquare,
    pub data: VlData,
    pub encoding: VlSquareEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

/// Encoding for `mark: "square"`. The mark shape is fixed; `size` maps to pixel area.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlSquareEncoding {
    pub x: VlChannel,
    pub y: VlChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<VlPointSizeChannel>,
}

// ────────────────────────────────────────────────
// Arc / pie chart (mark: "arc")
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<VlChannel>,
}

// ────────────────────────────────────────────────
// Rect / heatmap chart (mark: "rect")
//
// x/y はカテゴリ、color は quantitative(2色補間)または nominal(パレット割当)。
// encoding.color.aggregate は "mean" / "sum" 列挙で、schema と runtime の受理範囲を揃える。
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectSpec {
    pub mark: MarkRect,
    pub data: VlData,
    pub encoding: VlRectEncoding,
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
pub struct VlRectEncoding {
    pub x: VlRectAxisChannel,
    pub y: VlRectAxisChannel,
    pub color: VlRectColorChannel,
}

/// rect の x/y encoding が受理する type。quantitative は binned ヒートマップ
/// 想定で MVP 外のため、schema レベルで nominal / ordinal のみ許可する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlRectAxisType {
    Nominal,
    Ordinal,
}

/// rect の x/y チャネル。`field` は必須、`type` は nominal/ordinal のみ受理。
/// (`VlChannel` は `type` に任意文字列を許容するが、rect の軸では quantitative
/// は runtime で reject されるため、schema でも同じ範囲に絞っておく。)
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectAxisChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlRectAxisType>,
}

/// rect の color.aggregate に許容される集約方式。frontend が受理する "mean"/"sum" と
/// 対応する。runtime は `frontend::vegalite::check_unknown_keys` で同じ値だけを許可し、
/// 他値は strict モードで Err になる。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlRectAggregate {
    Mean,
    Sum,
}

/// rect の color チャネルのうち quantitative variant が受理する type。
/// `Option` と組合わせて `type` 省略も許容する (省略時は infer に委ねる)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlRectColorQuantitativeType {
    Quantitative,
}

/// rect の color チャネルのうち nominal / ordinal を表現する categorical variant の type。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlRectColorCategoricalType {
    Nominal,
    Ordinal,
}

/// rect の color チャネル (quantitative variant)。
/// `type` は省略可能 (省略時は infer)、`aggregate` は列挙で受理値を絞る。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectColorQuantitativeChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlRectColorQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<VlRectAggregate>,
}

/// rect の color チャネル (categorical variant)。
/// `type` は必須 (`nominal` / `ordinal`)、`aggregate` は許容しない
/// (categorical への aggregate は runtime で明示 Err になる)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectColorCategoricalChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: VlRectColorCategoricalType,
}

/// rect の color チャネル。
///
/// - Quantitative: `type` 省略 or `quantitative`、`aggregate` 許容。
/// - Categorical: `type` は `nominal` / `ordinal`、`aggregate` 不可。
///
/// `untagged` で外から見ると同じ JSON 形だが、"quantitative-with-aggregate XOR
/// nominal/ordinal-without-aggregate" の invariant を schema レベルで表現する。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlRectColorChannel {
    Quantitative(VlRectColorQuantitativeChannel),
    Categorical(VlRectColorCategoricalChannel),
}

// ────────────────────────────────────────────────
// GeoJSON geoshape and projection
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlGeoShapeType {
    Geojson,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoShapeFieldChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlGeoShapeType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlGeoColorType {
    Quantitative,
    Nominal,
    Ordinal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoColorFieldChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlGeoColorType>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoColorValueChannel {
    pub value: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlGeoColorChannel {
    Field(VlGeoColorFieldChannel),
    Value(VlGeoColorValueChannel),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoShapeEncoding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<VlGeoShapeFieldChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlGeoColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum VlProjectionType {
    Albers,
    AlbersUsa,
    AzimuthalEqualArea,
    AzimuthalEquidistant,
    ConicConformal,
    ConicEqualArea,
    ConicEquidistant,
    EqualEarth,
    Equirectangular,
    Gnomonic,
    Identity,
    Mercator,
    NaturalEarth1,
    Orthographic,
    Stereographic,
    TransverseMercator,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VlProjectionRotate {
    Two([f64; 2]),
    Three([f64; 3]),
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlProjection {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub projection_type: Option<VlProjectionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<VlProjectionRotate>,
    #[serde(rename = "clipAngle", skip_serializing_if = "Option::is_none")]
    pub clip_angle: Option<f64>,
    #[serde(rename = "clipExtent", skip_serializing_if = "Option::is_none")]
    pub clip_extent: Option<[[f64; 2]; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallels: Option<[f64; 2]>,
    #[serde(rename = "pointRadius", skip_serializing_if = "Option::is_none")]
    pub point_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<[f64; 2]>,
    #[serde(rename = "reflectX", skip_serializing_if = "Option::is_none")]
    pub reflect_x: Option<bool>,
    #[serde(rename = "reflectY", skip_serializing_if = "Option::is_none")]
    pub reflect_y: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlGeoShapeSpec {
    pub mark: MarkGeoShape,
    pub data: VlGeoData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<VlGeoShapeEncoding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<VlProjection>,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}
