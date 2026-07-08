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
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkBar {
    Bar,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkLine {
    Line,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkPoint {
    Point,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkCircle {
    Circle,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkArc {
    Arc,
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
// point mark の常に塗りつぶし円バリアント。`shape` フィールドは意図的に持たない
// ため、将来 point mark に shape エンコーディングが加わっても circle は shape
// 非対応のまま構造的に保たれる。
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
