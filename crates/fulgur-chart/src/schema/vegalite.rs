//! Vega-Lite サブセットの JSON Schema 型定義。

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
// mark 定数型
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
pub enum MarkArc {
    Arc,
}

// ────────────────────────────────────────────────
// 棒グラフ (mark: "bar")
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
// 折れ線グラフ (mark: "line")
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
    pub x: VlChannel,
    pub y: VlChannel,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<VlChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<VlChannel>,
}
