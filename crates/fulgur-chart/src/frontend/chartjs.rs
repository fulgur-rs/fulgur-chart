//! chart.js v4 spec のデータ専用・静的サブセットを IR へ変換する。

use crate::color::parse_color;
use crate::ir::*;
use crate::schema::chartjs::{
    BarThickness as SchemaBarThickness, BorderRadius as SchemaBorderRadius, CubicMode,
    DatasetPointStyle as SchemaPointStyle, SchemaArcBorderRadius,
};
use crate::schema::common::{
    AxisBorderOptions, AxisOptions, AxisTitleAlign as SchemaAxisTitleAlign, AxisTitleOptions,
    GridLineOptions, LegendPointStyle as SchemaLegendPointStyle,
};
use serde::Deserialize;
use std::collections::HashMap;

/// top-level `width`/`height` 省略時の既定キャンバスサイズ(px)。
/// wordCloud のみ専用既定(500x300)を使うため、ここには含めない。
const DEFAULT_CHART_WIDTH: f64 = 800.0;
const DEFAULT_CHART_HEIGHT: f64 = 450.0;

/// Deserialize a field that may be explicitly `null`, treating `null` (and a missing
/// field via `#[serde(default)]`) as `T::default()`. Keeps parser tolerance aligned with
/// schemas that render optional fields as nullable.
fn null_or_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct RawSpec {
    #[serde(rename = "type")]
    chart_type: String,
    data: RawData,
    #[serde(default)]
    options: RawOptions,
    // fulgur 拡張: トップレベル width/height(px)。省略時は既定 800x450。
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RawOptions {
    #[serde(rename = "indexAxis")]
    index_axis: Option<String>,
    /// Pie-only; retain raw JSON until the chart type is known so other kinds stay tolerant.
    #[serde(default)]
    cutout: Option<serde_json::Value>,
    // Accept an explicit `options.plugins: null` as the default (schemas render it nullable).
    #[serde(default, deserialize_with = "null_or_default")]
    plugins: RawPlugins,
    #[serde(default)]
    theme: Option<RawTheme>,
    // scales.<index 軸>.stacked → placement_stacked(配置)、<値軸>.stacked → value_stacked(値域・累積)。
    // 型付きにすることで、schema 側 `AxisOptions` / `AxisTitleOptions` /
    // `GridLineOptions` / `AxisBorderOptions` の `deny_unknown_fields` が sub-object
    // レベルのタイポ(例: `title.txt` / `border.colorr`)を deserialize 段で拒否する。
    #[serde(default)]
    scales: Option<RawScales>,
}

/// `options.scales` の受け皿。x/y の直交 2 軸と r の動径軸を typed に扱う。
/// `deny_unknown_fields` は付けない: 非 strict モードでは `scales.x1`/`y1`(multi-axis)
/// を silently 無視する必要がある(Chart.js 互換)。
/// strict モードでの未知キー拒否は上流の `check_unknown_keys` が担当する。
#[derive(Deserialize, Default)]
struct RawScales {
    #[serde(default)]
    x: Option<AxisOptions>,
    #[serde(default)]
    y: Option<AxisOptions>,
    /// radar / polarArea の動径軸。
    ///
    /// ここで `RawRadialAxis` に型付けしてしまうと、chart kind が確定する前に検証が走る。
    /// その結果、非 radial チャートに紛れ込んだ無関係な `scales.r` (例 bar + `"r": 5`) が
    /// 非 strict モードでも deserialize エラーになってしまう —— main では未知キーとして
    /// silently 無視されていた挙動の後退。kind が分かるまで生の JSON 値のまま保持する。
    #[serde(default)]
    r: Option<serde_json::Value>,
}

/// `options.scales.r`(radar / polarArea の動径軸)のドメイン指定。
/// `deny_unknown_fields` は付けない: 非 strict では `ticks` / `angleLines` / `pointLabels`
/// 等の視覚キーを silently 無視する(Chart.js 互換)。strict は `check_unknown_keys` が担当。
/// 全フィールドが `Option` なので、どのキーも明示されなかった場合を呼び出し側で判別できる。
#[derive(Deserialize, Default)]
struct RawRadialAxis {
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(rename = "suggestedMin", default)]
    suggested_min: Option<f64>,
    #[serde(rename = "suggestedMax", default)]
    suggested_max: Option<f64>,
    #[serde(rename = "beginAtZero", default)]
    begin_at_zero: Option<bool>,
}

/// `options.theme`: 視覚トークンの上書き。各フィールドは任意。
#[derive(Deserialize)]
struct RawTheme {
    #[serde(default)]
    palette: Option<Vec<String>>,
    #[serde(rename = "gridColor", default)]
    grid_color: Option<String>,
    #[serde(rename = "textColor", default)]
    text_color: Option<String>,
    #[serde(rename = "backgroundColor", default)]
    background_color: Option<String>,
    #[serde(rename = "fontSize", default)]
    font_size: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RawPlugins {
    title: Option<RawTitle>,
    legend: Option<RawLegend>,
    datalabels: Option<RawDataLabels>,
    outlabels: Option<RawOutlabels>,
    decimation: Option<RawDecimation>,
}

#[derive(Deserialize)]
struct RawDataLabels {
    #[serde(default)]
    display: Option<bool>,
}

#[derive(Deserialize)]
struct RawDecimation {
    enabled: Option<bool>,
    algorithm: Option<String>,
    samples: Option<f64>,
    threshold: Option<f64>,
}

#[derive(Deserialize)]
struct RawOutlabels {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(rename = "backgroundColor", default)]
    background_color: Option<String>,
    #[serde(default)]
    stretch: Option<f64>,
}

#[derive(Deserialize)]
struct RawTitle {
    #[serde(default)]
    display: bool,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct RawLegend {
    #[serde(default = "default_true")]
    display: bool,
    position: Option<String>,
    align: Option<String>,
    reverse: Option<bool>,
    labels: Option<RawLegendLabels>,
    title: Option<RawLegendTitle>,
}

#[derive(Deserialize, Default)]
struct RawLegendLabels {
    color: Option<String>,
    font: Option<RawLegendFont>,
    padding: Option<f64>,
    #[serde(rename = "boxWidth")]
    box_width: Option<f64>,
    #[serde(rename = "boxHeight")]
    box_height: Option<f64>,
    #[serde(rename = "usePointStyle")]
    use_point_style: Option<bool>,
    #[serde(rename = "pointStyle")]
    point_style: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawLegendTitle {
    display: Option<bool>,
    text: Option<String>,
    color: Option<String>,
    font: Option<RawLegendFont>,
    padding: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct RawLegendFont {
    size: Option<f64>,
    family: Option<String>,
    weight: Option<serde_json::Value>,
    style: Option<String>,
}

fn default_true() -> bool {
    true
}

fn parse_pie_cutout(value: &serde_json::Value) -> Result<PieCutout, String> {
    match value {
        serde_json::Value::Number(number) => number
            .as_f64()
            .filter(|number| number.is_finite())
            .map(PieCutout::Pixels)
            .ok_or_else(|| "options.cutout must be a finite number or a percentage string".into()),
        serde_json::Value::String(value) => value
            .strip_suffix('%')
            .and_then(|number| number.parse::<f64>().ok())
            .filter(|percent| percent.is_finite())
            .map(PieCutout::Percent)
            .ok_or_else(|| {
                "options.cutout must be a finite number or a percentage string ending in '%'".into()
            }),
        _ => Err("options.cutout must be a finite number or a percentage string".into()),
    }
}

fn parse_pie_dataset_options(datasets: &[RawDataset]) -> Result<Vec<PieGeometryOptions>, String> {
    // オプション未指定は layout 側でも既定値になるため、配列を確保しない。
    if datasets.iter().all(|dataset| {
        dataset.spacing.is_none() && dataset.offset.is_none() && dataset.border_radius.is_none()
    }) {
        return Ok(Vec::new());
    }

    datasets
        .iter()
        .enumerate()
        .map(|(index, dataset)| {
            let prefix = format!("data.datasets[{index}]");
            let spacing = dataset
                .spacing
                .as_deref()
                .map(|value| finite_json_number(value, &format!("{prefix}.spacing")))
                .transpose()?
                .unwrap_or(0.0);
            let offsets = dataset
                .offset
                .as_deref()
                .map(|value| pie_number_values(value, &format!("{prefix}.offset")))
                .transpose()?
                .unwrap_or_default();
            let border_radii = dataset
                .border_radius
                .as_ref()
                .map(|value| {
                    serde_json::from_value::<ScalarOrArray<SchemaArcBorderRadius>>(value.clone())
                        .map_err(|error| format!("{prefix}.borderRadius: {error}"))
                        .map(|radii| {
                            radii
                                .into_vec()
                                .into_iter()
                                .map(|radius| match radius {
                                    SchemaArcBorderRadius::Pixels(value) => {
                                        ArcBorderRadius::Uniform(value)
                                    }
                                    SchemaArcBorderRadius::Corners(corners) => {
                                        ArcBorderRadius::Corners {
                                            outer_start: corners.outer_start.unwrap_or(0.0),
                                            outer_end: corners.outer_end.unwrap_or(0.0),
                                            inner_start: corners.inner_start.unwrap_or(0.0),
                                            inner_end: corners.inner_end.unwrap_or(0.0),
                                        }
                                    }
                                })
                                .collect()
                        })
                })
                .transpose()?
                .unwrap_or_default();

            Ok(PieGeometryOptions {
                spacing,
                offsets,
                border_radii,
            })
        })
        .collect()
}

fn finite_json_number(value: &serde_json::Value, path: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("{path} must be a finite number"))
}

fn pie_number_values(value: &serde_json::Value, path: &str) -> Result<Vec<f64>, String> {
    match value {
        serde_json::Value::Number(_) => Ok(vec![finite_json_number(value, path)?]),
        serde_json::Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, number)| finite_json_number(number, &format!("{path}[{index}]")))
            .collect(),
        _ => Err(format!("{path} must be a number or an array of numbers")),
    }
}

#[derive(Deserialize)]
struct RawData {
    #[serde(default)]
    labels: Vec<String>,
    datasets: Vec<RawDataset>,
}

#[derive(Deserialize)]
struct RawDataset {
    #[serde(default)]
    label: String,
    #[serde(default)]
    order: Option<f64>,
    /// dataset 別の描画種別("bar"/"line")。混合チャートで使う。未指定なら chart 基本型に従う。
    #[serde(rename = "type", default)]
    dataset_type: Option<String>,
    #[serde(default)]
    stack: Option<String>,
    #[serde(rename = "categoryPercentage", default)]
    category_percentage: Option<f64>,
    #[serde(rename = "barPercentage", default)]
    bar_percentage: Option<f64>,
    #[serde(rename = "barThickness", default)]
    bar_thickness: Option<SchemaBarThickness>,
    #[serde(rename = "maxBarThickness", default)]
    max_bar_thickness: Option<f64>,
    #[serde(rename = "minBarLength", default)]
    min_bar_length: Option<f64>,
    // RawDataset は全 chart type で共有するため、pie 専用の生 JSON は Box 化して
    // Vec<RawDataset> の要素サイズ増加を抑える。
    #[serde(default)]
    spacing: Option<Box<serde_json::Value>>,
    #[serde(default)]
    offset: Option<Box<serde_json::Value>>,
    #[serde(rename = "borderRadius", default)]
    border_radius: Option<serde_json::Value>,
    data: DataField,
    #[serde(rename = "backgroundColor")]
    background_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderColor")]
    border_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderWidth")]
    border_width: Option<f64>,
    #[serde(default)]
    fill: RawFillSpec,
    #[serde(default)]
    tension: f64,
    #[serde(
        rename = "cubicInterpolationMode",
        default,
        deserialize_with = "deserialize_cubic_interpolation_mode"
    )]
    cubic_interpolation_mode: RawCubicMode,
    #[serde(rename = "spanGaps", default)]
    span_gaps: Option<serde_json::Value>,
    #[serde(default)]
    stepped: Option<serde_json::Value>,
    // scatter のマーカー半径。Series.point_radius へマップする。
    #[serde(rename = "pointRadius", default)]
    point_radius: Option<f64>,
    #[serde(rename = "pointStyle", default)]
    point_style: Option<Box<serde_json::Value>>,
    #[serde(rename = "showLine", default)]
    show_line: Option<Box<serde_json::Value>>,
    #[serde(rename = "borderDash", default)]
    border_dash: Option<Box<serde_json::Value>>,
    #[serde(rename = "borderDashOffset", default)]
    border_dash_offset: Option<Box<serde_json::Value>>,
}

/// Private parser counterpart of the public schema's `Stepped` contract.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawStepped {
    Bool(bool),
    Mode(RawSteppedMode),
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawSteppedMode {
    Before,
    After,
    Middle,
}

#[derive(Clone, Copy, Debug, Default)]
enum RawCubicMode {
    #[default]
    Unspecified,
    Default,
    Monotone,
    Null,
    Invalid,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawCubicModeValue {
    Mode(CubicMode),
    Null(()),
    Invalid(serde::de::IgnoredAny),
}

fn deserialize_cubic_interpolation_mode<'de, D>(deserializer: D) -> Result<RawCubicMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match RawCubicModeValue::deserialize(deserializer)? {
        RawCubicModeValue::Mode(CubicMode::Default) => RawCubicMode::Default,
        RawCubicModeValue::Mode(CubicMode::Monotone) => RawCubicMode::Monotone,
        RawCubicModeValue::Null(()) => RawCubicMode::Null,
        RawCubicModeValue::Invalid(_) => RawCubicMode::Invalid,
    })
}

impl RawStepped {
    fn into_step_mode(self) -> Option<StepMode> {
        match self {
            Self::Bool(false) => None,
            Self::Bool(true) | Self::Mode(RawSteppedMode::Before) => Some(StepMode::Before),
            Self::Mode(RawSteppedMode::After) => Some(StepMode::After),
            Self::Mode(RawSteppedMode::Middle) => Some(StepMode::Middle),
        }
    }
}

/// `spanGaps` and `stepped` stay as raw JSON on the shared dataset so non-line roots
/// retain their historical "ignore in non-strict mode" behavior; validate them for line roots.
fn parse_line_dataset_options(ds: &RawDataset) -> Result<(bool, Option<StepMode>), String> {
    let span_gaps = ds
        .span_gaps
        .as_ref()
        .map(|value| serde_json::from_value::<bool>(value.clone()))
        .transpose()
        .map_err(|e| format!("spanGaps の型が不正です: {e}"))?
        .unwrap_or(false);
    let step_mode = ds
        .stepped
        .as_ref()
        .map(|value| serde_json::from_value::<RawStepped>(value.clone()))
        .transpose()
        .map_err(|e| format!("stepped の値が不正です: {e}"))?
        .and_then(RawStepped::into_step_mode);
    Ok((span_gaps, step_mode))
}

fn parse_dataset_line_style(
    ds: &RawDataset,
    dataset_index: usize,
    default_show_line: bool,
) -> Result<DatasetLineStyle, String> {
    let prefix = format!("data.datasets[{dataset_index}]");
    let point_style = ds
        .point_style
        .as_deref()
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value::<SchemaPointStyle>(value.clone())
                .map_err(|_| format!("{prefix}.pointStyle must be a supported style or false"))
                .map(|style| match style {
                    SchemaPointStyle::Disabled(_) => DatasetPointStyle::Hidden,
                    SchemaPointStyle::Named(style) => match style {
                        SchemaLegendPointStyle::Circle => DatasetPointStyle::Circle,
                        SchemaLegendPointStyle::Cross => DatasetPointStyle::Cross,
                        SchemaLegendPointStyle::CrossRot => DatasetPointStyle::CrossRot,
                        SchemaLegendPointStyle::Dash => DatasetPointStyle::Dash,
                        SchemaLegendPointStyle::Line => DatasetPointStyle::Line,
                        SchemaLegendPointStyle::Rect => DatasetPointStyle::Rect,
                        SchemaLegendPointStyle::RectRounded => DatasetPointStyle::RectRounded,
                        SchemaLegendPointStyle::RectRot => DatasetPointStyle::RectRot,
                        SchemaLegendPointStyle::Star => DatasetPointStyle::Star,
                        SchemaLegendPointStyle::Triangle => DatasetPointStyle::Triangle,
                    },
                })
        })
        .transpose()?;
    let show_line = ds
        .show_line
        .as_deref()
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value::<bool>(value.clone())
                .map_err(|_| format!("{prefix}.showLine must be a boolean"))
        })
        .transpose()?
        .unwrap_or(default_show_line);
    let border_dash = ds
        .border_dash
        .as_deref()
        .filter(|value| !value.is_null())
        .map(|value| {
            let values = serde_json::from_value::<Vec<f64>>(value.clone()).map_err(|_| {
                format!("{prefix}.borderDash must be an array of finite non-negative numbers")
            })?;
            if values
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            {
                return Err(format!(
                    "{prefix}.borderDash must be an array of finite non-negative numbers"
                ));
            }
            Ok(values)
        })
        .transpose()?
        .unwrap_or_default();
    let border_dash_offset = ds
        .border_dash_offset
        .as_deref()
        .filter(|value| !value.is_null())
        .map(|value| {
            let offset = serde_json::from_value::<f64>(value.clone())
                .map_err(|_| format!("{prefix}.borderDashOffset must be a finite number"))?;
            if !offset.is_finite() {
                return Err(format!("{prefix}.borderDashOffset must be a finite number"));
            }
            Ok(offset)
        })
        .transpose()?
        .unwrap_or(0.0);

    Ok(DatasetLineStyle {
        show_line,
        point_style,
        border_dash,
        border_dash_offset,
    })
}

fn has_dataset_line_style_options(ds: &RawDataset) -> bool {
    ds.point_style.is_some()
        || ds.show_line.is_some()
        || ds.border_dash.is_some()
        || ds.border_dash_offset.is_some()
}

fn parse_cubic_interpolation_mode(ds: &RawDataset) -> Result<Option<CubicMode>, String> {
    match ds.cubic_interpolation_mode {
        RawCubicMode::Unspecified => Ok(None),
        RawCubicMode::Default => Ok(Some(CubicMode::Default)),
        RawCubicMode::Monotone => Ok(Some(CubicMode::Monotone)),
        RawCubicMode::Null | RawCubicMode::Invalid => Err(
            "cubicInterpolationMode の値が不正です: expected \"default\" or \"monotone\""
                .to_string(),
        ),
    }
}

/// `data`: 数値配列(カテゴリ系)、ネスト配列(boxplot)、または点オブジェクト配列(scatter/bubble)。
/// untagged は順に試す: `Nums` → `[1, null, 2]`、`Boxes` → `[[1,2,3,4,5], null]`、`Points` → `[{x,y}]`。
/// `Nums` / `Boxes` は要素の `null` を許容し、frontend 境界で `f64::NAN` に写像する。
#[derive(Deserialize)]
#[serde(untagged)]
enum DataField {
    Nums(Vec<Option<f64>>),
    Boxes(Vec<Option<Vec<f64>>>),
    Points(Vec<RawPoint>),
}

#[derive(Deserialize, Clone)]
struct RawPoint {
    x: f64,
    y: f64,
    #[serde(default)]
    r: Option<f64>,
}

/// 全フィールドが NaN の欠損 `BoxPoint`。boxplot の null 行(row=null, 全 None の
/// フラット配列、5 要素でない非 null 行)を layout 側の欠損として一貫に扱うための共通値。
fn nan_box_point() -> crate::ir::BoxPoint {
    crate::ir::BoxPoint {
        min: f64::NAN,
        q1: f64::NAN,
        median: f64::NAN,
        q3: f64::NAN,
        max: f64::NAN,
    }
}

impl DataField {
    /// 数値配列なら採用（`None` → `f64::NAN`)、それ以外は空。カテゴリ系チャートの `values` 用。
    fn into_values(self) -> Vec<f64> {
        match self {
            DataField::Nums(v) => v.into_iter().map(|x| x.unwrap_or(f64::NAN)).collect(),
            _ => vec![],
        }
    }

    /// 点配列なら IR の `Point` へ、数値配列なら空。scatter/bubble の `points` 用。
    fn into_points(self) -> Vec<Point> {
        match self {
            DataField::Points(ps) => ps
                .into_iter()
                .map(|p| Point {
                    x: p.x,
                    y: p.y,
                    r: p.r,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// ネスト配列なら IR の `BoxPoint` へ変換する。boxplot の `box_points` 用。
    /// `None` 行は全 NaN の BoxPoint に写像する（layout 側で欠損として扱われる)。
    /// 各非 null 行は厳密に [min, q1, median, q3, max] の 5 要素でなければならない。
    /// 5 要素以外の非 null 行はバイト数ガードをバイパスできるため拒否し NaN 行として扱う。
    ///
    /// 加えて全要素が `None` のフラット配列(`data:[null, null]` 等)も受理する:
    /// untagged enum は `Boxes` より先に `Nums` にマッチするため、スキーマ有効な
    /// 「行が全て null の boxplot」は `Nums(Vec<None>)` として届く。layout は
    /// 全 NaN 行を欠損として扱うので、空チャートではなく null 行数と同じ長さの
    /// 全 NaN box 列に写像する。
    fn into_box_points(self) -> Vec<crate::ir::BoxPoint> {
        match self {
            DataField::Boxes(rows) => rows
                .into_iter()
                .map(|row| match row {
                    None => nan_box_point(),
                    Some(cols) if cols.len() == 5 => crate::ir::BoxPoint {
                        min: cols[0],
                        q1: cols[1],
                        median: cols[2],
                        q3: cols[3],
                        max: cols[4],
                    },
                    Some(_) => nan_box_point(),
                })
                .collect(),
            DataField::Nums(v) if v.iter().all(Option::is_none) => {
                vec![nan_box_point(); v.len()]
            }
            _ => vec![],
        }
    }
}

/// chart.js の「スカラ or 配列」を許容する untagged ヘルパ。
#[derive(Deserialize)]
#[serde(untagged)]
enum ScalarOrArray<T> {
    One(T),
    Many(Vec<T>),
}

impl<T: Clone> ScalarOrArray<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            ScalarOrArray::One(v) => vec![v],
            ScalarOrArray::Many(v) => v,
        }
    }
}

// ---------------------------------------------------------------------------
// schema → IR ヘルパ
//
// `options.scales.<axis>` の typed オプション(schema::common)を、IR の
// `AxisTitle` / `AxisGrid` / `AxisBorder` へ変換する純関数群。
// `ChartSpec` 構築時に `axis_from` 経路で呼び出される。
// ---------------------------------------------------------------------------

/// `axis.type == "logarithmic"` かどうかを判定する。それ以外の値
/// (`"category"`/`"time"`/`"linear"` やタイポ、未指定)は false 扱い。
fn is_logarithmic(opts: Option<&AxisOptions>) -> bool {
    opts.and_then(|a| a.r#type.as_deref()) == Some("logarithmic")
}

/// `axis.title` を IR の [`AxisTitle`] に変換する。
///
/// 以下のいずれかで [`None`] を返す:
/// - `opts` 自体が [`None`]
/// - `display == Some(false)` (明示的に非表示)
/// - `text` が [`None`] または空文字 (描画すべき文字列がない)
///
/// `align` は schema の [`SchemaAxisTitleAlign`] を IR の
/// [`crate::ir::AxisTitleAlign`] にマップする。指定なしは `Center`。
fn axis_title_from(opts: Option<&AxisTitleOptions>) -> Option<AxisTitle> {
    let o = opts?;
    if o.display == Some(false) {
        return None;
    }
    let text = o.text.as_deref()?;
    if text.is_empty() {
        return None;
    }
    let align = match o.align {
        Some(SchemaAxisTitleAlign::Start) => AxisTitleAlign::Start,
        Some(SchemaAxisTitleAlign::End) => AxisTitleAlign::End,
        _ => AxisTitleAlign::Center,
    };
    Some(AxisTitle {
        text: text.to_string(),
        color: o.color.as_deref().and_then(parse_color),
        font_size: o.font.as_ref().and_then(|f| f.size),
        align,
    })
}

/// `axis.grid` を IR の [`AxisGrid`] に変換する。
///
/// - `opts` が [`None`] のときは [`AxisGrid::default()`] (display=true, width=1.0)
/// - v1 仕様: `drawOnChartArea=false` は `display=false` と同義として扱う
///   (chart area 外だけに grid を残す挙動は v1 で未サポート)
/// - `color` / `line_width` の [`ScalarOrArray`] は先頭要素だけを見る
///   (per-tick 配列は v1 未描画; 受理のみ)
fn axis_grid_from(opts: Option<&GridLineOptions>) -> AxisGrid {
    use crate::schema::common::ScalarOrArray;
    let Some(g) = opts else {
        return AxisGrid::default();
    };
    let display = g.display.unwrap_or(true) && g.draw_on_chart_area.unwrap_or(true);
    let color = match &g.color {
        Some(ScalarOrArray::One(s)) => parse_color(s),
        Some(ScalarOrArray::Many(v)) => v.first().and_then(|s| parse_color(s)),
        None => None,
    };
    let line_width = match &g.line_width {
        Some(ScalarOrArray::One(w)) => *w,
        Some(ScalarOrArray::Many(v)) => *v.first().unwrap_or(&1.0),
        None => 1.0,
    };
    // fulgur は Chart.js の既定 (true) から意図的に乖離: 未指定なら false。
    // 既存スナップショット保護のため。詳細は `AxisGrid::default` のドキュメント参照。
    let draw_ticks = g.draw_ticks.unwrap_or(false);
    AxisGrid {
        display,
        color,
        line_width,
        draw_ticks,
    }
}

/// `axis.border` を IR の [`AxisBorder`] に変換する。
///
/// `opts` が [`None`] のときは [`AxisBorder::default()`]。値ごとの既定値は
/// `display=true` / `width=1.0` / `dash=[]` / `color=None`。
fn axis_border_from(opts: Option<&AxisBorderOptions>) -> AxisBorder {
    let Some(b) = opts else {
        return AxisBorder::default();
    };
    AxisBorder {
        display: b.display.unwrap_or(true),
        color: b.color.as_deref().and_then(parse_color),
        width: b.width.unwrap_or(1.0),
        dash: b.dash.clone().unwrap_or_default(),
    }
}

/// Private parser counterpart of the public line fill target schema.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum RawFillSpec {
    Bool(bool),
    Number(f64),
    Mode(String),
    Value(RawFillValue),
    Colors(RawFillColors),
    #[default]
    Absent,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFillValue {
    value: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFillColors {
    target: RawFillTarget,
    above: Option<String>,
    below: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawFillTarget {
    Bool(bool),
    Number(f64),
    Mode(String),
    Value(RawFillValue),
}

impl RawFillSpec {
    fn to_area_fill(&self, source_index: usize, dataset_count: usize) -> Option<AreaFill> {
        let (target, above, below) = match self {
            Self::Bool(true) => (AreaFillTarget::Origin, None, None),
            Self::Bool(false) | Self::Absent => return None,
            Self::Number(value) => (
                decode_fill_index(*value, false, source_index, dataset_count)?,
                None,
                None,
            ),
            Self::Mode(mode) => (
                decode_fill_mode(mode, source_index, dataset_count)?,
                None,
                None,
            ),
            Self::Value(value) if value.value.is_finite() => {
                (AreaFillTarget::Value(value.value), None, None)
            }
            Self::Value(_) => return None,
            Self::Colors(colors) => (
                decode_fill_target(&colors.target, source_index, dataset_count)?,
                colors.above.as_deref().and_then(parse_color),
                colors.below.as_deref().and_then(parse_color),
            ),
        };
        Some(AreaFill {
            target,
            above,
            below,
        })
    }
}

fn decode_fill_target(
    target: &RawFillTarget,
    source_index: usize,
    dataset_count: usize,
) -> Option<AreaFillTarget> {
    match target {
        RawFillTarget::Bool(true) => Some(AreaFillTarget::Origin),
        RawFillTarget::Bool(false) => None,
        RawFillTarget::Number(value) => {
            decode_fill_index(*value, false, source_index, dataset_count)
        }
        RawFillTarget::Mode(mode) => decode_fill_mode(mode, source_index, dataset_count),
        RawFillTarget::Value(value) if value.value.is_finite() => {
            Some(AreaFillTarget::Value(value.value))
        }
        RawFillTarget::Value(_) => None,
    }
}

fn decode_fill_mode(
    mode: &str,
    source_index: usize,
    dataset_count: usize,
) -> Option<AreaFillTarget> {
    match mode {
        "origin" => Some(AreaFillTarget::Origin),
        "start" => Some(AreaFillTarget::Start),
        "end" => Some(AreaFillTarget::End),
        "stack" => Some(AreaFillTarget::Stack),
        _ => {
            let relative = mode.starts_with('+') || mode.starts_with('-');
            let value = mode.parse::<f64>().ok()?;
            decode_fill_index(value, relative, source_index, dataset_count)
        }
    }
}

fn decode_fill_index(
    value: f64,
    relative: bool,
    source_index: usize,
    dataset_count: usize,
) -> Option<AreaFillTarget> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    let target = if relative {
        i128::try_from(source_index)
            .ok()?
            .checked_add(value as i128)?
    } else if value >= 0.0 {
        value as i128
    } else {
        return None;
    };
    let target = usize::try_from(target).ok()?;
    (target < dataset_count && target != source_index).then_some(AreaFillTarget::Dataset(target))
}

pub fn parse(json: &str, strict: bool) -> Result<ChartSpec, String> {
    // matrix は専用パスで処理する（data 形式が {x,y,v} で他と異なるため）。
    // check_unknown_keys より先に捕捉することで、matrix の "v" キーを未知キーと
    // 誤判定するのを防ぐ。
    {
        let chart_type = serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            });
        if chart_type.as_deref() == Some("matrix") {
            if strict {
                check_unknown_keys_matrix(json)?;
            }
            return parse_matrix(json);
        }
        if chart_type.as_deref() == Some("treemap") {
            if strict {
                check_unknown_keys_treemap(json)?;
            }
            return parse_treemap(json);
        }
        if matches!(chart_type.as_deref(), Some("wordCloud") | Some("word")) {
            if strict {
                check_unknown_keys_wordcloud(json)?;
            }
            return parse_wordcloud(json);
        }
        if chart_type.as_deref() == Some("sankey") {
            if strict {
                check_unknown_keys_sankey(json)?;
            }
            return parse_sankey(json);
        }
        if matches!(chart_type.as_deref(), Some("gauge") | Some("radialGauge")) {
            let radial = chart_type.as_deref() == Some("radialGauge");
            if strict {
                check_unknown_keys_gauge(json)?;
            }
            return parse_gauge(json, radial);
        }
        if matches!(
            chart_type.as_deref(),
            Some("progress") | Some("progressBar")
        ) {
            if strict {
                check_unknown_keys_progress(json)?;
            }
            // progress は専用チェック済み、汎用 check_unknown_keys はスキップ
        } else if strict {
            let allow_outlabels = matches!(
                chart_type.as_deref(),
                Some("outlabeledPie") | Some("outlabeledDoughnut")
            );
            let allow_radial_scale =
                matches!(chart_type.as_deref(), Some("radar") | Some("polarArea"));
            let allow_pie = matches!(chart_type.as_deref(), Some("pie") | Some("doughnut"));
            check_unknown_keys(json, allow_outlabels, allow_radial_scale, allow_pie)?;
        }
    }

    let raw: RawSpec = serde_json::from_str(json).map_err(|e| e.to_string())?;

    // 積み上げ判定: chart.js は配置(dodge/同スロット)と値累積を独立した軸で制御する。
    // index 軸の stacked → placement_stacked(棒の配置)
    // 値軸の stacked  → value_stacked(値累積・値域計算)
    // dataset.stack は同じスタック内のグループ化に使う。stacked の有効化自体は軸設定で行う。
    // indexAxis は chart.js では "x"/"y" のみ。想定外の値は orientation 判定と同様に
    // 縦棒(index 軸=x)として扱うため、"y" 以外は "x" に正規化する。
    let index_axis = if raw.options.index_axis.as_deref() == Some("y") {
        "y"
    } else {
        "x"
    };
    let value_axis = if index_axis == "y" { "x" } else { "y" };
    let get_axis_stacked = |axis: &str| -> bool {
        raw.options
            .scales
            .as_ref()
            .and_then(|s| match axis {
                "x" => s.x.as_ref(),
                "y" => s.y.as_ref(),
                _ => None,
            })
            .and_then(|a| a.stacked)
            .unwrap_or(false)
    };
    let placement_stacked = get_axis_stacked(index_axis);
    let value_stacked = get_axis_stacked(value_axis);

    // chart 基本型。bar/line のときだけ dataset 別 type による混合が起こりうる。
    // 基本型は SeriesType のフォールバックにも使う(bar→Bar, line→Line, それ以外→Bar(未使用))。
    let base_series_type = match raw.chart_type.as_str() {
        "line" | "sparkline" => SeriesType::Line,
        _ => SeriesType::Bar,
    };

    // dataset の実効描画種別を解決する。
    // type が "bar"/"line" ならそれを優先し、無ければ chart 基本型に従う。
    let resolve_series_type = |dt: &Option<String>| -> SeriesType {
        match dt.as_deref() {
            Some("bar") => SeriesType::Bar,
            Some("line") => SeriesType::Line,
            _ => base_series_type,
        }
    };

    // dataset 別 type は「基本 type が bar/line」かつ値が bar/line のときのみ有効。
    // pie/scatter 等への type 指定や bar/line 以外の値は、黙って無視せず明示エラーにする。
    let is_mixable_base = matches!(raw.chart_type.as_str(), "bar" | "line");
    for ds in &raw.data.datasets {
        if let Some(t) = &ds.dataset_type {
            if !is_mixable_base {
                return Err(format!(
                    "dataset の type は基本 type が bar/line のときのみ指定できます(基本 type={})",
                    raw.chart_type
                ));
            }
            if t != "bar" && t != "line" {
                return Err(format!("未対応の dataset type: {t}"));
            }
        }
    }

    // 各 dataset の実効種別。Mixed 判定と Series.series_type の双方に使う。
    let series_types: Vec<SeriesType> = raw
        .data
        .datasets
        .iter()
        .map(|ds| resolve_series_type(&ds.dataset_type))
        .collect();

    // 描画 kind は、基本型が bar/line のとき「解決後の dataset 種別」で決める:
    // 両方含む→Mixed、全 Line→Line、全 Bar→Bar。dataset 別 type の単独上書き
    // (例 {"type":"bar","datasets":[{"type":"line"}]})も正しく反映される。
    let has_bar = series_types.contains(&SeriesType::Bar);
    let has_line = series_types.contains(&SeriesType::Line);
    let bar_kind = || ChartKind::Bar {
        horizontal: index_axis == "y",
        placement_stacked,
        value_stacked,
    };

    // 混合(bar+line)は縦・非積み上げのみ対応。横棒(indexAxis:y)や積み上げと併用すると
    // それらが黙って失われるため、受理せず明示エラーにする(mixed.rs は縦・非積み上げ前提)。
    // value_stacked も拒否: ChartKind::Mixed にフラグが伝わらず黙って消えるため。
    if is_mixable_base && has_bar && has_line {
        let horizontal = index_axis == "y";
        if horizontal || placement_stacked || value_stacked {
            return Err(
                "混合チャート(bar+line)は横棒(indexAxis:y)・index/value軸の積み上げ(stacked)と併用できません"
                    .to_string(),
            );
        }
    }

    // Line layout は常に index 軸=X、値軸=Y として描く。水平 line で値軸=X の
    // stacked を受理すると、IR の value_stacked と実際の描画軸が食い違うため拒否する。
    if is_mixable_base && has_line && !has_bar && index_axis == "y" && value_stacked {
        return Err("積み上げ line chart は横向き(indexAxis:y)に未対応です".to_string());
    }

    let kind = if is_mixable_base && has_bar && has_line {
        ChartKind::Mixed
    } else if is_mixable_base && has_line && !has_bar {
        ChartKind::Line {
            stacked: value_stacked,
            stacked_missing_values_are_gaps: true,
        }
    } else if is_mixable_base && has_bar && !has_line {
        bar_kind()
    } else {
        // dataset 空(種別未確定)、または mixable でない型。基本 type で決める。
        match raw.chart_type.as_str() {
            "bar" => bar_kind(),
            "line" => ChartKind::Line {
                stacked: value_stacked,
                stacked_missing_values_are_gaps: true,
            },
            "pie" => ChartKind::Pie {
                cutout: PieCutout::Percent(0.0),
                dataset_options: vec![],
            },
            "doughnut" => ChartKind::Pie {
                cutout: PieCutout::Percent(50.0),
                dataset_options: vec![],
            },
            "scatter" => ChartKind::Scatter,
            "bubble" => ChartKind::Bubble,
            "radar" => ChartKind::Radar,
            // QuickChart の正式名は "progressBar"。互換のため "progress" も受理する。
            "progress" | "progressBar" => ChartKind::Progress,
            "boxplot" => ChartKind::BoxPlot,
            "polarArea" => ChartKind::PolarArea,
            "sparkline" => ChartKind::Sparkline,
            "outlabeledPie" => ChartKind::OutlabeledPie {
                donut_ratio: 0.0,
                outlabel: build_outlabel_config(&raw.options.plugins.outlabels),
            },
            "outlabeledDoughnut" => ChartKind::OutlabeledPie {
                donut_ratio: 0.5,
                outlabel: build_outlabel_config(&raw.options.plugins.outlabels),
            },
            other => return Err(format!("未対応の type: {other}")),
        }
    };

    let kind = match kind {
        ChartKind::Pie { cutout, .. } => ChartKind::Pie {
            cutout: raw
                .options
                .cutout
                .as_ref()
                .map(parse_pie_cutout)
                .transpose()?
                .unwrap_or(cutout),
            dataset_options: parse_pie_dataset_options(&raw.data.datasets)?,
        },
        other => other,
    };

    // datalabels: 既存は「キーが存在し display!=false なら有効」。
    // progress のみ既定 ON（QuickChart 準拠）。明示 display:false は尊重する。
    let data_labels = match (&raw.options.plugins.datalabels, &kind) {
        (Some(dl), _) => dl.display != Some(false),
        (None, ChartKind::Progress) => true,
        (None, _) => false,
    };

    // decimation: options.plugins.decimation を IR へ解決する。未指定は既定(自動オン)。
    let decimation = match &raw.options.plugins.decimation {
        Some(d) => {
            let algorithm = match d.algorithm.as_deref() {
                None | Some("min-max") => DecimationAlgorithm::MinMax,
                Some("lttb") => DecimationAlgorithm::Lttb,
                Some(other) => {
                    return Err(format!("未対応の decimation algorithm: {other}"));
                }
            };
            Decimation {
                enabled: d.enabled.unwrap_or(true),
                algorithm,
                samples: d.samples,
                threshold: d.threshold,
            }
        }
        None => Decimation::default(),
    };

    // テーマ解決(配色に使うため色解決より先に行う)。
    let theme = build_theme(raw.options.theme);

    let is_pie = matches!(
        kind,
        ChartKind::Pie { .. } | ChartKind::PolarArea | ChartKind::OutlabeledPie { .. }
    );
    // progress も pie 同様に前景をソリッド(alpha=1.0)で塗る。
    let is_progress = matches!(kind, ChartKind::Progress);
    // scatter/bubble はどちらも点データ(Series.points)を使う線形×線形チャート。
    let is_point_based = matches!(kind, ChartKind::Scatter | ChartKind::Bubble);
    let is_boxplot = matches!(kind, ChartKind::BoxPlot);
    // sparkline はライン系のスケール慣習に従い begin_at_zero をデフォルト false にする。
    let is_sparkline = matches!(kind, ChartKind::Sparkline);

    // データ形状とチャート種の整合を検査する。点ベース(scatter/bubble)は {x,y(,r)}
    // 配列、カテゴリ系は数値配列を要する。非空の不一致は空チャート化せず明示エラーに。
    // 例外: boxplot の全 None Nums(`data:[null, null]` 等)は untagged 順で `Nums` にマッチする
    // スキーマ有効な入力で、`into_box_points` が全 NaN 行に写像するため mismatch 扱いしない。
    for ds in &raw.data.datasets {
        let mismatched = match &ds.data {
            DataField::Nums(v) => {
                if is_point_based {
                    !v.is_empty()
                } else if is_boxplot {
                    !v.is_empty() && !v.iter().all(Option::is_none)
                } else {
                    false
                }
            }
            DataField::Boxes(v) => !is_boxplot && !v.is_empty(),
            DataField::Points(v) => !is_point_based && !v.is_empty(),
        };
        if mismatched {
            return Err(format!(
                "チャート種 {} とデータ形状が一致しません",
                raw.chart_type
            ));
        }
    }

    // line/bar/mixed/boxplot 以外の数値系チャートは layout 側で NaN/欠損を扱う保証が
    // ないため(sparkline は y=0 に丸まり、radar/polarArea/gauge/progress/pie 等は
    // 不正な control point や 0 頂点として描画されうる)、data 内の null は parse 時に
    // 拒否する。将来対応した種別を追加した段階で allowlist を広げる。
    let supports_null_data = matches!(
        kind,
        crate::ir::ChartKind::Line { .. }
            | crate::ir::ChartKind::Bar { .. }
            | crate::ir::ChartKind::Mixed
            | crate::ir::ChartKind::BoxPlot
    );
    if !supports_null_data {
        for ds in &raw.data.datasets {
            if let DataField::Nums(v) = &ds.data
                && v.iter().any(Option::is_none)
            {
                return Err(format!(
                    "{} は data 内の null を受け付けません",
                    raw.chart_type
                ));
            }
        }
    }

    // chart.js v4 の Colors プラグインはデータセットのいずれかに backgroundColor か
    // borderColor が指定されていれば chart 全体をスキップする(per-dataset ではない)。
    let chart_has_explicit_colors = raw
        .data
        .datasets
        .iter()
        .any(|ds| ds.background_color.is_some() || ds.border_color.is_some());

    let has_line_dataset_options = raw
        .data
        .datasets
        .iter()
        .any(|ds| ds.span_gaps.is_some() || ds.stepped.is_some());
    if raw.chart_type == "line"
        && !matches!(kind, ChartKind::Line { .. })
        && has_line_dataset_options
    {
        return Err("spanGaps and stepped are only supported for line charts".to_string());
    }

    if is_mixable_base
        && (strict || raw.chart_type == "line")
        && raw
            .data
            .datasets
            .iter()
            .zip(&series_types)
            .any(|(dataset, series_type)| {
                *series_type != SeriesType::Line && has_dataset_line_style_options(dataset)
            })
    {
        return Err(
            "pointStyle, showLine, borderDash, and borderDashOffset are only supported for line datasets"
                .to_string(),
        );
    }

    let line_dataset_options = if raw.chart_type == "line" {
        raw.data
            .datasets
            .iter()
            .map(parse_line_dataset_options)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![(false, None); raw.data.datasets.len()]
    };

    let dataset_line_styles = raw
        .data
        .datasets
        .iter()
        .enumerate()
        .map(|(index, dataset)| {
            let is_line_dataset = is_mixable_base && series_types[index] == SeriesType::Line;
            let is_scatter_dataset = raw.chart_type == "scatter";
            if is_line_dataset || is_scatter_dataset {
                parse_dataset_line_style(dataset, index, is_line_dataset)
                    .map(|style| Some(Box::new(style)))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    let cubic_interpolation_modes = if is_mixable_base {
        raw.data
            .datasets
            .iter()
            .zip(&series_types)
            .map(|(ds, series_type)| {
                if *series_type == SeriesType::Line
                    || strict
                    || matches!(ds.cubic_interpolation_mode, RawCubicMode::Null)
                {
                    parse_cubic_interpolation_mode(ds)
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![None; raw.data.datasets.len()]
    };

    // `RawDataset` is shared by every chart type, but object-form borderRadius has
    // chart-specific meanings. Validate it as a bar option only for rendered bar datasets.
    let bar_border_radii = raw
        .data
        .datasets
        .iter()
        .enumerate()
        .map(|(i, ds)| {
            if series_types[i] != SeriesType::Bar
                || !matches!(kind, ChartKind::Bar { .. } | ChartKind::Mixed)
            {
                return Ok(None);
            }
            ds.border_radius
                .as_ref()
                .map(|value| {
                    serde_json::from_value::<SchemaBorderRadius>(value.clone())
                        .map_err(|error| format!("datasets[{i}].borderRadius: {error}"))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, String>>()?;

    // typed `AxisOptions` 経由で読むことで、`beginAtZero` 等の camelCase タイポは
    // schema deserialize 時に拒否される(silent 素通り防止)。
    let x_opts = raw.options.scales.as_ref().and_then(|s| s.x.as_ref());
    let y_opts = raw.options.scales.as_ref().and_then(|s| s.y.as_ref());

    // bar/line の値軸と scatter/bubble の数値 x/y 軸で log を許可する。
    // カテゴリ軸や未対応 kind への type:"logarithmic" 指定は黙って無視(Linear のまま)。
    let x_axis_is_log = matches!(
        kind,
        ChartKind::Bar {
            horizontal: true,
            ..
        } | ChartKind::Scatter
            | ChartKind::Bubble
    ) && is_logarithmic(x_opts);
    let y_axis_is_log = matches!(
        kind,
        ChartKind::Bar {
            horizontal: false,
            ..
        } | ChartKind::Line { .. }
            | ChartKind::Scatter
            | ChartKind::Bubble
    ) && is_logarithmic(y_opts);
    let is_mixed = matches!(kind, ChartKind::Mixed);
    let dataset_count = raw.data.datasets.len();
    let mut dataset_orders = is_mixed.then(|| Vec::with_capacity(raw.data.datasets.len()));
    let series: Vec<Series> = raw
        .data
        .datasets
        .into_iter()
        .enumerate()
        .zip(dataset_line_styles)
        .map(|((i, ds), line_style)| {
            if let Some(orders) = dataset_orders.as_mut() {
                orders.push(ds.order.unwrap_or(0.0));
            }
            let area_fill = ds.fill.to_area_fill(i, dataset_count);
            // 点ベースは点データ、boxplot はボックスデータ、それ以外は数値配列を採る。`data` は一度だけ消費する。
            let (values, points, box_points) = if is_point_based {
                (vec![], ds.data.into_points(), vec![])
            } else if is_boxplot {
                (vec![], vec![], ds.data.into_box_points())
            } else {
                // 対数軸で描画できない値のスキップは layout 層で行う。IR の values は
                // introspection API が入力値をそのまま報告できるよう保持する。
                (ds.data.into_values(), vec![], vec![])
            };
            let n = if is_point_based {
                points.len()
            } else if is_boxplot {
                box_points.len()
            } else {
                values.len()
            };
            let fill_alpha = if is_pie || is_progress {
                1.0_f32
            } else {
                0.5_f32
            };
            let has_explicit_bg = ds.background_color.is_some();
            let has_explicit_border = ds.border_color.is_some();
            // chart.js v4 の Colors プラグインは chart 内のいずれかのデータセットに
            // backgroundColor か borderColor が指定されていれば chart 全体をスキップし、
            // 未設定側は rgba(0,0,0,0.1) になる。pie/progress は独自パレットのため除外。
            let colors_plugin_skips = !is_pie && !is_progress && chart_has_explicit_colors;
            let global_default = |count: usize| {
                vec![
                    Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0.1,
                    };
                    count
                ]
            };
            let fill = if colors_plugin_skips && !has_explicit_bg {
                global_default(n.max(1))
            } else {
                resolve_colors(
                    ds.background_color,
                    is_pie,
                    i,
                    n,
                    &theme.palette,
                    fill_alpha,
                    theme.is_custom_palette,
                )
            };
            let border_color = ds.border_color;
            let stroke = if colors_plugin_skips && !has_explicit_border {
                global_default(fill.len())
            } else {
                resolve_colors(
                    border_color,
                    is_pie,
                    i,
                    n,
                    &theme.palette,
                    1.0,
                    theme.is_custom_palette,
                )
            };
            // 実効描画種別。線の既定線幅(3.0)を chart 基本型でなく系列種別で決めるため、
            // 単一種別(全 Line→3.0 / 全 Bar→1.0)では従来と byte 一致し、混合では line だけ太くなる。
            let series_type = series_types[i];
            let bar_geometry = if series_type == SeriesType::Bar
                && matches!(kind, ChartKind::Bar { .. } | ChartKind::Mixed)
                && (ds.category_percentage.is_some()
                    || ds.bar_percentage.is_some()
                    || ds.bar_thickness.is_some()
                    || ds.max_bar_thickness.is_some()
                    || ds.min_bar_length.is_some()
                    || bar_border_radii[i].is_some())
            {
                Some(BarGeometryOptions {
                    category_percentage: ds.category_percentage,
                    bar_percentage: ds.bar_percentage,
                    bar_thickness: ds.bar_thickness.map(|thickness| match thickness {
                        SchemaBarThickness::Pixels(value) => BarThickness::Pixels(value),
                        SchemaBarThickness::Mode(
                            crate::schema::chartjs::BarThicknessMode::Flex,
                        ) => BarThickness::Flex,
                    }),
                    max_bar_thickness: ds.max_bar_thickness,
                    min_bar_length: ds.min_bar_length,
                    border_radius: bar_border_radii[i].map(|radius| match radius {
                        SchemaBorderRadius::Pixels(value) => BarBorderRadius::Uniform(value),
                        SchemaBorderRadius::Corners(corners) => BarBorderRadius::Corners {
                            top_left: corners.top_left,
                            top_right: corners.top_right,
                            bottom_left: corners.bottom_left,
                            bottom_right: corners.bottom_right,
                        },
                    }),
                })
            } else {
                None
            };
            Series {
                name: ds.label,
                values,
                points,
                fill,
                stroke,
                stroke_width: ds.border_width.unwrap_or(default_border_width(series_type)),
                area: area_fill.is_some(),
                area_fill,
                interpolation: match cubic_interpolation_modes[i] {
                    Some(CubicMode::Monotone) => LineInterpolation::Monotone,
                    Some(CubicMode::Default) | None => {
                        line_interpolation(normalize_tension(ds.tension))
                    }
                },
                span_gaps: line_dataset_options[i].0,
                step_mode: line_dataset_options[i].1,
                line_style,
                series_type,
                stack: if is_mixable_base {
                    Some(ds.stack.unwrap_or_else(|| match series_type {
                        SeriesType::Bar => "bar".to_string(),
                        SeriesType::Line => "line".to_string(),
                    }))
                } else {
                    None
                },
                bar_geometry,
                point_radius: ds.point_radius,
                box_points,
                tree: vec![],
                links: vec![],
            }
        })
        .collect();

    // Chart.js は order 昇順で凡例・tooltip の系列を並べ、同値では宣言順を使う。
    // 描画側はこの順序を逆にたどり、高い order を先に(背面へ)描く。
    // 非 mixed chart は追加の order 配列・並べ替え領域を確保しない。
    let series = if let Some(orders) = dataset_orders {
        let mut ordered_series: Vec<(f64, usize, Series)> = series
            .into_iter()
            .zip(orders)
            .enumerate()
            .map(|(index, (series, order))| (order, index, series))
            .collect();
        ordered_series.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        let mut original_to_sorted = vec![0; ordered_series.len()];
        for (sorted_index, (_, original_index, _)) in ordered_series.iter().enumerate() {
            original_to_sorted[*original_index] = sorted_index;
        }
        for (_, _, series) in &mut ordered_series {
            if let Some(area_fill) = series.area_fill.as_mut()
                && let AreaFillTarget::Dataset(original_index) = &mut area_fill.target
            {
                *original_index = original_to_sorted[*original_index];
            }
        }
        ordered_series
            .into_iter()
            .map(|(_, _, series)| series)
            .collect()
    } else {
        series
    };

    // レーダーは負値に未対応。半径が負になると頂点が反対スポークへ反転し、
    // 実データと異なる多角形になるため、parse 時に明示的に拒否する。
    if matches!(kind, ChartKind::Radar)
        && series
            .iter()
            .any(|s: &Series| s.values.iter().any(|v| v.is_finite() && *v < 0.0))
    {
        return Err("レーダーチャートは負の値に未対応です".to_string());
    }

    // scatter/bubble は線形×線形軸でゼロ起点を強制しない(データ由来のドメインを使う)。
    // 縦棒: 値軸が Y → y_axis.begin_at_zero=true（デフォルト）。
    // 横棒: 値軸が X → x_axis.begin_at_zero=true（デフォルト）。
    // ライン: chart.js デフォルトは beginAtZero=false（データ密着レンジ）。Mixed は bar データセットを
    // 含むため除外せず true のまま。ユーザーが options.scales.{x,y}.beginAtZero を明示した場合は優先する。
    let is_horizontal = matches!(
        kind,
        ChartKind::Bar {
            horizontal: true,
            ..
        }
    );
    let is_line = matches!(kind, ChartKind::Line { .. });
    let value_begin_at_zero = !is_point_based && !is_sparkline && !is_line;

    // suggestedMin/suggestedMax および beginAtZero: options.scales.{x,y} から取得する
    // (x_opts/y_opts 自体は series 構築より前に hoist 済み)。
    let x_begin_at_zero = x_opts
        .and_then(|a| a.begin_at_zero)
        .unwrap_or(is_horizontal && value_begin_at_zero);
    let y_begin_at_zero = y_opts
        .and_then(|a| a.begin_at_zero)
        .unwrap_or(!is_horizontal && value_begin_at_zero && !is_boxplot);
    let suggested_min_y = y_opts.and_then(|a| a.suggested_min);
    let suggested_max_y = y_opts.and_then(|a| a.suggested_max);
    let suggested_min_x = x_opts.and_then(|a| a.suggested_min);
    let suggested_max_x = x_opts.and_then(|a| a.suggested_max);
    // category スケールの offset。明示時のみ尊重(既定 false=edge-to-edge)。
    // line レイアウトの x 軸のみが消費する(y は line の値軸)。
    let x_offset = x_opts.and_then(|a| a.offset).unwrap_or(false);
    let y_offset = y_opts.and_then(|a| a.offset).unwrap_or(false);

    // scales.r: radar / polarArea かつ scales.r にドメインキーが明示されているときのみ populate。
    // 他 kind、`scales.r` 未指定、`scales.r: null` の場合は None を保ち後方互換を維持する。
    let is_radial = matches!(kind, ChartKind::Radar | ChartKind::PolarArea);
    let radial_axis = if is_radial {
        // ここで初めて typed に解釈する。
        //
        // 非 strict では型不一致 (例 `"r": 5`、`{"max": "100"}`) を silently 無視する
        // (Chart.js 互換)。strict では伝播させる: `check_unknown_keys` はキー名しか
        // 見ないので `{"max": "100"}` のような「キーは正しいが型が違う」入力を
        // 素通りさせてしまい、`.ok()` で握り潰すと既定ドメインで描画されてしまう。
        let parsed = match raw.options.scales.as_ref().and_then(|s| s.r.as_ref()) {
            None => None,
            Some(v) => match serde_json::from_value::<RawRadialAxis>(v.clone()) {
                Ok(r) => Some(r),
                Err(e) if strict => {
                    return Err(format!("options.scales.r の型が不正です: {e}"));
                }
                Err(_) => None,
            },
        };
        parsed
            .as_ref()
            // Codex Fix 9: `scales.r: {}` や、`ticks` 等の視覚キーのみを持つ `r` は no-op とする。
            // ドメインキーが 1 つも無いのに Some(RadialAxis) を返すと layout が override 経路に
            // 入り、既定の nice ドメイン (例 0..100) が raw データ max (0..95) へ変わってしまう。
            .filter(|r| {
                r.min.is_some()
                    || r.max.is_some()
                    || r.suggested_min.is_some()
                    || r.suggested_max.is_some()
                    || r.begin_at_zero.is_some()
            })
            .map(|r| RadialAxis {
                min: r.min,
                max: r.max,
                suggested_min: r.suggested_min,
                suggested_max: r.suggested_max,
                // radar / polarArea の既存挙動は 0 起点なので default true。
                begin_at_zero: r.begin_at_zero.unwrap_or(true),
            })
    } else {
        None
    };

    Ok(ChartSpec {
        kind,
        series,
        categories: raw.data.labels,
        x_positions: XPositions::Category,
        x_axis: AxisSpec {
            title: axis_title_from(x_opts.and_then(|a| a.title.as_ref())),
            min: x_opts.and_then(|a| a.min),
            max: x_opts.and_then(|a| a.max),
            suggested_min: suggested_min_x,
            suggested_max: suggested_max_x,
            begin_at_zero: x_begin_at_zero,
            offset: x_offset,
            grid: axis_grid_from(x_opts.and_then(|a| a.grid.as_ref())),
            border: axis_border_from(x_opts.and_then(|a| a.border.as_ref())),
            scale_kind: if x_axis_is_log {
                ScaleKind::Logarithmic
            } else {
                ScaleKind::Linear
            },
        },
        y_axis: AxisSpec {
            title: axis_title_from(y_opts.and_then(|a| a.title.as_ref())),
            min: y_opts.and_then(|a| a.min),
            max: y_opts.and_then(|a| a.max),
            suggested_min: suggested_min_y,
            suggested_max: suggested_max_y,
            begin_at_zero: y_begin_at_zero,
            offset: y_offset,
            grid: axis_grid_from(y_opts.and_then(|a| a.grid.as_ref())),
            border: axis_border_from(y_opts.and_then(|a| a.border.as_ref())),
            scale_kind: if y_axis_is_log {
                ScaleKind::Logarithmic
            } else {
                ScaleKind::Linear
            },
        },
        legend: legend_pos(&raw.options.plugins.legend),
        legend_options: legend_options(&raw.options.plugins.legend),
        legend_title: legend_title(&raw.options.plugins.legend),
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: raw.width.unwrap_or(DEFAULT_CHART_WIDTH),
        height: raw.height.unwrap_or(DEFAULT_CHART_HEIGHT),
        size_mode: SizeMode::Canvas,
        data_labels,
        theme,
        decimation,
        radial_axis,
    })
}

/// `options.theme` を [`Theme`] へ解決する。各トークンは「指定 + 妥当」なら上書き、
/// それ以外はデフォルト値を保つ。色は `parse_color` を通し、不正値はそのトークンの
/// デフォルトにフォールバックする。パレットは妥当な要素を入力順で採り、空または
/// 全要素不正ならデフォルトパレットを使う。
fn build_theme(raw: Option<RawTheme>) -> Theme {
    let mut theme = Theme::default();
    let Some(raw) = raw else {
        return theme;
    };

    if let Some(entries) = raw.palette {
        let parsed: Vec<Color> = entries.iter().filter_map(|c| parse_color(c)).collect();
        if !parsed.is_empty() {
            theme.palette = parsed;
            theme.is_custom_palette = true;
        }
    }
    if let Some(c) = raw.grid_color.as_deref().and_then(parse_color) {
        theme.grid_color = c;
    }
    if let Some(c) = raw.text_color.as_deref().and_then(parse_color) {
        theme.text_color = c;
    }
    if let Some(c) = raw.background_color.as_deref().and_then(parse_color) {
        theme.background = Some(c);
    }
    if let Some(sz) = raw.font_size
        && sz.is_finite()
        && sz > 0.0
    {
        theme.font_size = sz;
    }
    theme
}

fn build_outlabel_config(raw: &Option<RawOutlabels>) -> crate::ir::OutlabelConfig {
    use crate::ir::OutlabelConfig;
    let mut cfg = OutlabelConfig::default();
    let Some(raw) = raw else { return cfg };
    if let Some(t) = &raw.text {
        // DoS 防止: テンプレートを MAX_LABEL_BYTES でクランプ。
        const MAX_TEMPLATE_BYTES: usize = crate::guard::DEFAULT_MAX_LABEL_BYTES;
        if t.len() <= MAX_TEMPLATE_BYTES {
            cfg.text = t.clone();
        } else {
            let mut end = MAX_TEMPLATE_BYTES;
            while !t.is_char_boundary(end) {
                end -= 1;
            }
            cfg.text = t[..end].to_string();
        }
    }
    if let Some(c) = raw.color.as_deref().and_then(parse_color) {
        cfg.color = c;
    }
    if let Some(c) = raw.background_color.as_deref().and_then(parse_color) {
        cfg.background = Some(c);
    }
    if let Some(s) = raw.stretch
        && s.is_finite()
        && s >= 0.0
    {
        cfg.stretch = s;
    }
    cfg
}

/// Chart.js の tension は 0.0〜1.0 の範囲で扱い、巨大な有限値で
/// SVG パスのコントロールポイントが膨張しないよう正規化する。
fn normalize_tension(tension: f64) -> f64 {
    if !tension.is_finite() || tension <= 0.0 {
        0.0
    } else {
        tension.min(1.0)
    }
}

fn line_interpolation(tension: f64) -> LineInterpolation {
    if tension <= 0.0 {
        LineInterpolation::Linear
    } else {
        LineInterpolation::CatmullRom { tension }
    }
}

/// 系列の既定線幅。line 系列は太く(3.0)、bar 系列は細い(1.0)。
/// chart 基本型でなく系列種別で決めることで、混合チャートの line 系列も正しく太くなる。
/// 単一種別では従来挙動(全 Line→3.0 / 非 Line→1.0)と byte 一致する。
fn default_border_width(series_type: SeriesType) -> f64 {
    match series_type {
        SeriesType::Line => 3.0,
        SeriesType::Bar => 1.0,
    }
}

/// 指定色(スカラ/配列)を点ごとの Vec<Color> に解決する。
/// 未指定: pie はスライス別パレット(n色)、それ以外は系列インデックスの 1 色。
/// 不正色のフォールバックも pie はスライス位置色・非pieは系列色で、未指定時と一貫させる。
/// 自動配色はテーマの `palette`(空でないことが保証済み)を巡回する。
fn resolve_colors(
    spec: Option<ScalarOrArray<String>>,
    is_pie: bool,
    series_index: usize,
    n: usize,
    palette: &[Color],
    default_alpha: f32,
    is_custom_palette: bool,
) -> Vec<Color> {
    let pick = |i: usize| {
        let c = palette[i % palette.len()];
        Color {
            a: if is_custom_palette && c.a < 1.0 {
                c.a
            } else {
                default_alpha
            },
            ..c
        }
    };
    match spec {
        Some(s) => s
            .into_vec()
            .iter()
            .enumerate()
            .map(|(idx, c)| {
                parse_color(c).unwrap_or_else(|| {
                    if is_pie {
                        pick(idx)
                    } else {
                        pick(series_index)
                    }
                })
            })
            .collect(),
        None if is_pie => (0..n).map(pick).collect(),
        None => vec![pick(series_index)],
    }
}

fn legend_pos(l: &Option<RawLegend>) -> LegendPos {
    match l {
        Some(l) if !l.display => LegendPos::None,
        Some(l) => match l.position.as_deref() {
            Some("bottom") => LegendPos::Bottom,
            Some("left") => LegendPos::Left,
            Some("right") => LegendPos::Right,
            _ => LegendPos::Top,
        },
        None => LegendPos::Top,
    }
}

fn legend_options(l: &Option<RawLegend>) -> LegendOptions {
    let Some(l) = l else {
        return LegendOptions::default();
    };
    let labels = l.labels.as_ref();
    let labels_font = labels.and_then(|labels| labels.font.as_ref());
    let title = l.title.as_ref();
    let title_font = title.and_then(|title| title.font.as_ref());
    LegendOptions {
        align: match l.align.as_deref() {
            Some("start") => LegendAlign::Start,
            Some("end") => LegendAlign::End,
            _ => LegendAlign::Center,
        },
        reverse: l.reverse.unwrap_or(false),
        labels_color: labels
            .and_then(|labels| labels.color.as_deref())
            .and_then(parse_color),
        labels_font_size: labels_font.and_then(|font| positive_finite(font.size)),
        labels_font_family: labels_font.and_then(|font| font.family.clone()),
        labels_font_weight: labels_font.and_then(|font| font_weight(&font.weight)),
        labels_font_style: labels_font.and_then(|font| font.style.clone()),
        labels_padding: labels.and_then(|labels| nonnegative_finite(labels.padding)),
        labels_box_width: labels.and_then(|labels| nonnegative_finite(labels.box_width)),
        labels_box_height: labels.and_then(|labels| nonnegative_finite(labels.box_height)),
        labels_use_point_style: labels
            .is_some_and(|labels| labels.use_point_style.unwrap_or(false)),
        labels_point_style: labels
            .and_then(|labels| labels.point_style.as_deref())
            .and_then(legend_point_style),
        title_display: title.is_some_and(|title| title.display.unwrap_or(false)),
        title_color: title
            .and_then(|title| title.color.as_deref())
            .and_then(parse_color),
        title_font_size: title_font.and_then(|font| positive_finite(font.size)),
        title_font_family: title_font.and_then(|font| font.family.clone()),
        title_font_weight: title_font.and_then(|font| font_weight(&font.weight)),
        title_font_style: title_font.and_then(|font| font.style.clone()),
        title_padding: title
            .and_then(|title| title.padding.as_ref())
            .map(legend_title_padding)
            .unwrap_or_default(),
    }
}

fn legend_title(l: &Option<RawLegend>) -> Option<String> {
    l.as_ref()?
        .title
        .as_ref()
        .filter(|title| title.display.unwrap_or(false))?
        .text
        .clone()
        .filter(|text| !text.is_empty())
}

fn font_weight(weight: &Option<serde_json::Value>) -> Option<String> {
    match weight.as_ref()? {
        serde_json::Value::String(weight) => Some(weight.clone()),
        serde_json::Value::Number(weight) => Some(weight.to_string()),
        _ => None,
    }
}

fn positive_finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn nonnegative_finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn legend_point_style(value: &str) -> Option<LegendPointStyle> {
    Some(match value {
        "circle" => LegendPointStyle::Circle,
        "cross" => LegendPointStyle::Cross,
        "crossRot" => LegendPointStyle::CrossRot,
        "dash" => LegendPointStyle::Dash,
        "line" => LegendPointStyle::Line,
        "rect" => LegendPointStyle::Rect,
        "rectRounded" => LegendPointStyle::RectRounded,
        "rectRot" => LegendPointStyle::RectRot,
        "star" => LegendPointStyle::Star,
        "triangle" => LegendPointStyle::Triangle,
        _ => return None,
    })
}

fn legend_title_padding(value: &serde_json::Value) -> LegendTitlePadding {
    let side = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0)
    };
    if let Some(padding) = value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
    {
        return LegendTitlePadding {
            top: padding,
            right: padding,
            bottom: padding,
            left: padding,
        };
    }
    LegendTitlePadding {
        top: side("top"),
        right: side("right"),
        bottom: side("bottom"),
        left: side("left"),
    }
}

/// strict モード用: 既知キーのホワイトリストに照らし、未知キーを検出する。
///
/// 防御的に走査し、ノードが欠落/想定外の形なら `Ok(())` を返す（後段の通常パースが
/// 適切な Err を出す）。最初に見つけた未知キーのパスを `Err` で返す。
// strict 用ホワイトリスト。chart.js v4 サブセットとして「認識済み」のキーを並べる。
// IR へ未マップでも、設計で v1 サポート対象に挙げたキーは strict でも受理する
// （strict が弾くのは未知キーであり、認識済み・未完成キーではない）:
//   datalabels=Task16(最小データラベル) / scales=Task9 / pointRadius=Task13。
fn check_unknown_keys(
    json: &str,
    allow_outlabels: bool,
    allow_radial_scale: bool,
    allow_pie: bool,
) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()), // 不正 JSON は後段パースに委ねる
    };
    let Some(top) = value.as_object() else {
        return Ok(()); // object でなければ後段パースに委ねる
    };

    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    let chart_type = top.get("type").and_then(|value| value.as_str());
    let line_root = chart_type == Some("line");
    let bar_root = chart_type == Some("bar");
    let scatter_root = chart_type == Some("scatter");
    let pie_root = matches!(chart_type, Some("pie") | Some("doughnut"));

    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    let dataset_keys: &[&str] = if line_root {
                        &[
                            "label",
                            "order",
                            "type",
                            "stack",
                            "categoryPercentage",
                            "barPercentage",
                            "barThickness",
                            "maxBarThickness",
                            "minBarLength",
                            "borderRadius",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                            "fill",
                            "tension",
                            "cubicInterpolationMode",
                            "spanGaps",
                            "stepped",
                            "pointRadius",
                            "pointStyle",
                            "showLine",
                            "borderDash",
                            "borderDashOffset",
                        ]
                    } else if bar_root {
                        &[
                            "label",
                            "order",
                            "type",
                            "stack",
                            "categoryPercentage",
                            "barPercentage",
                            "barThickness",
                            "maxBarThickness",
                            "minBarLength",
                            "borderRadius",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                            "fill",
                            "tension",
                            "cubicInterpolationMode",
                            "pointRadius",
                            "pointStyle",
                            "showLine",
                            "borderDash",
                            "borderDashOffset",
                        ]
                    } else if scatter_root {
                        &[
                            "label",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                            "fill",
                            "tension",
                            "pointRadius",
                            "pointStyle",
                            "showLine",
                            "borderDash",
                            "borderDashOffset",
                        ]
                    } else if pie_root {
                        &[
                            "label",
                            "type",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                            "spacing",
                            "offset",
                            "borderRadius",
                            "fill",
                            "tension",
                            "pointRadius",
                        ]
                    } else {
                        &[
                            "label",
                            "type",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                            "fill",
                            "tension",
                            "pointRadius",
                        ]
                    };
                    check_object(ds, dataset_keys, &format!("data.datasets[{i}]"))?;
                    // scatter/bubble の点データ {x,y,r} 各オブジェクト内のキーも検査する。
                    // RawPoint は未知キーを無視するため、ここで typo(例 radius)を検出する。
                    if let Some(points) = ds.get("data").and_then(|v| v.as_array()) {
                        for (j, pt) in points.iter().enumerate() {
                            if let Some(pt) = pt.as_object() {
                                check_object(
                                    pt,
                                    &["x", "y", "r"],
                                    &format!("data.datasets[{i}].data[{j}]"),
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        let allowed_options: &[&str] = if allow_pie {
            &["indexAxis", "plugins", "scales", "theme", "cutout"]
        } else {
            &["indexAxis", "plugins", "scales", "theme"]
        };
        check_object(options, allowed_options, "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            let allowed_plugins: &[&str] = if allow_outlabels {
                &["title", "legend", "datalabels", "outlabels", "decimation"]
            } else {
                &["title", "legend", "datalabels", "decimation"]
            };
            check_object(plugins, allowed_plugins, "options.plugins")?;
            if let Some(legend) = plugins.get("legend").and_then(|v| v.as_object()) {
                check_object(
                    legend,
                    &["display", "position", "align", "reverse", "labels", "title"],
                    "options.plugins.legend",
                )?;
                if let Some(labels) = legend.get("labels").and_then(|v| v.as_object()) {
                    check_object(
                        labels,
                        &[
                            "color",
                            "font",
                            "padding",
                            "boxWidth",
                            "boxHeight",
                            "usePointStyle",
                            "pointStyle",
                        ],
                        "options.plugins.legend.labels",
                    )?;
                    if let Some(font) = labels.get("font").and_then(|v| v.as_object()) {
                        check_object(
                            font,
                            &["size", "family", "weight", "style", "lineHeight"],
                            "options.plugins.legend.labels.font",
                        )?;
                    }
                }
                if let Some(title) = legend.get("title").and_then(|v| v.as_object()) {
                    check_object(
                        title,
                        &["display", "text", "color", "font", "padding"],
                        "options.plugins.legend.title",
                    )?;
                    if let Some(font) = title.get("font").and_then(|v| v.as_object()) {
                        check_object(
                            font,
                            &["size", "family", "weight", "style", "lineHeight"],
                            "options.plugins.legend.title.font",
                        )?;
                    }
                    if let Some(padding) = title.get("padding").and_then(|v| v.as_object()) {
                        check_object(
                            padding,
                            &["top", "right", "bottom", "left"],
                            "options.plugins.legend.title.padding",
                        )?;
                    }
                }
            }
            if let Some(dl) = plugins.get("datalabels").and_then(|v| v.as_object()) {
                check_object(dl, &["display"], "options.plugins.datalabels")?;
            }
            if let Some(dec) = plugins.get("decimation").and_then(|v| v.as_object()) {
                check_object(
                    dec,
                    &["enabled", "algorithm", "samples", "threshold"],
                    "options.plugins.decimation",
                )?;
            }
            if allow_outlabels
                && let Some(ol) = plugins.get("outlabels").and_then(|v| v.as_object())
            {
                check_object(
                    ol,
                    &["text", "color", "backgroundColor", "stretch"],
                    "options.plugins.outlabels",
                )?;
            }
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
        // scales 配下も検査する。stacked は描画に効く load-bearing キーなので、
        // typo(例 stakced)を strict で取りこぼさないようにする。各軸は設計が認める
        // サブセットのみ許可(stacked のみ実装、他は認識済み・未実装)。
        // radar/polarArea は直交軸を持たず、動径軸 `r` のみを受け付ける。
        if let Some(scales) = options.get("scales").and_then(|v| v.as_object()) {
            let allowed_axes: &[&str] = if allow_radial_scale {
                &["r"]
            } else {
                &["x", "y"]
            };
            check_object(scales, allowed_axes, "options.scales")?;
            let allowed_axis_keys: &[&str] = if allow_radial_scale {
                &["min", "max", "suggestedMin", "suggestedMax", "beginAtZero"]
            } else {
                &[
                    "stacked",
                    "min",
                    "max",
                    "type",
                    "title",
                    "grid",
                    "border",
                    "beginAtZero",
                    "suggestedMin",
                    "suggestedMax",
                    "offset",
                ]
            };
            // Codex Fix 7: axis 値が object でない (例: "r": 5) 場合は strict で拒否する。
            // 従来は as_object() の None 分岐で無音スキップされていた。chart.js では
            // 非 object の axis 値は常にエラーなので radial (r) / cartesian (x/y) 双方に適用。
            //
            // Codex Fix 10: ただし JSON null は「未指定」として扱う。schema 側の
            // `BarScales.y` / `RadialLinearScales.r` は `Option<_>` なので null は None に
            // deserialize される。optional フィールドを nullable に serialize する
            // クライアント (例: 多くの JSON エンコーダ) が strict で落ちないようにする。
            // null を許すのは axis レベルのみで、数値・文字列などの非 object 値は従来通り拒否する。
            for axis in allowed_axes {
                match scales.get(*axis) {
                    None => {}
                    // `"r": null` / `"y": null` → 軸未指定と同義。`options.scales: null`
                    // が上の as_object() で既に無視されるのと同じ扱い。
                    Some(v) if v.is_null() => {}
                    Some(ax_val) => {
                        let ax = ax_val.as_object().ok_or_else(|| {
                            format!("options.scales.{axis} は object でなければなりません")
                        })?;
                        check_object(ax, allowed_axis_keys, &format!("options.scales.{axis}"))?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn check_unknown_keys_matrix(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &[
                            "label",
                            "data",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                        ],
                        &format!("data.datasets[{i}]"),
                    )?;
                    if let Some(points) = ds.get("data").and_then(|v| v.as_array()) {
                        for (j, pt) in points.iter().enumerate() {
                            if let Some(pt) = pt.as_object() {
                                check_object(
                                    pt,
                                    &["x", "y", "v"],
                                    &format!("data.datasets[{i}].data[{j}]"),
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            // matrix の plugins は title/legend/decimation を受理する(schema 側 MatrixPlugins と一致)。
            // decimation は matrix では no-op だが Chart.js のグローバルプラグイン挙動どおり受理して
            // 無視する。datalabels は matrix が描画しないため schema・strict とも契約から外し、
            // 危険方向のパリティ破れ(schema 受理→strict 拒否)を起こさない(27k)。
            check_object(
                plugins,
                &["title", "legend", "decimation"],
                "options.plugins",
            )?;
            if let Some(dec) = plugins.get("decimation").and_then(|v| v.as_object()) {
                check_object(
                    dec,
                    &["enabled", "algorithm", "samples", "threshold"],
                    "options.plugins.decimation",
                )?;
            }
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

fn check_unknown_keys_sankey(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets", "labels"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &[
                            "label",
                            "data",
                            "colorFrom",
                            "colorTo",
                            "colorMode",
                            "hoverColorFrom",
                            "hoverColorTo",
                            "alpha",
                            "borderColor",
                            "borderWidth",
                            "color",
                            "nodeWidth",
                            "nodePadding",
                            "modeX",
                            "size",
                            "labels",
                            "priority",
                            "column",
                            "parsing",
                        ],
                        &format!("data.datasets[{i}]"),
                    )?;
                    // parsing 指定時は from/to/flow の代わりに parsing で指定された
                    // キー名を許可する(残る "color"/"colorFrom"/"colorTo" は固定)。
                    // parsing サブオブジェクト自身も strict 検証: schema (SankeyParsing)
                    // が deny_unknown_fields なので、`{"formm":"src"}` のようなタイプミスを
                    // 黙って fallback に流さず、strict モードで明示的に拒否する。
                    let p = ds.get("parsing").and_then(|v| v.as_object());
                    if let Some(p) = p {
                        check_object(
                            p,
                            &["from", "to", "flow"],
                            &format!("data.datasets[{i}].parsing"),
                        )?;
                    }
                    let mapped = |k: &str, dflt: &'static str| -> String {
                        p.and_then(|o| o.get(k))
                            .and_then(|v| v.as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| dflt.to_string())
                    };
                    let key_from = mapped("from", "from");
                    let key_to = mapped("to", "to");
                    let key_flow = mapped("flow", "flow");
                    if let Some(points) = ds.get("data").and_then(|v| v.as_array()) {
                        for (j, pt) in points.iter().enumerate() {
                            if let Some(pt) = pt.as_object() {
                                check_object(
                                    pt,
                                    &[
                                        key_from.as_str(),
                                        key_to.as_str(),
                                        key_flow.as_str(),
                                        "color",
                                        "colorFrom",
                                        "colorTo",
                                    ],
                                    &format!("data.datasets[{i}].data[{j}]"),
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            // sankey は legend を描画しないため title のみ受理する(schema と一致)。
            check_object(plugins, &["title"], "options.plugins")?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

/// gauge と radialGauge の許可キーの**和集合**（緩い上位集合）に対して検証する。
/// 型ごとの厳密な契約は JSON Schema（`schema/chartjs.rs`）が担い、そちらは型別に
/// 厳密。ランタイムの strict 検証は真に未知のキー（タイポ）だけを安全側で弾く目的で
/// あり、スキーマ妥当な入力は必ずパースできる（緩いのは安全な方向のみ）。
/// このため gauge / radialGauge を区別する必要はなく、引数を取らない。
fn check_unknown_keys_gauge(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    // gauge/radialGauge はゾーン/弧の境界線を描かないため borderColor/
                    // borderWidth は受け付けない(スキーマ・パーサと一致)。
                    check_object(
                        ds,
                        &["label", "value", "minValue", "data", "backgroundColor"],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(
            options,
            &[
                "domain",
                "trackColor",
                "centerPercentage",
                "roundedCorners",
                "centerArea",
                "needle",
                "valueLabel",
                "plugins",
                "theme",
            ],
            "options",
        )?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            // 単一ゲージには凡例が描けないため legend は受け付けない(スキーマと一致)。
            check_object(plugins, &["title"], "options.plugins")?;
        }
        if let Some(ca) = options.get("centerArea").and_then(|v| v.as_object()) {
            check_object(
                ca,
                &[
                    "displayText",
                    "fontSize",
                    "fontColor",
                    "text",
                    "subText",
                    "padding",
                ],
                "options.centerArea",
            )?;
        }
        if let Some(nd) = options.get("needle").and_then(|v| v.as_object()) {
            // 針サイズ系(*Percentage)はスキーマ非公開・内部固定のため許可しない(color のみ)。
            check_object(nd, &["color"], "options.needle")?;
        }
        if let Some(vl) = options.get("valueLabel").and_then(|v| v.as_object()) {
            check_object(
                vl,
                &[
                    "display",
                    "formatter",
                    "color",
                    "backgroundColor",
                    "borderRadius",
                    "padding",
                    "bottomMarginPercentage",
                    "fontSize",
                ],
                "options.valueLabel",
            )?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

/// progress / progressBar の許可キーに対して検証する。
/// stroke を描かないため borderColor/borderWidth は受け付けない。
/// legend は描画しないため受け付けない（datalabels は % 表示制御に使用するため許可）。
fn check_unknown_keys_progress(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &["label", "data", "backgroundColor"],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title", "datalabels"], "options.plugins")?;
            if let Some(dl) = plugins.get("datalabels").and_then(|v| v.as_object()) {
                check_object(dl, &["display"], "options.plugins.datalabels")?;
            }
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

/// treemap 専用パース。`tree`(数値配列 or オブジェクト配列) + `key` + `groups` を
/// 挿入順でグルーピング・合算して TreeNode forest を構築する。
fn parse_treemap(json: &str) -> Result<ChartSpec, String> {
    use crate::ir::TreeNode;

    #[derive(Deserialize)]
    struct TreemapWrapper {
        data: TreemapRawData,
        #[serde(default)]
        options: RawOptions,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }
    #[derive(Deserialize)]
    struct TreemapRawData {
        datasets: Vec<TreemapRawDataset>,
    }
    #[derive(Deserialize)]
    struct TreemapRawDataset {
        #[allow(dead_code)]
        #[serde(default)]
        label: String,
        tree: TreeField,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        groups: Vec<String>,
    }
    /// `tree`: 数値配列(フラット) または オブジェクト配列(groups でグルーピング)。
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TreeField {
        Nums(Vec<f64>),
        Objs(Vec<serde_json::Map<String, serde_json::Value>>),
    }

    let raw: TreemapWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.len() != 1 {
        return Err("treemap チャートには dataset が 1 つ必要です".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();

    // groups 階層の深さを再帰前に検証する (build_tree_forest のスタックオーバーフロー/DoS 対策)。
    if ds.groups.len() > MAX_TREEMAP_GROUP_DEPTH {
        return Err(format!(
            "treemap の groups 階層が深すぎます (上限 {MAX_TREEMAP_GROUP_DEPTH})"
        ));
    }

    let forest: Vec<TreeNode> = match ds.tree {
        TreeField::Nums(nums) => {
            // object 分岐と対称に、TreeNode 構築前に生入力件数を上限化する
            // (各数値が String + 子 Vec を持つ TreeNode を確保するため)。
            if nums.len() > MAX_TREEMAP_INPUT_ROWS {
                return Err(format!(
                    "treemap の入力データ件数が多すぎます (上限 {MAX_TREEMAP_INPUT_ROWS})"
                ));
            }
            nums.into_iter()
                .map(|v| TreeNode {
                    label: String::new(),
                    value: v,
                    children: vec![],
                })
                .collect()
        }
        TreeField::Objs(objs) => {
            if objs.len() > MAX_TREEMAP_INPUT_ROWS {
                return Err(format!(
                    "treemap の入力データ件数が多すぎます (上限 {MAX_TREEMAP_INPUT_ROWS})"
                ));
            }
            let key = ds
                .key
                .as_deref()
                .ok_or("treemap: オブジェクト tree には key が必要です")?;
            if ds.groups.is_empty() {
                objs.iter()
                    .map(|o| TreeNode {
                        label: String::new(),
                        value: obj_num(o, key).max(0.0),
                        children: vec![],
                    })
                    .collect()
            } else {
                build_tree_forest(&objs, &ds.groups, key)
            }
        }
    };

    // ノード総数の上限 (DoS 対策、matrix の 10000 セル上限に揃える)。
    if count_nodes(&forest) > 10_000 {
        return Err("treemap のノード数が多すぎます (上限 10000)".to_string());
    }

    let theme = build_theme(raw.options.theme);
    let no_axis = AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: false,
        grid: AxisGrid {
            display: false,
            ..Default::default()
        },
        border: AxisBorder::default(),
        scale_kind: ScaleKind::Linear,
    };

    let series = vec![Series {
        name: String::new(),
        values: vec![],
        points: vec![],
        fill: vec![],
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        area_fill: None,
        interpolation: LineInterpolation::Linear,
        span_gaps: false,
        step_mode: None,
        line_style: None,
        stack: None,
        bar_geometry: None,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: forest,
        links: vec![],
    }];

    Ok(ChartSpec {
        kind: ChartKind::Treemap,
        series,
        categories: vec![],
        x_positions: XPositions::Category,
        x_axis: no_axis.clone(),
        y_axis: no_axis,
        legend: crate::ir::LegendPos::None,
        legend_options: crate::ir::LegendOptions::default(),
        legend_title: None,
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: raw.width.unwrap_or(DEFAULT_CHART_WIDTH),
        height: raw.height.unwrap_or(DEFAULT_CHART_HEIGHT),
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

/// オブジェクトから数値プロパティを読む (欠落/非数値は 0.0)。
fn obj_num(o: &serde_json::Map<String, serde_json::Value>, key: &str) -> f64 {
    o.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// オブジェクトからグルーピングキーを文字列として読む。
fn obj_group_key(o: &serde_json::Map<String, serde_json::Value>, field: &str) -> String {
    match o.get(field) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// オブジェクト群を groups[0] でグルーピング(挿入順保持)し、groups[1..] で再帰。
/// 各レベルの value は子の合算 (最深レベルは key の合算)。
fn build_tree_forest(
    objs: &[serde_json::Map<String, serde_json::Value>],
    groups: &[String],
    key: &str,
) -> Vec<crate::ir::TreeNode> {
    use crate::ir::TreeNode;
    let field = &groups[0];
    let mut order: Vec<String> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut buckets: Vec<Vec<serde_json::Map<String, serde_json::Value>>> = Vec::new();
    for o in objs {
        let gk = obj_group_key(o, field);
        let bi = *idx.entry(gk.clone()).or_insert_with(|| {
            order.push(gk);
            buckets.push(Vec::new());
            buckets.len() - 1
        });
        buckets[bi].push(o.clone());
    }

    order
        .into_iter()
        .zip(buckets)
        .map(|(label, bucket)| {
            // 集約が +Inf に overflow すると layout が非有限の面積を 0 扱いして空描画に
            // なるため、合計を有限値(f64::MAX)にクランプして正の親値を保つ。
            if groups.len() == 1 {
                let value = bucket
                    .iter()
                    .map(|o| obj_num(o, key).max(0.0))
                    .sum::<f64>()
                    .min(f64::MAX);
                TreeNode {
                    label,
                    value,
                    children: vec![],
                }
            } else {
                let children = build_tree_forest(&bucket, &groups[1..], key);
                let value = children.iter().map(|c| c.value).sum::<f64>().min(f64::MAX);
                TreeNode {
                    label,
                    value,
                    children,
                }
            }
        })
        .collect()
}

/// treemap の groups 階層深さの上限 (スタックオーバーフロー/DoS 対策)。
/// 実用上 treemap が 50 段を超えることはない。
const MAX_TREEMAP_GROUP_DEPTH: usize = 50;

/// treemap のオブジェクト入力行数の上限。集約 (build_tree_forest) は各グループ階層で
/// object を clone するため、ノード数上限とは別に生入力件数も制限して DoS を防ぐ。
const MAX_TREEMAP_INPUT_ROWS: usize = 10_000;

/// treemap の forest 内ノード総数を再帰的に数える (DoS ガード用)。
fn count_nodes(nodes: &[crate::ir::TreeNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// treemap の許可キーを検証する (strict モード)。
fn check_unknown_keys_treemap(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        // treemap は palette/depth 配色で dataset レベルの色(backgroundColor)・
                        // 枠線(borderColor/borderWidth)を honor しないため許可しない
                        // (schema TreemapDataset とも一致させる)。
                        &["label", "tree", "key", "groups"],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            // treemap は凡例を描かない(LegendPos::None 固定)ため legend は許可しない。
            check_object(plugins, &["title"], "options.plugins")?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

fn check_unknown_keys_wordcloud(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    // fulgur 拡張の width/height をトップレベルで許可する（他チャート種別と同様）
    check_object(top, &["type", "data", "options", "width", "height"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &["label", "data", "color"],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["elements", "plugins", "theme"], "options")?;
        if let Some(elements) = options.get("elements").and_then(|v| v.as_object()) {
            check_object(elements, &["word"], "options.elements")?;
            if let Some(word) = elements.get("word").and_then(|v| v.as_object()) {
                check_object(
                    word,
                    &["minRotation", "maxRotation", "rotationSteps", "padding"],
                    "options.elements.word",
                )?;
            }
        }
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title"], "options.plugins")?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}

fn parse_matrix(json: &str) -> Result<ChartSpec, String> {
    #[derive(Deserialize)]
    struct MatrixWrapper {
        data: MatrixRawData,
        #[serde(default)]
        options: RawOptions,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }

    #[derive(Deserialize)]
    struct MatrixRawData {
        datasets: Vec<MatrixRawDataset>,
    }

    #[derive(Deserialize)]
    struct MatrixRawDataset {
        #[allow(dead_code)]
        #[serde(default)]
        label: String,
        data: Vec<MatrixRawCell>,
        #[serde(rename = "backgroundColor", default)]
        background_color: Option<ScalarOrArray<String>>,
        #[serde(rename = "borderColor", default)]
        border_color: Option<ScalarOrArray<String>>,
        #[serde(rename = "borderWidth", default)]
        border_width: Option<f64>,
    }

    #[derive(Deserialize)]
    struct MatrixRawCell {
        x: String,
        y: String,
        v: f64,
    }

    let raw: MatrixWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;

    if raw.data.datasets.len() > 1 {
        return Err("matrix チャートは dataset が 1 つのみサポートされます".to_string());
    }
    if raw.data.datasets.is_empty() {
        return Err("matrix チャートには dataset が 1 つ必要です".to_string());
    }

    let ds = raw.data.datasets.into_iter().next().unwrap();

    // x/y カテゴリを出現順に収集（重複除去）— HashMap で O(n) ルックアップ
    let mut x_cats: Vec<String> = Vec::new();
    let mut x_idx: HashMap<String, usize> = HashMap::new();
    let mut y_cats: Vec<String> = Vec::new();
    let mut y_idx: HashMap<String, usize> = HashMap::new();
    for cell in &ds.data {
        if !x_idx.contains_key(&cell.x) {
            x_idx.insert(cell.x.clone(), x_cats.len());
            x_cats.push(cell.x.clone());
        }
        if !y_idx.contains_key(&cell.y) {
            y_idx.insert(cell.y.clone(), y_cats.len());
            y_cats.push(cell.y.clone());
        }
    }

    let n_cols = x_cats.len();
    let n_rows = y_cats.len();

    // グリッドサイズ上限チェック
    if n_cols.saturating_mul(n_rows) > 10_000 {
        return Err(format!(
            "matrix grid too large: {}×{} = {} cells (limit 10000)",
            n_cols,
            n_rows,
            n_cols * n_rows
        ));
    }

    // NaN で初期化したグリッドを構築
    let mut grid: Vec<Vec<f64>> = vec![vec![f64::NAN; n_cols]; n_rows];
    for cell in &ds.data {
        let ci = x_idx[&cell.x];
        let ri = y_idx[&cell.y];
        grid[ri][ci] = cell.v;
    }

    let theme = build_theme(raw.options.theme);

    let color_hi = ds
        .background_color
        .as_ref()
        .and_then(|c| match c {
            ScalarOrArray::One(v) => parse_color(v),
            ScalarOrArray::Many(vs) => vs.first().and_then(|v| parse_color(v)),
        })
        .unwrap_or(theme.palette[0]);
    let color_lo = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 1.0,
    };

    let stroke_color: Vec<Color> = ds
        .border_color
        .as_ref()
        .and_then(|c| match c {
            ScalarOrArray::One(v) => parse_color(v),
            ScalarOrArray::Many(vs) => vs.first().and_then(|v| parse_color(v)),
        })
        .map(|c| vec![c])
        .unwrap_or_default();

    let series: Vec<Series> = y_cats
        .iter()
        .enumerate()
        .map(|(i, name)| Series {
            name: name.clone(),
            values: grid[i].clone(),
            points: vec![],
            fill: vec![color_hi],
            stroke: stroke_color.clone(),
            stroke_width: ds.border_width.unwrap_or(0.0),
            area: false,
            area_fill: None,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            line_style: None,
            stack: None,
            bar_geometry: None,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        })
        .collect();

    Ok(ChartSpec {
        kind: ChartKind::Matrix { color_lo, color_hi },
        series,
        categories: x_cats,
        x_positions: XPositions::Category,
        x_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None,
            suggested_max: None,
            begin_at_zero: false,
            offset: false,
            grid: AxisGrid {
                display: false,
                ..Default::default()
            },
            border: AxisBorder::default(),
            scale_kind: ScaleKind::Linear,
        },
        y_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None,
            suggested_max: None,
            begin_at_zero: false,
            offset: false,
            grid: AxisGrid {
                display: false,
                ..Default::default()
            },
            border: AxisBorder::default(),
            scale_kind: ScaleKind::Linear,
        },
        legend: legend_pos(&raw.options.plugins.legend),
        legend_options: legend_options(&raw.options.plugins.legend),
        legend_title: legend_title(&raw.options.plugins.legend),
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: raw.width.unwrap_or(DEFAULT_CHART_WIDTH),
        height: raw.height.unwrap_or(DEFAULT_CHART_HEIGHT),
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

fn parse_sankey(json: &str) -> Result<ChartSpec, String> {
    use crate::ir::{ChartKind, SankeyColorMode, SankeyLink, SankeyModeX, SankeySize};
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct W {
        data: D,
        // Accept an explicit `options: null` as the default (schema renders options nullable).
        #[serde(default, deserialize_with = "null_or_default")]
        options: RawOptions,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }
    #[derive(Deserialize)]
    struct D {
        datasets: Vec<DS>,
    }
    #[derive(Deserialize)]
    struct DS {
        #[serde(default)]
        label: String,
        data: Vec<serde_json::Value>,
        #[serde(rename = "colorFrom", default)]
        color_from: Option<String>,
        #[serde(rename = "colorTo", default)]
        color_to: Option<String>,
        #[serde(rename = "colorMode", default)]
        color_mode: Option<String>,
        #[serde(rename = "hoverColorFrom", default)]
        hover_color_from: Option<String>,
        #[serde(rename = "hoverColorTo", default)]
        hover_color_to: Option<String>,
        #[serde(default)]
        alpha: Option<f64>,
        #[serde(rename = "borderColor", default)]
        border_color: Option<String>,
        #[serde(rename = "borderWidth", default)]
        border_width: Option<f64>,
        #[serde(default)]
        color: Option<String>,
        #[serde(rename = "nodeWidth", default)]
        node_width: Option<f64>,
        #[serde(rename = "nodePadding", default)]
        node_padding: Option<f64>,
        #[serde(rename = "modeX", default)]
        mode_x: Option<String>,
        #[serde(default)]
        size: Option<String>,
        #[serde(default)]
        labels: Option<HashMap<String, String>>,
        #[serde(default)]
        priority: Option<HashMap<String, f64>>,
        #[serde(default)]
        column: Option<HashMap<String, u32>>,
        #[serde(default, deserialize_with = "deserialize_sankey_parsing_opt")]
        parsing: Option<Parsing>,
    }
    #[derive(Deserialize)]
    struct Parsing {
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        flow: Option<String>,
    }

    // chartjs-chart-sankey は `parsing: false` を「remap しない」意で受け付ける。
    // fulgur-chart の data は既に {from,to,flow} 形式なので false は parsing 未指定と
    // 等価。object → Some(Parsing) / false → None / true → 明示エラー。
    fn deserialize_sankey_parsing_opt<'de, D>(d: D) -> Result<Option<Parsing>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Object(Parsing),
            Bool(bool),
        }
        match Option::<Repr>::deserialize(d)? {
            None | Some(Repr::Bool(false)) => Ok(None),
            Some(Repr::Object(p)) => Ok(Some(p)),
            Some(Repr::Bool(true)) => Err(serde::de::Error::custom(
                "dataset.parsing accepts an object or `false`; `true` is not supported",
            )),
        }
    }

    let raw: W = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.len() != 1 {
        return Err("sankey requires exactly one dataset".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();

    // parsing の effective key。未指定なら default 名 (from/to/flow) を使う。
    // 指定時は入力 JSON からそのキー名で値を取り出す(chartjs 挙動)。
    // per-link color/colorFrom/colorTo は parsing で remap しない(chartjs 互換)。
    let parsing = ds.parsing.as_ref();
    let parsing_active = ds.parsing.is_some();
    let key_from = parsing.and_then(|p| p.from.as_deref()).unwrap_or("from");
    let key_to = parsing.and_then(|p| p.to.as_deref()).unwrap_or("to");
    let key_flow = parsing.and_then(|p| p.flow.as_deref()).unwrap_or("flow");

    // リンク構築: 各要素を Value としてパースし、parsing で指定された effective key で
    // from/to/flow を拾ってから正規化 Value を組み立てる。parsing 未指定なら key_* は
    // default 名なので、既存 spec に対する挙動は変わらない。
    // per-link 色 precedence:
    //   effective_from = flow.color_from ?? flow.color ?? None  (None なら dataset フォールバック)
    //   effective_to   = flow.color_to   ?? flow.color ?? None  (None なら dataset フォールバック)
    // 不正な色文字列は明示エラー(silent default にしない)。
    let mut links = Vec::with_capacity(ds.data.len());
    for (i, raw_entry) in ds.data.into_iter().enumerate() {
        let obj = raw_entry
            .as_object()
            .ok_or_else(|| format!("sankey data[{i}] must be an object"))?;
        let take_str = |k: &str| -> Result<String, String> {
            let hint = if parsing_active {
                " (mapped via dataset.parsing)"
            } else {
                ""
            };
            let v = obj
                .get(k)
                .ok_or_else(|| format!("sankey data[{i}] missing key '{k}'{hint}"))?;
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("sankey data[{i}].{k} must be a string"))
        };
        let take_num = |k: &str| -> Result<f64, String> {
            let hint = if parsing_active {
                " (mapped via dataset.parsing)"
            } else {
                ""
            };
            let v = obj
                .get(k)
                .ok_or_else(|| format!("sankey data[{i}] missing key '{k}'{hint}"))?;
            v.as_f64()
                .ok_or_else(|| format!("sankey data[{i}].{k} must be a number"))
        };
        let from = take_str(key_from)?;
        let to = take_str(key_to)?;
        let flow = take_num(key_flow)?;
        if !flow.is_finite() || flow < 0.0 {
            return Err("sankey flow must be a non-negative finite number".to_string());
        }
        // per-link color は常に固定キー ("color"/"colorFrom"/"colorTo") で読む。
        // 値が存在するが文字列でない場合は silent-ignore せず明示エラー(Phase B の
        // typed struct 挙動を維持)。ただし schema 側で Option<ColorString> は nullable
        // なので、明示的な null は "未指定" と等価に扱う。
        // また、parsing で from/to/flow を "color"/"colorFrom"/"colorTo" に再マップした
        // 場合は同一キーがノードIDや flow 値と衝突するため、色としては読まない。
        let take_color = |name: &str| -> Result<Option<String>, String> {
            if name == key_from || name == key_to || name == key_flow {
                return Ok(None);
            }
            match obj.get(name) {
                None => Ok(None),
                Some(v) if v.is_null() => Ok(None),
                Some(v) => match v.as_str() {
                    Some(s) => Ok(Some(s.to_owned())),
                    None => Err(format!("sankey data[{i}].{name} must be a string")),
                },
            }
        };
        let color = take_color("color")?;
        let color_from_str = take_color("colorFrom")?;
        let color_to_str = take_color("colorTo")?;
        let parse_flow_color = |name: &str, s: &str| -> Result<Color, String> {
            parse_color(s)
                .ok_or_else(|| format!("sankey data[{i}].{name} is not a valid color: {s}"))
        };
        let shared = match color.as_deref() {
            Some(s) => Some(parse_flow_color("color", s)?),
            None => None,
        };
        let cf = match color_from_str.as_deref() {
            Some(s) => Some(parse_flow_color("colorFrom", s)?),
            None => shared,
        };
        let ct = match color_to_str.as_deref() {
            Some(s) => Some(parse_flow_color("colorTo", s)?),
            None => shared,
        };
        links.push(SankeyLink {
            from,
            to,
            flow,
            color_from: cf,
            color_to: ct,
        });
    }

    let theme = build_theme(raw.options.theme);
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    };
    let green = Color {
        r: 0,
        g: 128,
        b: 0,
        a: 1.0,
    };
    let black = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };

    let color_from = ds
        .color_from
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(red);
    let color_to = ds
        .color_to
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(green);
    // hoverColorFrom / hoverColorTo は静的レンダラでは描画されないため IR に流さないが、
    // 指定時は色値としてパース可能かは検証する(silent ignore を防ぐ)。
    if let Some(s) = ds.hover_color_from.as_deref()
        && parse_color(s).is_none()
    {
        return Err(format!("sankey hoverColorFrom is not a valid color: {s}"));
    }
    if let Some(s) = ds.hover_color_to.as_deref()
        && parse_color(s).is_none()
    {
        return Err(format!("sankey hoverColorTo is not a valid color: {s}"));
    }
    // 未知値(タイポ)は silent default にせず明示エラーにする(schema の enum 制約と一致)。
    let color_mode = match ds.color_mode.as_deref() {
        None | Some("gradient") => SankeyColorMode::Gradient,
        Some("from") => SankeyColorMode::From,
        Some("to") => SankeyColorMode::To,
        Some(other) => return Err(format!("unsupported sankey colorMode: {other}")),
    };
    let alpha = ds.alpha.map(|a| a as f32).unwrap_or(0.5).clamp(0.0, 1.0);
    let border = ds
        .border_color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(black);
    let border_width = ds.border_width.unwrap_or(1.0);
    let label_color = ds.color.as_deref().and_then(parse_color).unwrap_or(black);
    let node_width = ds.node_width.unwrap_or(10.0);
    let node_padding = ds.node_padding.unwrap_or(10.0);
    // 寸法は [0, MAX_DIMENSION_PX] に収める。負値は <rect width="-5"> 等の不正 SVG を生み、
    // 巨大な有限値(例 nodePadding=1e308)は layout の (max_y/height)*node_padding で ∞ に
    // overflow し py(inf)=0 で図形を潰す。canvas 最大寸法を超える寸法は無意味なので拒否する。
    let max_dim = crate::guard::DEFAULT_MAX_DIMENSION_PX;
    for (name, v) in [
        ("nodeWidth", node_width),
        ("nodePadding", node_padding),
        ("borderWidth", border_width),
    ] {
        if !v.is_finite() || v < 0.0 || v > max_dim {
            return Err(format!(
                "sankey {name} must be within [0, {max_dim}] (got {v})"
            ));
        }
    }
    let mode_x = match ds.mode_x.as_deref() {
        None | Some("edge") => SankeyModeX::Edge,
        Some("even") => SankeyModeX::Even,
        Some(other) => return Err(format!("unsupported sankey modeX: {other}")),
    };
    let size = match ds.size.as_deref() {
        None | Some("max") => SankeySize::Max,
        Some("min") => SankeySize::Min,
        Some(other) => return Err(format!("unsupported sankey size: {other}")),
    };
    let labels = ds.labels.unwrap_or_default();
    let priority = ds.priority.unwrap_or_default();
    let columns: HashMap<String, usize> = ds
        .column
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, v as usize))
        .collect();

    let series = vec![Series {
        // 他のパーサと同様、dataset の label を Series.name に保持する(inspect/bindings で観測可能)。
        name: ds.label,
        values: vec![],
        points: vec![],
        fill: vec![],
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        area_fill: None,
        interpolation: LineInterpolation::Linear,
        span_gaps: false,
        step_mode: None,
        line_style: None,
        stack: None,
        bar_geometry: None,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: vec![],
        links,
    }];

    Ok(ChartSpec {
        kind: ChartKind::Sankey {
            color_from,
            color_to,
            color_mode,
            alpha,
            node_width,
            node_padding,
            mode_x,
            size,
            border,
            border_width,
            label_color,
            labels,
            priority,
            columns,
        },
        series,
        categories: vec![],
        x_positions: XPositions::Category,
        x_axis: zero_axis(),
        y_axis: zero_axis(),
        legend: crate::ir::LegendPos::None,
        legend_options: crate::ir::LegendOptions::default(),
        legend_title: None,
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: raw.width.unwrap_or(DEFAULT_CHART_WIDTH),
        height: raw.height.unwrap_or(DEFAULT_CHART_HEIGHT),
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

fn parse_gauge(json: &str, radial: bool) -> Result<ChartSpec, String> {
    use crate::ir::ChartKind;

    #[derive(Deserialize)]
    struct GaugeWrapper {
        data: GaugeRawData,
        #[serde(default)]
        options: serde_json::Value,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }
    #[derive(Deserialize)]
    struct GaugeRawData {
        datasets: Vec<GaugeRawDataset>,
    }
    #[derive(Deserialize)]
    struct GaugeRawDataset {
        #[serde(default)]
        value: Option<f64>,
        #[serde(rename = "minValue", default)]
        min_value: Option<f64>,
        #[serde(default)]
        data: Vec<f64>,
        #[serde(rename = "backgroundColor", default)]
        background_color: Option<ScalarOrArray<String>>,
    }

    let raw: GaugeWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    // gauge/radialGauge は 1 dataset = 1 ゲージ。余剰 dataset を無言で捨てない(matrix と同様)。
    if raw.data.datasets.len() != 1 {
        return Err("gauge/radialGauge チャートには dataset が 1 つ必要です".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();
    let opt = &raw.options;
    let raw_theme: Option<RawTheme> = opt
        .get("theme")
        .and_then(|t| serde_json::from_value(t.clone()).ok());
    let theme = build_theme(raw_theme);

    // タイトル(options.plugins.title.display/text)。
    let title = opt
        .get("plugins")
        .and_then(|p| p.get("title"))
        .filter(|t| t.get("display").and_then(|d| d.as_bool()).unwrap_or(false))
        .and_then(|t| {
            t.get("text")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        });

    // 色解決ヘルパ(背景色配列を Color に)。
    let colors: Vec<crate::ir::Color> = ds
        .background_color
        .map(|c| c.into_vec())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, s)| parse_color(s).unwrap_or_else(|| theme.palette[i % theme.palette.len()]))
        .collect();

    let (kind, values, fill) = if radial {
        // radialGauge: data[0]=値、color[0]=塗り色、domain/track/centerPercentage/...
        // 値は単一。余剰要素を無言で捨てない。
        if ds.data.len() != 1 {
            return Err("radialGauge の datasets[0].data は単一値のみ対応です".to_string());
        }
        let value = ds.data.first().copied().unwrap_or(0.0);
        let domain = opt.get("domain").and_then(|d| d.as_array());
        let min = domain
            .and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let max = domain
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);
        let track = opt
            .get("trackColor")
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color {
                r: 204,
                g: 221,
                b: 238,
                a: 1.0,
            });
        let center_pct = opt
            .get("centerPercentage")
            .and_then(|v| v.as_f64())
            .filter(|p| p.is_finite() && *p >= 0.0 && *p < 100.0)
            .unwrap_or(80.0);
        let rounded = opt
            .get("roundedCorners")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let display_text = opt
            .get("centerArea")
            .and_then(|c| c.get("displayText"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        // centerArea.fontSize: 指定時は中央値テキストのサイズを上書き(未指定は内径比で自動)。
        let center_font_size = opt
            .get("centerArea")
            .and_then(|c| c.get("fontSize"))
            .and_then(|v| v.as_f64())
            .filter(|s| s.is_finite() && *s > 0.0);
        let fill = if colors.is_empty() {
            vec![theme.palette[0]]
        } else {
            vec![colors[0]]
        };
        (
            ChartKind::RadialGauge {
                min,
                max,
                track,
                inner_ratio: center_pct / 100.0,
                rounded,
                display_text,
                center_font_size,
            },
            vec![value],
            fill,
        )
    } else {
        // gauge: data=累積閾値、value=針、min=minValue、backgroundColor=ゾーン色。
        let value = ds.value.unwrap_or(0.0);
        let min = ds.min_value.unwrap_or(0.0);
        let needle = opt
            .get("needle")
            .and_then(|n| n.get("color"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            });
        let vl = opt.get("valueLabel");
        let label = vl
            .and_then(|v| v.get("display"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let label_color = vl
            .and_then(|v| v.get("color"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 1.0,
            });
        let label_bg = vl
            .and_then(|v| v.get("backgroundColor"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            });
        // ゾーン色: 未指定はパレットをゾーンごとに割り当て、指定があれば fill_at の
        // ブロードキャスト/巡回規則に委ねる(スカラ "#f00" は全ゾーンへブロードキャスト、
        // 配列はゾーンごと、足りなければ巡回)。
        let n = ds.data.len();
        let fill: Vec<crate::ir::Color> = if colors.is_empty() {
            (0..n)
                .map(|i| theme.palette[i % theme.palette.len()])
                .collect()
        } else {
            colors
        };
        (
            ChartKind::Gauge {
                value,
                min,
                needle,
                label,
                label_color,
                label_bg,
            },
            ds.data.clone(),
            fill,
        )
    };

    let series = vec![Series {
        name: String::new(),
        values,
        points: vec![],
        fill,
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        area_fill: None,
        interpolation: LineInterpolation::Linear,
        span_gaps: false,
        step_mode: None,
        line_style: None,
        stack: None,
        bar_geometry: None,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: vec![],
        links: vec![],
    }];

    Ok(ChartSpec {
        kind,
        series,
        categories: vec![],
        x_positions: XPositions::Category,
        x_axis: zero_axis(),
        y_axis: zero_axis(),
        legend: LegendPos::None,
        legend_options: crate::ir::LegendOptions::default(),
        legend_title: None,
        title,
        width: raw.width.unwrap_or(DEFAULT_CHART_WIDTH),
        height: raw.height.unwrap_or(DEFAULT_CHART_HEIGHT),
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

/// gauge 用の最小 AxisSpec(軸を使わないチャート向け)。
fn zero_axis() -> AxisSpec {
    AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: false,
        grid: AxisGrid {
            display: false,
            draw_ticks: false,
            ..Default::default()
        },
        border: AxisBorder {
            display: false,
            ..Default::default()
        },
        scale_kind: ScaleKind::Linear,
    }
}

/// wordCloud 専用パース。labels + datasets[0].data を WordEntry に変換する。
fn parse_wordcloud(json: &str) -> Result<ChartSpec, String> {
    #[derive(serde::Deserialize)]
    struct WcWrapper {
        data: WcData,
        #[serde(default)]
        options: Option<WcOptions>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }
    #[derive(serde::Deserialize)]
    struct WcData {
        labels: Vec<String>,
        datasets: Vec<WcDataset>,
    }
    #[derive(serde::Deserialize)]
    struct WcDataset {
        data: Vec<f64>,
        #[serde(default)]
        color: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize, Default)]
    struct WcOptions {
        #[serde(default)]
        elements: Option<WcElements>,
        #[serde(default)]
        plugins: Option<serde_json::Value>,
        #[serde(default)]
        theme: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize, Default)]
    struct WcElements {
        #[serde(default)]
        word: Option<WcWordOpts>,
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct WcWordOpts {
        #[serde(default)]
        min_rotation: Option<f64>,
        #[serde(default)]
        max_rotation: Option<f64>,
        #[serde(default)]
        rotation_steps: Option<u32>,
        #[serde(default)]
        padding: Option<f64>,
    }

    let raw: WcWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.len() != 1 {
        return Err(format!(
            "wordCloud チャートは dataset が 1 つのみサポートされます ({}件指定)",
            raw.data.datasets.len()
        ));
    }
    let ds = &raw.data.datasets[0];
    if ds.data.len() != raw.data.labels.len() {
        return Err(format!(
            "wordCloud: labels ({}) と data ({}) の長さが一致しません",
            raw.data.labels.len(),
            ds.data.len(),
        ));
    }

    // color の解析（スカラー or 配列）
    let n = ds.data.len();
    let colors: Vec<Option<crate::ir::Color>> = match &ds.color {
        None => vec![None; n],
        Some(serde_json::Value::String(s)) => {
            let c = parse_color(s);
            vec![c; n]
        }
        Some(serde_json::Value::Array(arr)) => (0..n)
            .map(|i| arr.get(i).and_then(|v| v.as_str()).and_then(parse_color))
            .collect(),
        _ => vec![None; n],
    };

    let entries: Vec<crate::ir::WordEntry> = raw
        .data
        .labels
        .iter()
        .zip(ds.data.iter())
        .zip(colors.iter())
        .map(|((text, &size), color)| crate::ir::WordEntry {
            text: text.clone(),
            size,
            color: *color,
        })
        .collect();

    let word_opts = raw
        .options
        .as_ref()
        .and_then(|o| o.elements.as_ref())
        .and_then(|e| e.word.as_ref());

    let min_rotation = word_opts.and_then(|w| w.min_rotation).unwrap_or(-90.0);
    let max_rotation = word_opts.and_then(|w| w.max_rotation).unwrap_or(0.0);
    let rotation_steps = word_opts.and_then(|w| w.rotation_steps).unwrap_or(2).max(1);
    let padding = word_opts.and_then(|w| w.padding).unwrap_or(2.0);

    // title
    let title = raw
        .options
        .as_ref()
        .and_then(|o| o.plugins.as_ref())
        .and_then(|p| p.get("title"))
        .and_then(|t| {
            if t.get("display").and_then(|v| v.as_bool()).unwrap_or(false) {
                t.get("text")?.as_str().map(|s| s.to_string())
            } else {
                None
            }
        });

    // theme
    let raw_theme = raw
        .options
        .as_ref()
        .and_then(|o| o.theme.as_ref())
        .and_then(|t| serde_json::from_value::<RawTheme>(t.clone()).ok());
    let theme = build_theme(raw_theme);

    Ok(ChartSpec {
        kind: ChartKind::WordCloud {
            entries,
            min_rotation,
            max_rotation,
            rotation_steps,
            padding,
        },
        series: vec![],
        categories: vec![],
        x_positions: XPositions::Category,
        x_axis: zero_axis(),
        y_axis: zero_axis(),
        legend: LegendPos::None,
        legend_options: crate::ir::LegendOptions::default(),
        legend_title: None,
        title,
        width: raw.width.unwrap_or(500.0),
        height: raw.height.unwrap_or(300.0),
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

/// `obj` のキーを `allowed` に照らし、最初の未知キーを `Err(パス)` で返す。
fn check_object(
    obj: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    path: &str,
) -> Result<(), String> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            let full = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            return Err(format!("未知のキー: {full}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_group_ids_use_chart_type_defaults_and_are_allowed_in_strict_mode() {
        let bar = parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1]},{"data":[2],"stack":"bar"},{"data":[3],"stack":"fruit"}
            ]}}"#,
            true,
        )
        .expect("strict bar stack parse");
        assert_eq!(bar.series[0].stack.as_deref(), Some("bar"));
        assert_eq!(bar.series[1].stack.as_deref(), Some("bar"));
        assert_eq!(bar.series[2].stack.as_deref(), Some("fruit"));

        let line = parse(
            r#"{"type":"line","data":{"labels":["A"],"datasets":[
              {"data":[1]},{"data":[2],"stack":"line"}
            ]}}"#,
            true,
        )
        .expect("strict line stack parse");
        assert_eq!(line.series[0].stack.as_deref(), Some("line"));
        assert_eq!(line.series[1].stack.as_deref(), Some("line"));
    }

    #[test]
    fn parse_boxplot_basic() {
        let json = r#"{
            "type": "boxplot",
            "data": {
                "labels": ["Mon", "Tue"],
                "datasets": [{
                    "label": "Values",
                    "data": [
                        [10, 25, 50, 75, 90],
                        [5, 20, 45, 70, 95]
                    ]
                }]
            }
        }"#;
        let spec = parse(json, false).expect("parse error");
        assert!(matches!(spec.kind, crate::ir::ChartKind::BoxPlot));
        assert_eq!(spec.series.len(), 1);
        let bp = &spec.series[0].box_points;
        assert_eq!(bp.len(), 2);
        assert_eq!(bp[0].min, 10.0);
        assert_eq!(bp[0].q1, 25.0);
        assert_eq!(bp[0].median, 50.0);
        assert_eq!(bp[0].q3, 75.0);
        assert_eq!(bp[0].max, 90.0);
        assert_eq!(bp[1].min, 5.0);
        assert_eq!(bp[1].median, 45.0);
    }

    #[test]
    fn parse_boxplot_rejects_flat_nums() {
        let json = r#"{
            "type": "boxplot",
            "data": {
                "labels": ["A"],
                "datasets": [{"data": [10, 25, 50, 75, 90]}]
            }
        }"#;
        assert!(
            parse(json, false).is_err(),
            "boxplot with flat numbers should fail"
        );
    }

    #[test]
    fn auto_fill_gets_half_alpha() {
        // backgroundColor 未指定のバーチャートで fill が alpha=0.5 になること。
        let json = r#"{
            "type": "bar",
            "data": {
                "labels": ["A", "B"],
                "datasets": [{"label": "S1", "data": [1, 2]}]
            }
        }"#;
        let spec = parse(json, false).expect("parse error");
        let fill_alpha = spec.series[0].fill[0].a;
        assert!(
            (fill_alpha - 0.5).abs() < 1e-6,
            "fill alpha は 0.5 であるべき、実際は {}",
            fill_alpha
        );
    }

    #[test]
    fn auto_stroke_gets_full_alpha() {
        // borderColor 未指定のバーチャートで stroke が alpha=1.0 になること。
        let json = r#"{
            "type": "bar",
            "data": {
                "labels": ["A", "B"],
                "datasets": [{"label": "S1", "data": [1, 2]}]
            }
        }"#;
        let spec = parse(json, false).expect("parse error");
        let stroke_alpha = spec.series[0].stroke[0].a;
        assert!(
            (stroke_alpha - 1.0).abs() < 1e-6,
            "stroke alpha は 1.0 であるべき、実際は {}",
            stroke_alpha
        );
    }

    #[test]
    fn pie_auto_fill_gets_full_alpha() {
        // pie チャートは chart.js v4 の colorizeDoughnutDataset が BORDER_COLORS を使うため alpha=1.0。
        let json =
            r#"{"type": "pie", "data": {"labels": ["A", "B"], "datasets": [{"data": [1, 2]}]}}"#;
        let spec = parse(json, false).expect("parse error");
        let fill_alpha = spec.series[0].fill[0].a;
        assert!(
            (fill_alpha - 1.0).abs() < 1e-6,
            "pie の fill alpha は 1.0 であるべき、実際は {}",
            fill_alpha
        );
    }

    #[test]
    fn pie_cutout_accepts_pixels_and_percentage() {
        let pixels = parse(
            r#"{"type":"doughnut","data":{"datasets":[{"data":[1,1]}]},"options":{"cutout":40}}"#,
            false,
        )
        .unwrap();
        let percent = parse(
            r#"{"type":"pie","data":{"datasets":[{"data":[1,1]}]},"options":{"cutout":"25%"}}"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            pixels.kind,
            ChartKind::Pie {
                cutout: PieCutout::Pixels(40.0),
                ..
            }
        ));
        assert!(matches!(
            percent.kind,
            ChartKind::Pie {
                cutout: PieCutout::Percent(25.0),
                ..
            }
        ));
    }

    #[test]
    fn pie_cutout_rejects_invalid_percent_strings() {
        let error = parse(
            r#"{"type":"pie","data":{"datasets":[{"data":[1,1]}]},"options":{"cutout":"half"}}"#,
            false,
        )
        .unwrap_err();
        assert!(
            error.contains("options.cutout"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn pie_cutout_and_offsets_reject_wrong_json_types() {
        for json in [
            r#"{"type":"pie","data":{"datasets":[{"data":[1]}]},"options":{"cutout":true}}"#,
            r#"{"type":"pie","data":{"datasets":[{"data":[1],"offset":"7"}]}}"#,
        ] {
            assert!(parse(json, false).is_err(), "should reject {json}");
        }
    }

    #[test]
    fn pie_dataset_arc_options_parse_in_strict_mode() {
        let spec = parse(
            r#"{"type":"pie","data":{"datasets":[
              {"data":[1,2,3],"spacing":2,"offset":5,"borderRadius":4},
              {"data":[3,2,1],"spacing":3,"offset":[1,2],"borderRadius":[1,{"outerStart":2,"outerEnd":3,"innerStart":4,"innerEnd":5}]},
              {"data":[1]}
            ]}}"#,
            true,
        )
        .unwrap();
        assert!(matches!(&spec.kind, ChartKind::Pie { .. }));
        if let ChartKind::Pie {
            dataset_options, ..
        } = spec.kind
        {
            assert_eq!(dataset_options.len(), 3);
            assert_eq!(dataset_options[0].spacing, 2.0);
            assert_eq!(dataset_options[0].offset_at(2), 5.0);
            assert!(matches!(
                dataset_options[0].border_radius_at(0),
                ArcBorderRadius::Uniform(4.0)
            ));
            assert_eq!(dataset_options[1].spacing, 3.0);
            assert_eq!(dataset_options[1].offset_at(0), 1.0);
            assert_eq!(dataset_options[1].offset_at(1), 2.0);
            assert_eq!(dataset_options[1].offset_at(2), 1.0);
            assert!(matches!(
                dataset_options[1].border_radius_at(0),
                ArcBorderRadius::Uniform(1.0)
            ));
            assert_eq!(
                dataset_options[1].border_radius_at(1),
                ArcBorderRadius::Corners {
                    outer_start: 2.0,
                    outer_end: 3.0,
                    inner_start: 4.0,
                    inner_end: 5.0,
                }
            );
            assert_eq!(dataset_options[2].spacing, 0.0);
            assert_eq!(dataset_options[2].offset_at(1), 0.0);
            assert_eq!(
                dataset_options[2].border_radius_at(1),
                ArcBorderRadius::Uniform(0.0)
            );
        }
    }

    #[test]
    fn pie_without_dataset_arc_options_keeps_implicit_defaults() {
        let spec = parse(
            r#"{"type":"pie","data":{"datasets":[{"data":[1,2,3]}]}}"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            spec.kind,
            ChartKind::Pie {
                dataset_options,
                ..
            } if dataset_options.is_empty()
        ));
    }

    #[test]
    fn non_pie_arc_options_are_not_validated() {
        let spec = parse(
            r#"{"type":"line","data":{"labels":["A"],"datasets":[{"data":[1],"spacing":{"bad":1},"offset":{"bad":1},"borderRadius":{"outerStart":"bad"}}]},"options":{"cutout":{"bad":1}}}"#,
            false,
        );
        assert!(spec.is_ok(), "non-pie options should be ignored: {spec:?}");
    }

    #[test]
    fn pie_dataset_arc_options_report_dataset_path_on_invalid_values() {
        let json = r#"{"type":"pie","data":{"datasets":[{"data":[1],"borderRadius":{"outerStart":"bad"}}]}}"#;
        let error = parse(json, false).unwrap_err();
        assert!(
            error.contains("data.datasets[0]"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn bubble_no_border_color_stroke_is_global_default() {
        // chart.js v4: backgroundColor 指定・borderColor 未指定の bubble は
        // Colors プラグインをスキップし、グローバルデフォルト rgba(0,0,0,0.1) になる。
        let json = r##"{
            "type": "bubble",
            "data": {
                "datasets": [{"backgroundColor": "#9966ff", "data": [{"x":1,"y":2,"r":5}]}]
            }
        }"##;
        let spec = parse(json, false).expect("parse error");
        let stroke = spec.series[0].stroke[0];
        assert_eq!(stroke.r, 0, "stroke.r must be 0 (black)");
        assert_eq!(stroke.g, 0, "stroke.g must be 0 (black)");
        assert_eq!(stroke.b, 0, "stroke.b must be 0 (black)");
        assert!(
            (stroke.a - 0.1).abs() < 1e-6,
            "stroke alpha must be 0.1 (global default), got {}",
            stroke.a
        );
    }

    #[test]
    fn scatter_no_border_color_stroke_is_global_default() {
        // chart.js v4: backgroundColor 指定・borderColor 未指定の scatter も同様。
        let json = r##"{
            "type": "scatter",
            "data": {
                "datasets": [{"backgroundColor": "#36a2eb", "data": [{"x":1,"y":2}]}]
            }
        }"##;
        let spec = parse(json, false).expect("parse error");
        let stroke = spec.series[0].stroke[0];
        assert_eq!(stroke.r, 0, "stroke.r must be 0 (black)");
        assert_eq!(stroke.g, 0, "stroke.g must be 0 (black)");
        assert_eq!(stroke.b, 0, "stroke.b must be 0 (black)");
        assert!(
            (stroke.a - 0.1).abs() < 1e-6,
            "stroke alpha must be 0.1 (global default), got {}",
            stroke.a
        );
    }

    #[test]
    fn scatter_no_colors_stroke_derives_from_auto_fill() {
        // backgroundColor も borderColor も未指定の scatter では
        // stroke が fill と同 RGB (= palette色)、alpha=1.0 になる。
        let json = r#"{
            "type": "scatter",
            "data": {
                "datasets": [{"data": [{"x":1,"y":2}]}]
            }
        }"#;
        let spec = parse(json, false).expect("parse error");
        let fill = spec.series[0].fill[0];
        let stroke = spec.series[0].stroke[0];
        // stroke RGB は fill (パレット由来) と一致する
        assert_eq!(
            stroke.r, fill.r,
            "stroke.r must match fill.r (palette color)"
        );
        assert_eq!(
            stroke.g, fill.g,
            "stroke.g must match fill.g (palette color)"
        );
        assert_eq!(
            stroke.b, fill.b,
            "stroke.b must match fill.b (palette color)"
        );
        // stroke alpha は 1.0
        assert!(
            (stroke.a - 1.0).abs() < 1e-6,
            "stroke alpha must be 1.0, got {}",
            stroke.a
        );
        // fill alpha は 0.5 (scatter は半透明)
        assert!(
            (fill.a - 0.5).abs() < 1e-6,
            "fill alpha must be 0.5, got {}",
            fill.a
        );
    }

    #[test]
    fn scatter_explicit_border_color_is_respected() {
        // borderColor を明示した場合はその色が使われる。
        let json = r##"{
            "type": "scatter",
            "data": {
                "datasets": [{"backgroundColor": "#ff0000", "borderColor": "#0000ff", "data": [{"x":1,"y":2}]}]
            }
        }"##;
        let spec = parse(json, false).expect("parse error");
        let stroke = spec.series[0].stroke[0];
        assert_eq!(stroke.r, 0);
        assert_eq!(stroke.g, 0);
        assert_eq!(stroke.b, 255);
    }

    #[test]
    fn sparkline_parses_to_sparkline_kind() {
        let json = r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3]}]}}"#;
        let spec = parse(json, false).unwrap();
        assert!(matches!(spec.kind, crate::ir::ChartKind::Sparkline));
    }

    #[test]
    fn tension_is_normalized_to_chartjs_range() {
        let spec = parse(
            r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3],"tension":1e308}]}}"#,
            false,
        )
        .unwrap();
        assert_eq!(
            spec.series[0].interpolation,
            LineInterpolation::CatmullRom { tension: 1.0 }
        );

        let spec = parse(
            r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3],"tension":-2}]}}"#,
            false,
        )
        .unwrap();
        assert_eq!(spec.series[0].interpolation, LineInterpolation::Linear);
    }

    #[test]
    fn cubic_interpolation_mode_monotone_overrides_tension() {
        let spec = parse(
            r#"{"type":"line","data":{"datasets":[{"data":[1,2,3],"tension":0.8,"cubicInterpolationMode":"monotone"},{"data":[1,2,3],"tension":0.4,"cubicInterpolationMode":"default"}]}}"#,
            false,
        )
        .unwrap();

        assert_eq!(spec.series[0].interpolation, LineInterpolation::Monotone);
        assert_eq!(
            spec.series[1].interpolation,
            LineInterpolation::CatmullRom { tension: 0.4 }
        );
    }

    #[test]
    fn cubic_interpolation_mode_rejects_unknown_values() {
        let error = parse(
            r#"{"type":"line","data":{"datasets":[{"data":[1,2,3],"cubicInterpolationMode":"smooth"}]}}"#,
            false,
        )
        .unwrap_err();

        assert!(error.contains("cubicInterpolationMode"));
    }

    #[test]
    fn cubic_interpolation_mode_rejects_explicit_null() {
        let json = r#"{"type":"line","data":{"datasets":[{"data":[1,2,3],"cubicInterpolationMode":null}]}}"#;
        for strict in [false, true] {
            let error = parse(json, strict).unwrap_err();
            assert!(error.contains("cubicInterpolationMode"));
        }
    }

    #[test]
    fn mixed_line_dataset_accepts_monotone_cubic_mode() {
        let spec = parse(
            r#"{"type":"line","data":{"datasets":[{"data":[0,2,7],"tension":0.8,"cubicInterpolationMode":"monotone"},{"type":"bar","data":[1,2,3]}]}}"#,
            false,
        )
        .unwrap();

        assert!(matches!(spec.kind, ChartKind::Mixed));
        assert_eq!(spec.series[0].interpolation, LineInterpolation::Monotone);
    }

    #[test]
    fn strict_bar_parser_accepts_cubic_mode_for_line_overrides() {
        parse(
            r#"{"type":"bar","data":{"datasets":[{"type":"line","data":[0,2,7],"tension":0.8,"cubicInterpolationMode":"monotone"},{"data":[1,2,3]}]}}"#,
            true,
        )
        .unwrap();
    }

    #[test]
    fn strict_bar_parser_rejects_unknown_cubic_mode_even_for_bar_series() {
        let error = parse(
            r#"{"type":"bar","data":{"datasets":[{"data":[1,2,3],"cubicInterpolationMode":"smooth"}]}}"#,
            true,
        )
        .unwrap_err();

        assert!(error.contains("cubicInterpolationMode"));
    }

    #[test]
    fn cubic_interpolation_mode_rejects_explicit_null_for_bar_series() {
        let json = r#"{"type":"bar","data":{"datasets":[{"data":[1,2,3],"cubicInterpolationMode":null}]}}"#;
        for strict in [false, true] {
            let error = parse(json, strict).unwrap_err();
            assert!(error.contains("cubicInterpolationMode"));
        }
    }

    #[test]
    fn strict_line_parser_accepts_cubic_interpolation_mode() {
        parse(
            r#"{"type":"line","data":{"datasets":[{"data":[1,2,3],"cubicInterpolationMode":"monotone"}]}}"#,
            true,
        )
        .unwrap();
    }

    #[test]
    fn normalize_tension_edge_cases() {
        assert_eq!(normalize_tension(0.5), 0.5);
        assert_eq!(normalize_tension(0.0), 0.0);
        assert_eq!(normalize_tension(-0.5), 0.0);
        assert_eq!(normalize_tension(1.5), 1.0);
        assert_eq!(normalize_tension(f64::NAN), 0.0);
        assert_eq!(normalize_tension(f64::INFINITY), 0.0);
        assert_eq!(normalize_tension(f64::NEG_INFINITY), 0.0);
    }

    #[test]
    fn parse_outlabeled_pie_kind() {
        let json = r#"{"type":"outlabeledPie","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#;
        let spec = parse(json, false).expect("parse error");
        assert!(matches!(
            spec.kind,
            crate::ir::ChartKind::OutlabeledPie { donut_ratio, .. } if (donut_ratio - 0.0).abs() < 1e-9
        ));
    }

    #[test]
    fn parse_outlabeled_doughnut_kind() {
        let json = r#"{"type":"outlabeledDoughnut","data":{"labels":["A","B"],"datasets":[{"data":[40,60]}]}}"#;
        let spec = parse(json, false).expect("parse error");
        assert!(matches!(
            spec.kind,
            crate::ir::ChartKind::OutlabeledPie { donut_ratio, .. } if (donut_ratio - 0.5).abs() < 1e-9
        ));
    }

    #[test]
    fn parse_outlabeled_pie_outlabels_plugin() {
        let json = r#"{
            "type": "outlabeledPie",
            "data": {"labels": ["X"], "datasets": [{"data": [100]}]},
            "options": {"plugins": {"outlabels": {"stretch": 60.0, "color": "black"}}}
        }"#;
        let spec = parse(json, false).expect("parse error");
        if let crate::ir::ChartKind::OutlabeledPie { outlabel, .. } = &spec.kind {
            assert!((outlabel.stretch - 60.0).abs() < 1e-9, "stretch mismatch");
            assert_eq!(outlabel.color.r, 0, "color should be black");
        } else {
            panic!("wrong kind");
        }
    }

    #[test]
    fn outlabeled_pie_fill_alpha_is_one() {
        // outlabeledPie も pie 同様に fill alpha = 1.0 であるべき。
        let json =
            r#"{"type":"outlabeledPie","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = parse(json, false).expect("parse error");
        assert!(
            (spec.series[0].fill[0].a - 1.0).abs() < 1e-6,
            "fill alpha must be 1.0"
        );
    }

    #[test]
    fn parse_outlabeled_pie_strict_with_outlabels_plugin() {
        // strict モードで outlabels プラグインが正しく受け付けられること。
        let json = r#"{
            "type": "outlabeledPie",
            "data": {"labels": ["A", "B"], "datasets": [{"data": [60, 40]}]},
            "options": {"plugins": {"outlabels": {"stretch": 50.0, "text": "%l: %p%"}}}
        }"#;
        let result = parse(json, true);
        assert!(
            result.is_ok(),
            "strict mode should accept outlabels plugin: {:?}",
            result
        );
    }

    #[test]
    fn strict_allows_border_on_scales() {
        // schema は options.scales.<axis>.border を typed で受理するので、
        // strict モードでも allow-list に "border" が含まれていなければならない。
        // schema/strict 契約の分岐(schema 通過→strict 拒否)を防ぐ回帰テスト。
        let json = r#"{
            "type": "bar",
            "data": {"labels":["a"],"datasets":[{"data":[1]}]},
            "options": {"scales":{"x":{"border":{"width":2}}}}
        }"#;
        let result = parse(json, true);
        assert!(
            result.is_ok(),
            "strict モードで options.scales.x.border を許可すべき: {:?}",
            result
        );
    }

    #[test]
    fn parse_treemap_numeric_tree() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{ "tree": [6, 4, 3, 2, 1] }] }
        }"#;
        let spec = parse(json, false).expect("parse error");
        assert!(matches!(spec.kind, crate::ir::ChartKind::Treemap));
        assert_eq!(spec.series.len(), 1);
        let t = &spec.series[0].tree;
        assert_eq!(t.len(), 5);
        assert_eq!(t[0].value, 6.0);
        assert!(t[0].children.is_empty());
    }

    #[test]
    fn parse_treemap_grouped_sums_and_preserves_order() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{
                "key": "value",
                "groups": ["cat", "sub"],
                "tree": [
                    {"cat": "B", "sub": "x", "value": 2},
                    {"cat": "A", "sub": "p", "value": 5},
                    {"cat": "A", "sub": "p", "value": 1},
                    {"cat": "A", "sub": "q", "value": 4},
                    {"cat": "B", "sub": "x", "value": 3}
                ]
            }] }
        }"#;
        let spec = parse(json, false).expect("parse error");
        let t = &spec.series[0].tree;
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].label, "B");
        assert_eq!(t[1].label, "A");
        assert_eq!(t[0].value, 5.0);
        assert_eq!(t[0].children.len(), 1);
        assert_eq!(t[0].children[0].label, "x");
        assert_eq!(t[0].children[0].value, 5.0);
        assert!(t[0].children[0].children.is_empty());
        assert_eq!(t[1].value, 10.0);
        assert_eq!(t[1].children.len(), 2);
        assert_eq!(t[1].children[0].label, "p");
        assert_eq!(t[1].children[0].value, 6.0);
        assert_eq!(t[1].children[1].label, "q");
        assert_eq!(t[1].children[1].value, 4.0);
    }

    #[test]
    fn treemap_rejects_excessive_group_depth() {
        let groups: String = (0..60)
            .map(|i| format!("\"g{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"type":"treemap","data":{{"datasets":[{{"key":"v","groups":[{groups}],"tree":[{{"v":1}}]}}]}}}}"#
        );
        assert!(parse(&json, false).is_err());
    }

    #[test]
    fn treemap_strict_rejects_unknown_dataset_key() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{ "tree": [1,2], "bogus": true }] }
        }"#;
        assert!(parse(json, true).is_err());
    }

    #[test]
    fn treemap_strict_accepts_known_keys() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{ "key": "v", "groups": ["g"],
                "tree": [{"g": "a", "v": 1}] }] }
        }"#;
        assert!(parse(json, true).is_ok());
    }

    #[test]
    fn treemap_rejects_too_many_nodes() {
        let nums: String = (0..10_001).map(|_| "1").collect::<Vec<_>>().join(",");
        let json = format!(r#"{{"type":"treemap","data":{{"datasets":[{{"tree":[{nums}]}}]}}}}"#);
        assert!(parse(&json, false).is_err());
    }

    #[test]
    fn parse_line_null_becomes_nan_in_series() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3]}]}}"#;
        let spec = parse(json, false).unwrap();
        assert_eq!(spec.series[0].values.len(), 3);
        assert_eq!(spec.series[0].values[0], 1.0);
        assert!(spec.series[0].values[1].is_nan());
        assert_eq!(spec.series[0].values[2], 3.0);
    }

    #[test]
    fn parse_line_maps_span_gaps_and_every_stepped_value() {
        let cases = [
            (true, "false", None),
            (false, "true", Some(crate::ir::StepMode::Before)),
            (false, r#""before""#, Some(crate::ir::StepMode::Before)),
            (false, r#""after""#, Some(crate::ir::StepMode::After)),
            (false, r#""middle""#, Some(crate::ir::StepMode::Middle)),
        ];

        for (span_gaps, stepped, step_mode) in cases {
            let json = format!(
                r#"{{"type":"line","data":{{"datasets":[{{"data":[1,null,3],"spanGaps":{span_gaps},"stepped":{stepped}}}]}}}}"#
            );
            let spec = parse(&json, false).unwrap();
            assert_eq!(spec.series[0].span_gaps, span_gaps);
            assert_eq!(spec.series[0].step_mode, step_mode);
        }

        let default = parse(
            r#"{"type":"line","data":{"datasets":[{"data":[1,null,3]}]}}"#,
            false,
        )
        .unwrap();
        assert!(!default.series[0].span_gaps);
    }

    #[test]
    fn strict_line_parser_and_public_schema_match_for_span_gaps_and_stepped() {
        for (key, value) in [
            ("spanGaps", "true"),
            ("spanGaps", "false"),
            ("spanGaps", "null"),
            ("stepped", "false"),
            ("stepped", "true"),
            ("stepped", r#""before""#),
            ("stepped", r#""after""#),
            ("stepped", r#""middle""#),
            ("stepped", "null"),
        ] {
            let json = format!(
                r#"{{"type":"line","data":{{"datasets":[{{"data":[1,2],"{key}":{value}}}]}}}}"#
            );
            assert!(
                serde_json::from_str::<crate::schema::chartjs::ChartJsSpec>(&json).is_ok(),
                "public schema rejected {key}: {value}"
            );
            assert!(
                parse(&json, true).is_ok(),
                "strict parser rejected {key}: {value}"
            );
        }

        let invalid = r#"{"type":"line","data":{"datasets":[{"data":[1,2],"stepped":"left"}]}}"#;
        assert!(serde_json::from_str::<crate::schema::chartjs::ChartJsSpec>(invalid).is_err());
        assert!(parse(invalid, false).is_err());
        assert!(parse(invalid, true).is_err());
    }

    #[test]
    fn bar_dataset_geometry_options_parse_per_dataset_in_strict_mode() {
        let json = r#"{
            "type":"bar",
            "data":{"labels":["A"],"datasets":[{
                "data":[1],"categoryPercentage":0.6,"barPercentage":0.5,
                "barThickness":"flex","maxBarThickness":18,"minBarLength":3
            }]}
        }"#;
        assert!(
            serde_json::from_str::<crate::schema::chartjs::ChartJsSpec>(json).is_ok(),
            "the public schema should expose bar geometry on datasets"
        );
        let spec = parse(json, true).expect("strict parsing should accept bar geometry options");
        let geometry = spec.series[0]
            .bar_geometry
            .expect("bar geometry should be present");
        assert_eq!(geometry.category_percentage, Some(0.6));
        assert_eq!(geometry.bar_percentage, Some(0.5));
        assert_eq!(geometry.bar_thickness, Some(BarThickness::Flex));
        assert_eq!(geometry.max_bar_thickness, Some(18.0));
        assert_eq!(geometry.min_bar_length, Some(3.0));

        let mixed_line_root = r#"{
            "type":"line",
            "data":{"labels":["A"],"datasets":[
                {"type":"bar","data":[1],"barThickness":12},
                {"data":[2]}
            ]}
        }"#;
        assert!(
            serde_json::from_str::<crate::schema::chartjs::ChartJsSpec>(mixed_line_root).is_ok()
        );
        let mixed_spec = parse(mixed_line_root, true).expect("mixed chart should parse");
        let geometry = mixed_spec.series[0]
            .bar_geometry
            .expect("bar geometry should be present");
        assert_eq!(geometry.bar_thickness, Some(BarThickness::Pixels(12.0)));
        assert_eq!(geometry.category_percentage, None);
        assert_eq!(geometry.bar_percentage, None);
        assert_eq!(geometry.max_bar_thickness, None);
        assert_eq!(geometry.min_bar_length, None);

        let no_geometry = r#"{
            "type":"bar",
            "data":{"datasets":[{"data":[1]}]}
        }"#;
        let no_geometry_spec = parse(no_geometry, true).expect("bar chart should parse");
        assert!(no_geometry_spec.series[0].bar_geometry.is_none());
    }

    #[test]
    fn bar_dataset_border_radius_parses_per_dataset_in_strict_mode() {
        let cases = [
            r#"{"type":"bar","data":{"datasets":[{"data":[1],"borderRadius":6}]}}"#,
            r#"{"type":"line","data":{"datasets":[{"type":"bar","data":[1],"borderRadius":{"topLeft":4}}]}}"#,
        ];
        for json in cases {
            let spec = parse(json, false).unwrap();
            assert!(spec.series[0].bar_geometry.is_some(), "{json}");
            assert!(parse(json, true).is_ok(), "strict parse rejected {json}");
        }
        assert!(
            parse(
                r#"{"type":"bar","data":{"datasets":[{"data":[1],"borderRaduis":6}]}}"#,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn non_bar_border_radius_is_not_validated_as_a_bar_option() {
        let doughnut = r#"{"type":"doughnut","data":{"datasets":[{"data":[1,2],"borderRadius":{"outerStart":4}}]}}"#;
        assert!(
            parse(doughnut, false).is_ok(),
            "non-bar borderRadius must retain the chart's existing permissive parsing"
        );

        let invalid_bar =
            r#"{"type":"bar","data":{"datasets":[{"data":[1],"borderRadius":{"topCentre":4}}]}}"#;
        assert!(
            parse(invalid_bar, false).is_err(),
            "bar borderRadius must reject unsupported corner keys"
        );
    }

    #[test]
    fn bar_roots_ignore_line_only_options_but_strict_rejects_them() {
        for option in [r#""stepped":"left""#, r#""spanGaps":1"#] {
            let cases = [
                (
                    "bar",
                    r#"{"type":"bar","data":{"datasets":[{"data":[3,4]}]}}"#.to_string(),
                    format!(
                        r#"{{"type":"bar","data":{{"datasets":[{{"data":[3,4],{option}}}]}}}}"#
                    ),
                ),
                (
                    "mixed",
                    r#"{"type":"bar","data":{"datasets":[{"data":[3,4]},{"type":"line","data":[1,2]}]}}"#.to_string(),
                    format!(
                        r#"{{"type":"bar","data":{{"datasets":[{{"data":[3,4]}},{{"type":"line","data":[1,2],{option}}}]}}}}"#
                    ),
                ),
            ];

            for (name, absent, with_line_option) in cases {
                assert_eq!(
                    parse(&with_line_option, false).unwrap(),
                    parse(&absent, false).unwrap(),
                    "non-strict {name} root changed for {option}"
                );
                assert!(
                    parse(&with_line_option, true).is_err(),
                    "strict {name} root accepted {option}"
                );
            }
        }
    }

    #[test]
    fn bar_roots_ignore_line_style_options_on_bar_datasets_but_strict_rejects_them() {
        let plain = r#"{"type":"bar","data":{"datasets":[{"data":[3,4]}]}}"#;
        for (name, option) in [
            ("pointStyle", r#""pointStyle":"triangle""#),
            ("showLine", r#""showLine":false"#),
            ("borderDash", r#""borderDash":[4,2]"#),
            ("borderDashOffset", r#""borderDashOffset":2"#),
        ] {
            let with_option =
                plain.replace(r#""data":[3,4]"#, &format!(r#""data":[3,4],{option}"#));
            let parsed_with_option = parse(&with_option, false)
                .unwrap_or_else(|error| panic!("{name}: {error} in {with_option}"));
            assert_eq!(
                parsed_with_option,
                parse(plain, false).unwrap(),
                "non-strict bar root must ignore {name} on a bar dataset"
            );
            assert!(
                parse(&with_option, true).is_err(),
                "strict bar root must reject {name} on a bar dataset"
            );
        }
    }

    #[test]
    fn mixed_line_root_rejects_line_only_options() {
        for option in [r#""spanGaps":true"#, r#""stepped":"middle""#] {
            let json = format!(
                r#"{{"type":"line","data":{{"datasets":[{{"data":[1,null,3],{option}}},{{"type":"bar","data":[2,3,4]}}]}}}}"#
            );
            for strict in [false, true] {
                let err = parse(&json, strict).expect_err("mixed line options must be rejected");
                assert!(err.contains("only supported"), "unexpected error: {err}");
            }
        }
    }

    #[test]
    fn line_root_all_bar_override_rejects_line_only_options() {
        for option in [r#""spanGaps":true"#, r#""stepped":"middle""#] {
            let json = format!(
                r#"{{"type":"line","data":{{"datasets":[{{"type":"bar","data":[1,2],{option}}},{{"type":"bar","data":[3,4]}}]}}}}"#
            );

            for strict in [false, true] {
                let error = parse(&json, strict)
                    .expect_err("line-only options must be rejected for all-bar overrides");
                assert!(error.contains("only supported"), "{error}");
            }
        }
    }

    #[test]
    fn parse_bar_null_becomes_nan_in_series() {
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
            "datasets":[{"data":[null, 5]}]}}"#;
        let spec = parse(json, false).unwrap();
        assert!(spec.series[0].values[0].is_nan());
        assert_eq!(spec.series[0].values[1], 5.0);
    }

    #[test]
    fn parse_boxplot_null_row_becomes_nan_box_point() {
        let json = r#"{"type":"boxplot","data":{"labels":["a","b"],
            "datasets":[{"data":[[1,2,3,4,5], null]}]}}"#;
        let spec = parse(json, false).unwrap();
        let bp = spec.series[0].box_points[1];
        assert!(bp.min.is_nan());
        assert!(bp.max.is_nan());
        assert!(bp.median.is_nan());
    }

    #[test]
    fn parse_boxplot_all_null_data_accepted() {
        // untagged enum は Boxes より先に Nums に match するため、行が全て null の
        // boxplot(スキーマ有効入力)は Nums(Vec<None>) として届く。全 NaN 行の box 列
        // として受理されるべき(空チャート化させない)。
        let json = r#"{"type":"boxplot","data":{"labels":["a","b"],
            "datasets":[{"data":[null, null]}]}}"#;
        let spec = parse(json, false).unwrap();
        assert_eq!(spec.series[0].box_points.len(), 2);
        for bp in &spec.series[0].box_points {
            assert!(bp.min.is_nan() && bp.max.is_nan());
        }
    }

    #[test]
    fn parse_boxplot_single_null_data_accepted() {
        let json = r#"{"type":"boxplot","data":{"labels":["a"],
            "datasets":[{"data":[null]}]}}"#;
        let spec = parse(json, false).unwrap();
        assert_eq!(spec.series[0].box_points.len(), 1);
        assert!(spec.series[0].box_points[0].min.is_nan());
    }

    #[test]
    fn parse_sparkline_rejects_null() {
        // sparkline は layout 側で NaN 欠損を扱えないため、data 内の null は parse 段階で拒否。
        let json = r#"{"type":"sparkline","data":{"datasets":[{"data":[1, null, 3]}]}}"#;
        let err = parse(json, false).expect_err("sparkline should reject null");
        assert!(
            err.contains("sparkline"),
            "error should mention sparkline: {err}"
        );
    }

    #[test]
    fn parse_pie_rejects_null() {
        // pie は layout 側で NaN スライスを扱えない(0 頂点・不正な弧になる)ため
        // data 内の null は parse 段階で拒否。
        let json = r#"{"type":"pie","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3]}]}}"#;
        let err = parse(json, false).expect_err("pie should reject null");
        assert!(err.contains("pie"), "error should mention pie: {err}");
    }

    #[test]
    fn parse_radar_rejects_null() {
        // radar は layout 側で NaN 頂点を 0 に丸めるため、data 内の null は parse
        // 段階で拒否する(silent に 0 頂点で描画されないように)。
        let json = r#"{"type":"radar","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3]}]}}"#;
        let err = parse(json, false).expect_err("radar should reject null");
        assert!(err.contains("radar"), "error should mention radar: {err}");
    }

    #[test]
    fn parse_polar_area_rejects_null() {
        // polarArea も radar と同じく NaN を安全に扱えないため parse 段階で拒否。
        let json = r#"{"type":"polarArea","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3]}]}}"#;
        let err = parse(json, false).expect_err("polarArea should reject null");
        assert!(
            err.contains("polarArea"),
            "error should mention polarArea: {err}"
        );
    }

    #[test]
    fn treemap_grouped_overflow_value_is_finite() {
        // 同一バケットの大きな有限値が +Inf に overflow しても、集約値は有限に
        // クランプされる(layout の空描画を防ぐ)。
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{
                "key": "v", "groups": ["a"],
                "tree": [
                    {"a":"X","v":1e308},
                    {"a":"X","v":1e308},
                    {"a":"X","v":1e308}
                ]
            }] }
        }"#;
        let spec = parse(json, false).expect("parse error");
        let t = &spec.series[0].tree;
        assert_eq!(t.len(), 1);
        assert!(
            t[0].value.is_finite(),
            "grouped overflow must clamp to finite, got {}",
            t[0].value
        );
        assert!(t[0].value > 0.0);
    }

    // ----- Task 6: Schema→IR ヘルパ (axis_title_from / axis_grid_from / axis_border_from) -----

    #[test]
    fn axis_title_from_returns_none_when_display_false() {
        let opts = AxisTitleOptions {
            display: Some(false),
            text: Some("Y".into()),
            ..Default::default()
        };
        assert!(axis_title_from(Some(&opts)).is_none());
    }

    #[test]
    fn axis_title_from_returns_none_when_text_missing_or_empty() {
        let opts_no_text = AxisTitleOptions {
            display: Some(true),
            text: None,
            ..Default::default()
        };
        assert!(axis_title_from(Some(&opts_no_text)).is_none());
        let opts_empty = AxisTitleOptions {
            display: Some(true),
            text: Some(String::new()),
            ..Default::default()
        };
        assert!(axis_title_from(Some(&opts_empty)).is_none());
    }

    #[test]
    fn axis_title_from_maps_text_and_align() {
        let opts = AxisTitleOptions {
            display: Some(true),
            text: Some("Y (円)".into()),
            align: Some(SchemaAxisTitleAlign::End),
            ..Default::default()
        };
        let t = axis_title_from(Some(&opts)).expect("title");
        assert_eq!(t.text, "Y (円)");
        assert_eq!(t.align, crate::ir::AxisTitleAlign::End);
        assert!(t.color.is_none());
        assert!(t.font_size.is_none());
    }

    #[test]
    fn axis_title_from_maps_color_and_font_size() {
        use crate::schema::common::FontSpec;
        let opts = AxisTitleOptions {
            display: Some(true),
            text: Some("A".into()),
            color: Some("#123456".into()),
            font: Some(FontSpec {
                size: Some(14.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let t = axis_title_from(Some(&opts)).expect("title");
        assert!(t.color.is_some());
        assert_eq!(t.font_size, Some(14.0));
    }

    #[test]
    fn axis_grid_from_defaults_when_none() {
        let g = axis_grid_from(None);
        assert!(g.display);
        assert!((g.line_width - 1.0).abs() < 1e-9);
        // fulgur の意図的乖離: draw_ticks 既定は false(Chart.js は true)。
        // 詳細は `AxisGrid::default` のドキュメント参照。
        assert!(!g.draw_ticks);
        assert!(g.color.is_none());
    }

    #[test]
    fn axis_grid_from_display_false_kills_grid() {
        let opts = GridLineOptions {
            display: Some(false),
            ..Default::default()
        };
        assert!(!axis_grid_from(Some(&opts)).display);
    }

    #[test]
    fn axis_grid_from_draw_on_chart_area_false_kills_grid_in_v1() {
        let opts = GridLineOptions {
            display: Some(true),
            draw_on_chart_area: Some(false),
            ..Default::default()
        };
        assert!(
            !axis_grid_from(Some(&opts)).display,
            "v1: drawOnChartArea=false は display=false と同義"
        );
    }

    #[test]
    fn axis_grid_from_scalar_line_width() {
        use crate::schema::common::ScalarOrArray;
        let opts = GridLineOptions {
            line_width: Some(ScalarOrArray::One(2.5)),
            ..Default::default()
        };
        assert!((axis_grid_from(Some(&opts)).line_width - 2.5).abs() < 1e-9);
    }

    #[test]
    fn axis_grid_from_array_line_width_uses_first() {
        use crate::schema::common::ScalarOrArray;
        let opts = GridLineOptions {
            line_width: Some(ScalarOrArray::Many(vec![3.0, 5.0])),
            ..Default::default()
        };
        assert!((axis_grid_from(Some(&opts)).line_width - 3.0).abs() < 1e-9);
    }

    #[test]
    fn axis_border_from_defaults_when_none() {
        let b = axis_border_from(None);
        assert!(b.display);
        assert!((b.width - 1.0).abs() < 1e-9);
        assert!(b.color.is_none());
        assert!(b.dash.is_empty());
    }

    #[test]
    fn axis_border_from_dash_and_width_flow_through() {
        let opts = AxisBorderOptions {
            dash: Some(vec![4.0, 4.0]),
            width: Some(2.0),
            ..Default::default()
        };
        let b = axis_border_from(Some(&opts));
        assert_eq!(b.dash, vec![4.0, 4.0]);
        assert!((b.width - 2.0).abs() < 1e-9);
    }

    #[test]
    fn axis_border_from_display_false() {
        let opts = AxisBorderOptions {
            display: Some(false),
            ..Default::default()
        };
        assert!(!axis_border_from(Some(&opts)).display);
    }

    #[test]
    fn scales_x_title_flows_into_spec_x_axis() {
        // 統合: options.scales.x.title.text が ChartSpec.x_axis.title へ流れることを検証。
        // Task 6 の unit テストは helper 単体だけ、Task 7 では bridge 側の配線を確認する。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"x":{"title":{"display":true,"text":"時刻"}}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        let t = spec.x_axis.title.as_ref().expect("x title should be Some");
        assert_eq!(t.text, "時刻");
    }

    #[test]
    fn scales_y_border_dash_flows_into_spec_y_axis() {
        // 統合: options.scales.y.border.{dash,width} が ChartSpec.y_axis.border に反映される。
        let json = r##"{
          "type":"line",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"y":{"border":{"dash":[4,4],"width":2}}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert_eq!(spec.y_axis.border.dash, vec![4.0, 4.0]);
        assert!((spec.y_axis.border.width - 2.0).abs() < 1e-9);
    }

    #[test]
    fn scales_y_type_logarithmic_flows_into_spec_for_vertical_bar() {
        // 縦棒の値軸は y。type:"logarithmic" が y_axis.scale_kind::Logarithmic になり、
        // カテゴリ軸である x はそのまま Linear であること。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Logarithmic));
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn scales_x_type_logarithmic_flows_into_spec_for_horizontal_bar() {
        // 横棒(indexAxis:y)の値軸は x。type:"logarithmic" が x_axis.scale_kind::Logarithmic に
        // なり、カテゴリ軸である y はそのまま Linear であること。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Logarithmic));
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn scales_x_type_logarithmic_is_ignored_on_vertical_bar_category_axis() {
        // v1 スコープ外: 縦棒の x はカテゴリ軸なので type:"logarithmic" を黙って無視する
        // (Linear のまま)。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"x":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Linear));
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn scales_y_type_logarithmic_is_ignored_on_pie() {
        // v1 スコープ外: pie には値軸自体が無い。type:"logarithmic" を指定しても
        // strict でなければエラーにせず、単に無視する。
        let json = r##"{
          "type":"pie",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn logarithmic_y_axis_preserves_negative_values_and_zero() {
        // 描画時のスキップ判定と入力値の保持を分離する。負値も 0 も IR では元の値を保つ。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b","c"],"datasets":[{"data":[-5, 0, 10]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        let values = &spec.series[0].values;
        assert_eq!(values[0], -5.0, "negative value should remain in the IR");
        assert_eq!(values[1], 0.0, "zero should stay 0.0");
        assert_eq!(values[2], 10.0);
    }

    #[test]
    fn linear_y_axis_does_not_mask_negative_values() {
        // 対数軸でない場合は負値を一切変更しない(既存挙動を保つ)。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[-5, 10]}]}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert_eq!(spec.series[0].values[0], -5.0);
        assert_eq!(spec.series[0].values[1], 10.0);
    }

    #[test]
    fn strict_mode_accepts_scales_y_type_key() {
        // Task 4: strict allow-list に "type" を追加したことを確認する回帰テスト。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a"],"datasets":[{"data":[1]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        assert!(
            parse(json, true).is_ok(),
            "strict mode should accept scales.y.type"
        );
    }

    #[test]
    fn strict_mode_accepts_logarithmic_axes_for_scatter_and_bubble() {
        let scatter = r##"{
          "type":"scatter",
          "data":{"datasets":[{"data":[{"x":1,"y":2},{"x":10,"y":20}]}]},
          "options":{"scales":{"x":{"type":"logarithmic"},"y":{"type":"logarithmic"}}}
        }"##;
        let scatter_spec = parse(scatter, true).expect("strict scatter with log axes should parse");
        assert_eq!(scatter_spec.x_axis.scale_kind, ScaleKind::Logarithmic);
        assert_eq!(scatter_spec.y_axis.scale_kind, ScaleKind::Logarithmic);

        let bubble = r##"{
          "type":"bubble",
          "data":{"datasets":[{"data":[{"x":1,"y":2,"r":5},{"x":10,"y":20,"r":10}]}]},
          "options":{"scales":{"x":{"type":"logarithmic"},"y":{"type":"logarithmic"}}}
        }"##;
        let bubble_spec = parse(bubble, true).expect("strict bubble with log axes should parse");
        assert_eq!(bubble_spec.x_axis.scale_kind, ScaleKind::Logarithmic);
        assert_eq!(bubble_spec.y_axis.scale_kind, ScaleKind::Logarithmic);
    }

    #[test]
    fn scales_y_type_logarithmic_flows_into_spec_for_line() {
        // y_axis_is_log は `Bar{horizontal:false} | Line` の2アーム。この
        // テストは type:"line" 単体を通すことで、Line アームだけが担う被覆を
        // 固定する(`| ChartKind::Line` を消しても Bar 側テストは全部通ってしまうため)。
        let json = r##"{
          "type":"line",
          "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Logarithmic));
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn scales_y_type_logarithmic_is_ignored_on_mixed() {
        // v1 スコープ外: Mixed(bar+line 混在)は y_axis_is_log のどちらのアームにも
        // マッチしないため、type:"logarithmic" を指定しても Linear のまま無視される。
        // Mixed は基本 type:"bar"/"line" + dataset 別 type 上書きで構築する
        // (bar_base_with_line_dataset_is_mixed 等、tests/frontend_chartjs.rs の既存例に倣う)。
        let json = r##"{
          "type":"bar",
          "data":{"labels":["a","b","c"],
            "datasets":[{"label":"棒","data":[1,2,3]},{"type":"line","label":"折れ線","data":[4,5,6]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}
        }"##;
        let spec = parse(json, false).expect("parse ok");
        assert!(matches!(spec.kind, ChartKind::Mixed));
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn logarithmic_value_stacked_y_axis_is_supported() {
        let json = r#"{ "type":"bar",
          "data":{"labels":["a"],"datasets":[
            {"label":"s1","data":[10]},{"label":"s2","data":[10]}
          ]},
          "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"type":"logarithmic"}}} }"#;
        let spec = parse(json, false).expect("stacked + log y軸を受け付ける");
        assert!(
            matches!(
                spec.kind,
                ChartKind::Bar {
                    horizontal: false,
                    value_stacked: true,
                    ..
                }
            ),
            "stacked value axis should be represented in the ChartSpec"
        );
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Logarithmic));
    }

    #[test]
    fn logarithmic_value_stacked_x_axis_is_supported_on_horizontal_bar() {
        let json = r#"{ "type":"bar",
          "data":{"labels":["a"],"datasets":[
            {"label":"s1","data":[10]},{"label":"s2","data":[10]}
          ]},
          "options":{"indexAxis":"y",
            "scales":{"x":{"stacked":true,"type":"logarithmic"},"y":{"stacked":true}}} }"#;
        let spec = parse(json, false).expect("stacked + log x軸を受け付ける");
        assert!(
            matches!(
                spec.kind,
                ChartKind::Bar {
                    horizontal: true,
                    value_stacked: true,
                    ..
                }
            ),
            "stacked value axis should be represented in the horizontal ChartSpec"
        );
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Logarithmic));
    }

    #[test]
    fn logarithmic_placement_stacked_only_is_still_allowed() {
        // placement_stacked(index軸のみの積み上げ、値域は個別値のまま)は
        // log_value_domain のドメイン計算に影響しないため、対数軸と併用可能。
        let json = r#"{ "type":"bar",
          "data":{"labels":["a"],"datasets":[
            {"label":"s1","data":[10]},{"label":"s2","data":[20]}
          ]},
          "options":{"scales":{"x":{"stacked":true},"y":{"type":"logarithmic"}}} }"#;
        let spec = parse(json, false).expect("placement_stacked のみは対数軸と両立できる");
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Logarithmic));
    }
}
