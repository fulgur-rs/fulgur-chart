//! chart.js v4 spec のデータ専用・静的サブセットを IR へ変換する。

use crate::color::parse_color;
use crate::ir::*;
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
    #[serde(default)]
    theme: Option<RawTheme>,
    // scales.{x,y}.stacked のみ navigate する(積み上げ判定)。それ以外は未マップ。
    #[serde(default)]
    scales: Option<serde_json::Value>,
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
}

#[derive(Deserialize)]
struct RawDataLabels {
    #[serde(default)]
    display: Option<bool>,
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
    /// dataset 別の描画種別("bar"/"line")。混合チャートで使う。未指定なら chart 基本型に従う。
    #[serde(rename = "type", default)]
    dataset_type: Option<String>,
    data: DataField,
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

/// `data`: 数値配列(カテゴリ系)または点オブジェクト配列(scatter/bubble)。
/// untagged は `Nums` を先に試すので `[1,2]`→Nums、`[{x,y}]`→Points に解決される。
#[derive(Deserialize)]
#[serde(untagged)]
enum DataField {
    Nums(Vec<f64>),
    Points(Vec<RawPoint>),
}

#[derive(Deserialize, Clone)]
struct RawPoint {
    x: f64,
    y: f64,
    #[serde(default)]
    r: Option<f64>,
}

impl DataField {
    /// 数値配列なら採用、点配列なら空。カテゴリ系チャートの `values` 用。
    fn into_values(self) -> Vec<f64> {
        match self {
            DataField::Nums(v) => v,
            DataField::Points(_) => vec![],
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
            DataField::Nums(_) => vec![],
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

    // 積み上げ判定: options.scales.x.stacked または options.scales.y.stacked が true。
    // scales は緩く型付けされた serde_json::Value のまま navigate する(深い検証はしない)。
    let stacked = raw
        .options
        .scales
        .as_ref()
        .map(|s| {
            let f = |axis: &str| {
                s.get(axis)
                    .and_then(|a| a.get("stacked"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            f("x") || f("y")
        })
        .unwrap_or(false);

    // chart 基本型。bar/line のときだけ dataset 別 type による混合が起こりうる。
    // 基本型は SeriesType のフォールバックにも使う(bar→Bar, line→Line, それ以外→Bar(未使用))。
    let base_series_type = match raw.chart_type.as_str() {
        "line" => SeriesType::Line,
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

    // dataset 別 type は bar/line のみ対応。それ以外(scatter 等)が指定されたら、
    // 黙って基本型へフォールバックして点データを失わないよう明示的に拒否する。
    for ds in &raw.data.datasets {
        if let Some(t) = &ds.dataset_type {
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
    let is_mixable_base = matches!(raw.chart_type.as_str(), "bar" | "line");
    let has_bar = series_types.contains(&SeriesType::Bar);
    let has_line = series_types.contains(&SeriesType::Line);
    let bar_kind = || ChartKind::Bar {
        horizontal: raw.options.index_axis.as_deref() == Some("y"),
        stacked,
    };

    let kind = if is_mixable_base && has_bar && has_line {
        ChartKind::Mixed
    } else if is_mixable_base && has_line && !has_bar {
        ChartKind::Line
    } else if is_mixable_base && has_bar && !has_line {
        bar_kind()
    } else {
        // dataset 空(種別未確定)、または mixable でない型。基本 type で決める。
        match raw.chart_type.as_str() {
            "bar" => bar_kind(),
            "line" => ChartKind::Line,
            "pie" => ChartKind::Pie { donut_ratio: 0.0 },
            "doughnut" => ChartKind::Pie { donut_ratio: 0.5 },
            "scatter" => ChartKind::Scatter,
            "bubble" => ChartKind::Bubble,
            "radar" => ChartKind::Radar,
            other => return Err(format!("未対応の type: {other}")),
        }
    };

    // datalabels: キーが存在し display!=false なら有効。
    let data_labels = match &raw.options.plugins.datalabels {
        Some(dl) => dl.display != Some(false),
        None => false,
    };

    // テーマ解決(配色に使うため色解決より先に行う)。
    let theme = build_theme(raw.options.theme);

    let is_pie = matches!(kind, ChartKind::Pie { .. });
    // scatter/bubble はどちらも点データ(Series.points)を使う線形×線形チャート。
    let is_point_based = matches!(kind, ChartKind::Scatter | ChartKind::Bubble);
    let series: Vec<Series> = raw
        .data
        .datasets
        .into_iter()
        .enumerate()
        .map(|(i, ds)| {
            // 点ベースは点データ、それ以外は数値配列を採る。`data` は一度だけ消費する。
            let (values, points) = if is_point_based {
                (vec![], ds.data.into_points())
            } else {
                (ds.data.into_values(), vec![])
            };
            let n = if is_point_based {
                points.len()
            } else {
                values.len()
            };
            let fill = resolve_colors(ds.background_color, is_pie, i, n, &theme.palette);
            let stroke = resolve_colors(ds.border_color, is_pie, i, n, &theme.palette);
            // 実効描画種別。線の既定線幅(3.0)を chart 基本型でなく系列種別で決めるため、
            // 単一種別(全 Line→3.0 / 全 Bar→1.0)では従来と byte 一致し、混合では line だけ太くなる。
            let series_type = series_types[i];
            Series {
                name: ds.label,
                values,
                points,
                fill,
                stroke,
                stroke_width: ds.border_width.unwrap_or(default_border_width(series_type)),
                area: ds.fill.is_filled(),
                tension: ds.tension,
                series_type,
            }
        })
        .collect();

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
    // カテゴリ系は従来どおり y のみゼロ起点。
    let y_begin_at_zero = !is_point_based;

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
            begin_at_zero: y_begin_at_zero,
            grid: true,
        },
        legend: legend_pos(&raw.options.plugins.legend),
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: 800.0,
        height: 450.0,
        data_labels,
        theme,
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
    if let Some(sz) = raw.font_size {
        if sz.is_finite() && sz > 0.0 {
            theme.font_size = sz;
        }
    }
    theme
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
) -> Vec<Color> {
    let pick = |i: usize| palette[i % palette.len()];
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
                            "type",
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
        check_object(
            options,
            &["indexAxis", "plugins", "scales", "theme"],
            "options",
        )?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(
                plugins,
                &["title", "legend", "datalabels"],
                "options.plugins",
            )?;
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
        // scales 配下も検査する。stacked は描画に効く load-bearing キーなので、
        // typo(例 stakced)を strict で取りこぼさないようにする。各軸は設計が認める
        // サブセットのみ許可(stacked のみ実装、他は認識済み・未実装)。
        if let Some(scales) = options.get("scales").and_then(|v| v.as_object()) {
            check_object(scales, &["x", "y"], "options.scales")?;
            for axis in ["x", "y"] {
                if let Some(ax) = scales.get(axis).and_then(|v| v.as_object()) {
                    check_object(
                        ax,
                        &["stacked", "min", "max", "title", "grid", "beginAtZero"],
                        &format!("options.scales.{axis}"),
                    )?;
                }
            }
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
