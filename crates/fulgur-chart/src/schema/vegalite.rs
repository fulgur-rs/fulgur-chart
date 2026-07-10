//! JSON Schema types for the Vega-Lite subset input DSL.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Root type for a Vega-Lite subset spec accepted by fulgur-chart.
/// The `mark` field value selects the per-chart-kind schema variant.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VegaLiteSpec {
    Bar(VlBarSpec),
    Line(VlLineSpec),
    Point(VlPointSpec),
    Circle(VlCircleSpec),
    Arc(VlArcSpec),
    Rect(VlRectSpec),
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

/// An encoding channel: a data field reference with an optional type hint.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlChannel {
    /// Name of the field in each record of data.values.
    pub field: String,
    /// Type hint (e.g. "quantitative", "nominal"). Currently has no effect on rendering.
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
#[serde(deny_unknown_fields)]
pub struct MarkLineObject {
    #[serde(rename = "type")]
    pub mark_type: MarkLineName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkLine {
    String(MarkLineName),
    Object(MarkLineObject),
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
// Line chart (mark: "line")
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlLineSpec {
    pub mark: MarkLine,
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

/// rect の color チャネル。基本の `field`/`type` に加え、`aggregate` を許容する。
/// `aggregate` は列挙で受理値を絞ることで schema と runtime の受理範囲を揃える
/// (以前は `Option<String>` で任意文字列が受理されていた)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectColorChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<VlRectAggregate>,
}
