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

/// options.plugins.decimation（Chart.js 互換）。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DecimationPlugin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<DecimationAlgorithmName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// decimation algorithm 名（Chart.js: "min-max" | "lttb"）。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DecimationAlgorithmName {
    MinMax,
    Lttb,
}

/// Chart.js の共通 font オブジェクト。v1 では size のみ描画に反映される。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// number | "bold" 等。v1 では受理のみ。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum AxisTitleAlign {
    Start,
    Center,
    End,
}

/// options.scales.<axis>.title (Chart.js 準拠, camelCase)。
/// padding/font.family などは受理のみで v1 未描画。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisTitleOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontSpec>,
    /// v1 では未使用(受理のみ)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<AxisTitleAlign>,
}

/// options.scales.<axis>.grid (Chart.js 準拠)。
/// tick_length/offset/color per-tick 配列などは v1 未描画(受理のみ)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridLineOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ScalarOrArray<ColorString>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<ScalarOrArray<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_on_chart_area: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_ticks: Option<bool>,
    /// v1 では未使用(受理のみ)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_length: Option<f64>,
    /// v1 では未使用(受理のみ)。chart.js は band 中心/端で grid を描く挙動の切替。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<bool>,
}

/// options.scales.<axis>.border (Chart.js 準拠)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisBorderOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<Vec<f64>>,
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
    /// When true, category points/bands are centered (band center) instead of edge-to-edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_max: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_title_options_accepts_full_shape() {
        let v: AxisTitleOptions = serde_json::from_str(
            r##"{"display":true,"text":"Y (円)","color":"#333","font":{"size":14},"align":"center"}"##,
        ).unwrap();
        assert_eq!(v.text.as_deref(), Some("Y (円)"));
        assert!(matches!(v.align, Some(AxisTitleAlign::Center)));
    }

    #[test]
    fn grid_line_options_rejects_unknown_key() {
        let e = serde_json::from_str::<GridLineOptions>(r##"{"colorx":"#eee"}"##);
        assert!(e.is_err(), "unknown key must be rejected");
    }

    #[test]
    fn axis_border_options_accepts_dash_array() {
        let v: AxisBorderOptions =
            serde_json::from_str(r##"{"color":"#000","width":2,"dash":[4,4]}"##).unwrap();
        assert_eq!(v.dash.as_deref(), Some(&[4.0, 4.0][..]));
    }
}
