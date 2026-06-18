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
