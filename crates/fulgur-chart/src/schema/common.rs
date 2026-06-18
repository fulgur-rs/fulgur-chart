//! Common schema types shared by the chartjs and vegalite frontends.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A CSS color string (e.g. "#rrggbb", "rgba(...)"). Represented as a plain string in the schema.
pub type ColorString = String;

/// A value that can be either a single item or an array.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ScalarOrArray<T> {
    One(T),
    Many(Vec<T>),
}

/// Visual token overrides (options.theme).
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
    /// Base font size for labels in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
}

/// options.plugins.title configuration.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TitlePlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// options.plugins.legend configuration.
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

/// options.plugins.datalabels configuration.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DataLabelsPlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
}

/// Axis options for options.scales.x / options.scales.y.
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Axis title configuration (parsed but not yet mapped to IR).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<serde_json::Value>,
    /// Grid line configuration (parsed but not yet mapped to IR).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begin_at_zero: Option<bool>,
}
