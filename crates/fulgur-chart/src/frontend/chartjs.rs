//! chart.js v4 spec のデータ専用・静的サブセットを IR へ変換する。

use crate::color::parse_color;
use crate::ir::*;
use crate::palette::palette_color;
use serde::Deserialize;

#[derive(Deserialize)]
struct RawSpec {
    #[serde(rename = "type")]
    chart_type: String,
    data: RawData,
    #[serde(default)]
    options: RawOptions,
}

#[derive(Deserialize, Default)]
struct RawOptions {
    #[serde(rename = "indexAxis")]
    index_axis: Option<String>,
    #[serde(default)]
    plugins: RawPlugins,
    // 受理するが v1 の IR には未マップ（Task 9 のスケール対応時に使う）。
    #[allow(dead_code)]
    #[serde(default)]
    scales: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct RawPlugins {
    title: Option<RawTitle>,
    legend: Option<RawLegend>,
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
}

fn default_true() -> bool {
    true
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
    data: Vec<f64>,
    #[serde(rename = "backgroundColor")]
    background_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderColor")]
    border_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderWidth")]
    border_width: Option<f64>,
    #[serde(default)]
    fill: FillSpec,
    #[serde(default)]
    tension: f64,
    // pointRadius は受理するが v1 の IR には未マップ（Task 13 でマーカー対応時に使う）。
    #[allow(dead_code)]
    #[serde(rename = "pointRadius", default)]
    point_radius: Option<f64>,
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

/// `fill`: bool / 文字列("origin"等) を受ける。v1 は「塗るか否か」だけ解釈。
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum FillSpec {
    Bool(bool),
    // v1 はモード文字列("origin"等)の中身を解釈せず「塗る」とだけ扱う。
    // 文字列を bool と区別して受理するためにペイロードは必要だが、値は未使用。
    Mode(#[allow(dead_code)] String),
    #[default]
    Absent,
}

impl FillSpec {
    fn is_filled(&self) -> bool {
        match self {
            FillSpec::Bool(b) => *b,
            FillSpec::Mode(_) => true,
            FillSpec::Absent => false,
        }
    }
}

pub fn parse(json: &str, strict: bool) -> Result<ChartSpec, String> {
    if strict {
        check_unknown_keys(json)?;
    }
    let raw: RawSpec = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let kind = match raw.chart_type.as_str() {
        "bar" => ChartKind::Bar {
            horizontal: raw.options.index_axis.as_deref() == Some("y"),
        },
        "line" => ChartKind::Line,
        "pie" => ChartKind::Pie { donut_ratio: 0.0 },
        "doughnut" => ChartKind::Pie { donut_ratio: 0.5 },
        other => return Err(format!("未対応の type: {other}")),
    };

    let is_pie = matches!(kind, ChartKind::Pie { .. });
    let series = raw
        .data
        .datasets
        .into_iter()
        .enumerate()
        .map(|(i, ds)| {
            let n = ds.data.len();
            let fill = resolve_colors(ds.background_color, is_pie, i, n);
            let stroke = resolve_colors(ds.border_color, is_pie, i, n);
            Series {
                name: ds.label,
                values: ds.data,
                fill,
                stroke,
                stroke_width: ds.border_width.unwrap_or(default_border_width(&kind)),
                area: ds.fill.is_filled(),
                tension: ds.tension,
            }
        })
        .collect();

    Ok(ChartSpec {
        kind,
        series,
        categories: raw.data.labels,
        x_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            begin_at_zero: false,
            grid: true,
        },
        y_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            begin_at_zero: true,
            grid: true,
        },
        legend: legend_pos(&raw.options.plugins.legend),
        title: raw
            .options
            .plugins
            .title
            .as_ref()
            .filter(|t| t.display)
            .map(|t| t.text.clone()),
        width: 800.0,
        height: 450.0,
    })
}

fn default_border_width(kind: &ChartKind) -> f64 {
    match kind {
        ChartKind::Line => 3.0,
        _ => 1.0,
    }
}

/// 指定色(スカラ/配列)を点ごとの Vec<Color> に解決する。
/// 未指定: pie はスライス別パレット(n色)、それ以外は系列インデックスの 1 色。
fn resolve_colors(
    spec: Option<ScalarOrArray<String>>,
    is_pie: bool,
    series_index: usize,
    n: usize,
) -> Vec<Color> {
    match spec {
        Some(s) => s
            .into_vec()
            .iter()
            .map(|c| parse_color(c).unwrap_or_else(|| palette_color(series_index)))
            .collect(),
        None if is_pie => (0..n).map(palette_color).collect(),
        None => vec![palette_color(series_index)],
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

/// strict モード用: 既知キーのホワイトリストに照らし、未知キーを検出する。
///
/// 防御的に走査し、ノードが欠落/想定外の形なら `Ok(())` を返す（後段の通常パースが
/// 適切な Err を出す）。最初に見つけた未知キーのパスを `Err` で返す。
// strict 用ホワイトリスト。chart.js v4 サブセットとして「認識済み」のキーを並べる。
// IR へ未マップでも、設計で v1 サポート対象に挙げたキーは strict でも受理する
// （strict が弾くのは未知キーであり、認識済み・未完成キーではない）:
//   datalabels=Task16(最小データラベル) / scales=Task9 / pointRadius=Task13。
fn check_unknown_keys(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()), // 不正 JSON は後段パースに委ねる
    };
    let Some(top) = value.as_object() else {
        return Ok(()); // object でなければ後段パースに委ねる
    };

    check_object(top, &["type", "data", "options"], "")?;

    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
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
                            "fill",
                            "tension",
                            "pointRadius",
                        ],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }

    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["indexAxis", "plugins", "scales"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(
                plugins,
                &["title", "legend", "datalabels"],
                "options.plugins",
            )?;
        }
    }

    Ok(())
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
