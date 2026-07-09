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
