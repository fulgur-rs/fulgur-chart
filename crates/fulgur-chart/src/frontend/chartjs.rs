//! chart.js v4 spec のデータ専用・静的サブセットを IR へ変換する。

use crate::color::parse_color;
use crate::ir::*;
use serde::Deserialize;
use std::collections::HashMap;

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
    // scales.<index 軸>.stacked → placement_stacked(配置)、<値軸>.stacked → value_stacked(値域・累積)。
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
    outlabels: Option<RawOutlabels>,
}

#[derive(Deserialize)]
struct RawDataLabels {
    #[serde(default)]
    display: Option<bool>,
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
    // scatter のマーカー半径。Series.point_radius へマップする。
    #[serde(rename = "pointRadius", default)]
    point_radius: Option<f64>,
}

/// `data`: 数値配列(カテゴリ系)、ネスト配列(boxplot)、または点オブジェクト配列(scatter/bubble)。
/// untagged は順に試す: `Nums` → `[1,2]`、`Boxes` → `[[1,2,3,4,5]]`、`Points` → `[{x,y}]`。
#[derive(Deserialize)]
#[serde(untagged)]
enum DataField {
    Nums(Vec<f64>),
    Boxes(Vec<Vec<f64>>),
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
    /// 数値配列なら採用、それ以外は空。カテゴリ系チャートの `values` 用。
    fn into_values(self) -> Vec<f64> {
        match self {
            DataField::Nums(v) => v,
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
    /// 各行は厳密に [min, q1, median, q3, max] の 5 要素でなければならない。
    /// 5 要素以外の行はバイト数ガードをバイパスできるため拒否し NaN 行として扱う。
    fn into_box_points(self) -> Vec<crate::ir::BoxPoint> {
        match self {
            DataField::Boxes(rows) => rows
                .into_iter()
                .map(|row| {
                    if row.len() != 5 {
                        return crate::ir::BoxPoint {
                            min: f64::NAN,
                            q1: f64::NAN,
                            median: f64::NAN,
                            q3: f64::NAN,
                            max: f64::NAN,
                        };
                    }
                    crate::ir::BoxPoint {
                        min: row[0],
                        q1: row[1],
                        median: row[2],
                        q3: row[3],
                        max: row[4],
                    }
                })
                .collect(),
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
            check_unknown_keys(json, allow_outlabels)?;
        }
    }

    let raw: RawSpec = serde_json::from_str(json).map_err(|e| e.to_string())?;

    // 積み上げ判定: chart.js は配置(dodge/同スロット)と値累積を独立した軸で制御する。
    // index 軸の stacked → placement_stacked(棒の配置)
    // 値軸の stacked  → value_stacked(値累積・値域計算)
    // 既知の制約: per-dataset の stack プロパティによる積み上げは未対応(scales 経由のみ)。
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
            .and_then(|s| {
                s.get(axis)
                    .and_then(|a| a.get("stacked"))
                    .and_then(|v| v.as_bool())
            })
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

    // datalabels: 既存は「キーが存在し display!=false なら有効」。
    // progress のみ既定 ON（QuickChart 準拠）。明示 display:false は尊重する。
    let data_labels = match (&raw.options.plugins.datalabels, &kind) {
        (Some(dl), _) => dl.display != Some(false),
        (None, ChartKind::Progress) => true,
        (None, _) => false,
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
    for ds in &raw.data.datasets {
        let mismatched = match &ds.data {
            DataField::Nums(v) => (is_point_based || is_boxplot) && !v.is_empty(),
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

    // chart.js v4 の Colors プラグインはデータセットのいずれかに backgroundColor か
    // borderColor が指定されていれば chart 全体をスキップする(per-dataset ではない)。
    let chart_has_explicit_colors = raw
        .data
        .datasets
        .iter()
        .any(|ds| ds.background_color.is_some() || ds.border_color.is_some());

    let series: Vec<Series> = raw
        .data
        .datasets
        .into_iter()
        .enumerate()
        .map(|(i, ds)| {
            // 点ベースは点データ、boxplot はボックスデータ、それ以外は数値配列を採る。`data` は一度だけ消費する。
            let (values, points, box_points) = if is_point_based {
                (vec![], ds.data.into_points(), vec![])
            } else if is_boxplot {
                (vec![], vec![], ds.data.into_box_points())
            } else {
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
            Series {
                name: ds.label,
                values,
                points,
                fill,
                stroke,
                stroke_width: ds.border_width.unwrap_or(default_border_width(series_type)),
                area: ds.fill.is_filled(),
                tension: normalize_tension(ds.tension),
                series_type,
                point_radius: ds.point_radius,
                box_points,
                tree: vec![],
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
    let is_line = matches!(kind, ChartKind::Line);
    let value_begin_at_zero = !is_point_based && !is_sparkline && !is_line;

    // suggestedMin/suggestedMax および beginAtZero: options.scales.{x,y} から取得する。
    let scales_val = raw.options.scales.as_ref();
    let x_baz_json = scales_val
        .and_then(|s| s.get("x"))
        .and_then(|a| a.get("beginAtZero"))
        .and_then(|v| v.as_bool());
    let y_baz_json = scales_val
        .and_then(|s| s.get("y"))
        .and_then(|a| a.get("beginAtZero"))
        .and_then(|v| v.as_bool());
    let x_begin_at_zero = x_baz_json.unwrap_or(is_horizontal && value_begin_at_zero);
    let y_begin_at_zero =
        y_baz_json.unwrap_or(!is_horizontal && value_begin_at_zero && !is_boxplot);
    let suggested_min_y = scales_val
        .and_then(|s| s.get("y"))
        .and_then(|a| a.get("suggestedMin"))
        .and_then(|v| v.as_f64());
    let suggested_max_y = scales_val
        .and_then(|s| s.get("y"))
        .and_then(|a| a.get("suggestedMax"))
        .and_then(|v| v.as_f64());
    let suggested_min_x = scales_val
        .and_then(|s| s.get("x"))
        .and_then(|a| a.get("suggestedMin"))
        .and_then(|v| v.as_f64());
    let suggested_max_x = scales_val
        .and_then(|s| s.get("x"))
        .and_then(|a| a.get("suggestedMax"))
        .and_then(|v| v.as_f64());
    // category スケールの offset。明示時のみ尊重(既定 false=edge-to-edge)。
    // line レイアウトの x 軸のみが消費する(y は line の値軸)。
    let x_offset = scales_val
        .and_then(|s| s.get("x"))
        .and_then(|a| a.get("offset"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let y_offset = scales_val
        .and_then(|s| s.get("y"))
        .and_then(|a| a.get("offset"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ChartSpec {
        kind,
        series,
        categories: raw.data.labels,
        x_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: suggested_min_x,
            suggested_max: suggested_max_x,
            begin_at_zero: x_begin_at_zero,
            offset: x_offset,
            grid: true,
        },
        y_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: suggested_min_y,
            suggested_max: suggested_max_y,
            begin_at_zero: y_begin_at_zero,
            offset: y_offset,
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
    if let Some(sz) = raw.font_size {
        if sz.is_finite() && sz > 0.0 {
            theme.font_size = sz;
        }
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
    if let Some(s) = raw.stretch {
        if s.is_finite() && s >= 0.0 {
            cfg.stretch = s;
        }
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

/// strict モード用: 既知キーのホワイトリストに照らし、未知キーを検出する。
///
/// 防御的に走査し、ノードが欠落/想定外の形なら `Ok(())` を返す（後段の通常パースが
/// 適切な Err を出す）。最初に見つけた未知キーのパスを `Err` で返す。
// strict 用ホワイトリスト。chart.js v4 サブセットとして「認識済み」のキーを並べる。
// IR へ未マップでも、設計で v1 サポート対象に挙げたキーは strict でも受理する
// （strict が弾くのは未知キーであり、認識済み・未完成キーではない）:
//   datalabels=Task16(最小データラベル) / scales=Task9 / pointRadius=Task13。
fn check_unknown_keys(json: &str, allow_outlabels: bool) -> Result<(), String> {
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
        check_object(
            options,
            &["indexAxis", "plugins", "scales", "theme"],
            "options",
        )?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            let allowed_plugins: &[&str] = if allow_outlabels {
                &["title", "legend", "datalabels", "outlabels"]
            } else {
                &["title", "legend", "datalabels"]
            };
            check_object(plugins, allowed_plugins, "options.plugins")?;
            if let Some(dl) = plugins.get("datalabels").and_then(|v| v.as_object()) {
                check_object(dl, &["display"], "options.plugins.datalabels")?;
            }
            if allow_outlabels {
                if let Some(ol) = plugins.get("outlabels").and_then(|v| v.as_object()) {
                    check_object(
                        ol,
                        &["text", "color", "backgroundColor", "stretch"],
                        "options.plugins.outlabels",
                    )?;
                }
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
                        &[
                            "stacked",
                            "min",
                            "max",
                            "title",
                            "grid",
                            "beginAtZero",
                            "suggestedMin",
                            "suggestedMax",
                            "offset",
                        ],
                        &format!("options.scales.{axis}"),
                    )?;
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
    check_object(top, &["type", "data", "options"], "")?;
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
            check_object(plugins, &["title", "legend"], "options.plugins")?;
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
    check_object(top, &["type", "data", "options"], "")?;
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
    check_object(top, &["type", "data", "options"], "")?;
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
        grid: false,
    };

    let series = vec![Series {
        name: String::new(),
        values: vec![],
        points: vec![],
        fill: vec![],
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        tension: 0.0,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: forest,
    }];

    Ok(ChartSpec {
        kind: ChartKind::Treemap,
        series,
        categories: vec![],
        x_axis: no_axis.clone(),
        y_axis: no_axis,
        legend: crate::ir::LegendPos::None,
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
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
    check_object(top, &["type", "data", "options"], "")?;
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

fn parse_matrix(json: &str) -> Result<ChartSpec, String> {
    #[derive(Deserialize)]
    struct MatrixWrapper {
        data: MatrixRawData,
        #[serde(default)]
        options: RawOptions,
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
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
        })
        .collect();

    Ok(ChartSpec {
        kind: ChartKind::Matrix { color_lo, color_hi },
        series,
        categories: x_cats,
        x_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None,
            suggested_max: None,
            begin_at_zero: false,
            offset: false,
            grid: false,
        },
        y_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None,
            suggested_max: None,
            begin_at_zero: false,
            offset: false,
            grid: false,
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
        data_labels: false,
        theme,
    })
}

fn parse_gauge(json: &str, radial: bool) -> Result<ChartSpec, String> {
    use crate::ir::ChartKind;

    #[derive(Deserialize)]
    struct GaugeWrapper {
        data: GaugeRawData,
        #[serde(default)]
        options: serde_json::Value,
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
        tension: 0.0,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: vec![],
    }];

    Ok(ChartSpec {
        kind,
        series,
        categories: vec![],
        x_axis: zero_axis(),
        y_axis: zero_axis(),
        legend: LegendPos::None,
        title,
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
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
        grid: false,
    }
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
        assert_eq!(spec.series[0].tension, 1.0);

        let spec = parse(
            r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3],"tension":-2}]}}"#,
            false,
        )
        .unwrap();
        assert_eq!(spec.series[0].tension, 0.0);
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
}
