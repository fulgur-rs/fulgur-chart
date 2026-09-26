//! チャート意味モデル: chart.js と数値照合するための、解決済み色・軸目盛り・
//! counts を持つシリアライズ可能な中間表現。描画はせず IR + layout から構築する。

use serde::Serialize;

use crate::ir::{ChartKind, ChartSpec, Color, ScaleKind, SizeMode, XPositions};
use crate::temporal::TemporalTick;
use crate::text::TextMeasurer;

/// 解決済み色を正規化 rgba 文字列にする(plan の正規化規約に従う)。
pub fn rgba_string(c: &Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, fmt_alpha(c.a))
}

/// alpha を正規化整形する(>=1→"1", <=0→"0", それ以外は 3 桁丸め・末尾ゼロ除去)。
fn fmt_alpha(a: f32) -> String {
    if a >= 1.0 {
        return "1".to_string();
    }
    if a <= 0.0 {
        return "0".to_string();
    }
    let r = (a as f64 * 1000.0).round() / 1000.0;
    // f64 の Display は最短往復表現を出すため n/1000 に末尾ゼロは付かない。
    format!("{r}")
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ChartModel {
    pub meta: Meta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axes: Option<Axes>,
    pub series: Vec<SeriesModel>,
    pub counts: Counts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Meta {
    pub r#type: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Axes {
    pub x: AxisModel,
    pub y: AxisModel,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AxisModel {
    pub kind: String, // "linear" | "logarithmic" | "category" | "temporal"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<Vec<f64>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SeriesModel {
    pub label: String,
    pub fill: Vec<String>,
    pub stroke: Vec<String>,
    /// `None` は入力 JSON の `null`(IR では `f64::NAN` センチネル)に対応。
    /// serde_json は NaN をシリアライズできないため、NaN を `None` に落として
    /// `null` として出力する。
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Counts {
    pub datasets: usize,
    /// ラベル非空のデータセット数。描画される凡例エントリ数ではない
    /// (pie/doughnut はスライスごとに 1 エントリを描画する)。
    pub legend_items: usize,
    pub x_ticks: usize,
    pub y_ticks: usize,
}

/// 矩形/プロット領域の正規化座標(チャート間ジオメトリ照合用)。
#[derive(Debug, Serialize, PartialEq)]
pub struct RectN {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 単一データ要素の正規化ジオメトリ。n* はプロット領域基準 [0,1]。
#[derive(Debug, Serialize, PartialEq)]
pub struct ElemN {
    pub series: usize,
    pub index: usize,
    pub kind: String,
    pub nx: f64,
    pub ny: f64,
    pub nw: f64,
    pub nh: f64,
}

/// チャートのジオメトリ。plot_area はキャンバス基準 [0,1]、elements はプロット領域基準。
#[derive(Debug, Serialize, PartialEq)]
pub struct Geometry {
    pub plot_area: RectN,
    pub elements: Vec<ElemN>,
}

fn model_dimensions(spec: &ChartSpec, m: &TextMeasurer) -> (f64, f64) {
    if matches!(spec.size_mode, SizeMode::PlotArea) && matches!(spec.kind, ChartKind::Line { .. }) {
        let frame = crate::layout::common::compute(spec, m);
        (frame.scene_width, frame.scene_height)
    } else {
        (spec.width, spec.height)
    }
}

/// 縦棒のジオメトリを共有 `vertical_bar_boxes` から構築する(描画と単一真実源)。
/// 縦棒以外、または退化プロット領域(幅/高さ<=0)は None。
fn compute_geometry(spec: &ChartSpec, m: &TextMeasurer) -> Option<Geometry> {
    match &spec.kind {
        ChartKind::Bar {
            horizontal: false, ..
        } => {
            let frame = crate::layout::common::compute(spec, m);
            let pw = frame.plot_right - frame.plot_left;
            let ph = frame.plot_bottom - frame.plot_top;
            let (model_width, model_height) = model_dimensions(spec, m);
            if pw <= 0.0 || ph <= 0.0 || model_width <= 0.0 || model_height <= 0.0 {
                return None;
            }
            let plot_area = RectN {
                x: frame.plot_left / model_width,
                y: frame.plot_top / model_height,
                w: pw / model_width,
                h: ph / model_height,
            };
            let elements = crate::layout::bar::vertical_bar_boxes(spec, &frame)
                .iter()
                .map(|b| ElemN {
                    series: b.series,
                    index: b.index,
                    kind: "bar".to_string(),
                    nx: (b.x - frame.plot_left) / pw,
                    ny: (b.y - frame.plot_top) / ph,
                    nw: b.w / pw,
                    nh: b.h / ph,
                })
                .collect();
            Some(Geometry {
                plot_area,
                elements,
            })
        }
        ChartKind::Bar {
            horizontal: true, ..
        } if crate::layout::common::is_temporal_scale(&spec.x_axis)
            || crate::layout::common::is_temporal_scale(&spec.y_axis) =>
        {
            let layout = crate::layout::bar::horizontal_bar_layout(spec, m);
            let pw = layout.plot_right - layout.plot_left;
            let ph = layout.plot_bottom - layout.plot_top;
            let (model_width, model_height) = model_dimensions(spec, m);
            if pw <= 0.0 || ph <= 0.0 || model_width <= 0.0 || model_height <= 0.0 {
                return None;
            }
            let plot_area = RectN {
                x: layout.plot_left / model_width,
                y: layout.plot_top / model_height,
                w: pw / model_width,
                h: ph / model_height,
            };
            let elements = layout
                .bars
                .iter()
                .map(|bar| ElemN {
                    series: bar.series,
                    index: bar.index,
                    kind: "bar".to_string(),
                    nx: (bar.x - layout.plot_left) / pw,
                    ny: (bar.y - layout.plot_top) / ph,
                    nw: bar.w / pw,
                    nh: bar.h / ph,
                })
                .collect();
            Some(Geometry {
                plot_area,
                elements,
            })
        }
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square => {
            let layout = crate::layout::scatter::compute_scatter_layout(spec, m);
            let pw = layout.plot_right - layout.plot_left;
            let ph = layout.plot_bottom - layout.plot_top;
            if pw <= 0.0 || ph <= 0.0 || spec.width <= 0.0 || spec.height <= 0.0 {
                return None;
            }
            let plot_area = RectN {
                x: layout.plot_left / spec.width,
                y: layout.plot_top / spec.height,
                w: pw / spec.width,
                h: ph / spec.height,
            };
            let elements = crate::layout::scatter::scatter_points(spec, &layout)
                .iter()
                .map(|b| ElemN {
                    series: b.series,
                    index: b.index,
                    kind: b.kind.to_string(),
                    nx: (b.cx - layout.plot_left) / pw,
                    ny: (b.cy - layout.plot_top) / ph,
                    nw: if matches!(b.kind, "bubble" | "square") {
                        b.r / pw
                    } else {
                        0.0
                    },
                    nh: if b.kind == "square" { b.r / ph } else { 0.0 },
                })
                .collect();
            Some(Geometry {
                plot_area,
                elements,
            })
        }
        ChartKind::Line { .. } => {
            let frame = crate::layout::common::compute(spec, m);
            let pw = frame.plot_right - frame.plot_left;
            let ph = frame.plot_bottom - frame.plot_top;
            let (model_width, model_height) = model_dimensions(spec, m);
            if pw <= 0.0 || ph <= 0.0 || model_width <= 0.0 || model_height <= 0.0 {
                return None;
            }
            let plot_area = RectN {
                x: frame.plot_left / model_width,
                y: frame.plot_top / model_height,
                w: pw / model_width,
                h: ph / model_height,
            };
            let elements = crate::layout::line::line_points(spec, &frame)
                .iter()
                .map(|b| ElemN {
                    series: b.series,
                    index: b.index,
                    kind: b.kind.to_string(),
                    nx: (b.cx - frame.plot_left) / pw,
                    ny: (b.cy - frame.plot_top) / ph,
                    nw: 0.0,
                    nh: 0.0,
                })
                .collect();
            Some(Geometry {
                plot_area,
                elements,
            })
        }
        _ => None,
    }
}

/// 描画要素数(scatter/bubble は points、boxplot は box_points、violin は sample groups)。
fn element_count(s: &crate::ir::Series) -> usize {
    if !s.violin_samples.is_empty() {
        s.violin_samples.len()
    } else if !s.box_points.is_empty() {
        s.box_points.len()
    } else if s.points.is_empty() {
        s.values.len()
    } else {
        s.points.len()
    }
}

/// 色ベクタを「要素ごと rgba」に展開しつつ、全要素同色なら長さ1へ畳む。
/// 色解決はレンダラと共有する `ir::color_at` を使い、モデルと描画の差異を防ぐ。
/// 要素数 0(空データセット)では描画マークが無いため空ベクタを返す
/// (chart.js 抽出器も `meta.data.length`=0 で空配列を返すため、これに揃える)。
fn colors_to_strings(colors: &[Color], n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let all: Vec<String> = (0..n)
        .map(|i| rgba_string(&crate::ir::color_at(colors, i)))
        .collect();
    if all.iter().all(|x| x == &all[0]) {
        vec![all[0].clone()]
    } else {
        all
    }
}

fn chart_type_name(kind: &ChartKind) -> &'static str {
    match kind {
        ChartKind::Bar {
            horizontal: true, ..
        } => "bar-horizontal",
        ChartKind::Bar { .. } => "bar",
        ChartKind::Line { .. } => "line",
        ChartKind::Pie { cutout, .. } if cutout.is_doughnut() => "doughnut",
        ChartKind::Pie { .. } => "pie",
        ChartKind::Scatter => "scatter",
        ChartKind::Bubble => "bubble",
        ChartKind::Square => "square",
        ChartKind::Radar => "radar",
        ChartKind::Mixed => "mixed",
        ChartKind::Matrix { .. } => "matrix",
        ChartKind::VegaRect { .. } => "vegaRect",
        ChartKind::Progress => "progress",
        ChartKind::BoxPlot => "boxplot",
        ChartKind::Violin { horizontal: true } => "horizontalViolin",
        ChartKind::Violin { horizontal: false } => "violin",
        ChartKind::Sparkline => "sparkline",
        ChartKind::PolarArea => "polarArea",
        ChartKind::RadialGauge { .. } => "radialGauge",
        ChartKind::Gauge { .. } => "gauge",
        ChartKind::OutlabeledPie { donut_ratio, .. } if *donut_ratio > 0.0 => "outlabeledDoughnut",
        ChartKind::OutlabeledPie { .. } => "outlabeledPie",
        ChartKind::Treemap => "treemap",
        ChartKind::WordCloud { .. } => "wordCloud",
        ChartKind::Sankey { .. } => "sankey",
    }
}

/// 軸抜き(meta/series/counts のみ)のコアモデル。Task 3 で軸を載せる。
pub fn build_model_core(spec: &ChartSpec) -> ChartModel {
    // pie/doughnut のスライス境界は renderer が白(pie::SLICE_STROKE)で固定描画し、
    // 解析済み borderColor を使わない。モデルも実描画に合わせて白を主張する
    // (spec が borderColor を指定しても fulgur はそれを無視して白を描く点を、
    // chart.js との diff で正しく顕在化させるため)。
    let is_pie = matches!(
        spec.kind,
        ChartKind::Pie { .. } | ChartKind::PolarArea | ChartKind::OutlabeledPie { .. }
    );
    let series: Vec<SeriesModel> = spec
        .series
        .iter()
        .map(|s| {
            let n = element_count(s);
            let stroke = if is_pie {
                colors_to_strings(&[crate::layout::pie::SLICE_STROKE], n)
            } else {
                colors_to_strings(&s.stroke, n)
            };
            SeriesModel {
                label: s.name.clone(),
                fill: colors_to_strings(&s.fill, n),
                stroke,
                values: s
                    .values
                    .iter()
                    .map(|v| if v.is_finite() { Some(*v) } else { None })
                    .collect(),
            }
        })
        .collect();
    let legend_items = if crate::layout::common::temporal_plot_right_legend_title(spec).is_some() {
        spec.series.len()
    } else {
        spec.series.iter().filter(|s| !s.name.is_empty()).count()
    };
    let mut counts = Counts {
        datasets: spec.series.len(),
        legend_items,
        x_ticks: spec.categories.len(),
        y_ticks: 0,
    };
    // VegaRect は series/categories が空で、ラベルは ChartKind::VegaRect の
    // x_labels/y_labels に直接持たれる。既存の counts 算出だと datasets=0, x_ticks=0
    // と誤報告になるため、rect 側の情報源で上書きする。build_model 側の compute_axes は
    // VegaRect で None を返すので y_ticks は clobber されない。
    if let ChartKind::VegaRect {
        x_labels, y_labels, ..
    } = &spec.kind
    {
        counts.datasets = 1;
        counts.legend_items = 0; // rect には legend なし
        counts.x_ticks = x_labels.len();
        counts.y_ticks = y_labels.len();
    }
    ChartModel {
        meta: Meta {
            r#type: chart_type_name(&spec.kind).to_string(),
            width: spec.width,
            height: spec.height,
        },
        axes: None,
        series,
        counts,
        geometry: None,
    }
}

/// NiceTicks を線形軸モデルへ変換する。
fn linear_axis(t: &crate::scale::NiceTicks) -> AxisModel {
    AxisModel {
        kind: "linear".to_string(),
        labels: None,
        min: Some(t.min),
        max: Some(t.max),
        step: Some(t.step),
        ticks: Some(t.ticks.clone()),
    }
}

/// LogTicks 由来の major ticks(NiceTicks 形状で受け取る)を対数軸モデルへ変換する。
/// step は decade 間隔が一定でない(1,10,100,...)ため意味を持たず、常に None にする
/// (`t.step` は layout 側が内部的に使う 0.0 番兵であり、ここで漏らさない)。
fn logarithmic_axis(t: &crate::scale::NiceTicks) -> AxisModel {
    AxisModel {
        kind: "logarithmic".to_string(),
        labels: None,
        min: Some(t.min),
        max: Some(t.max),
        step: None,
        ticks: Some(t.ticks.clone()),
    }
}

/// カテゴリ軸モデル(ラベルのみ)。
fn category_axis(labels: &[String]) -> AxisModel {
    AxisModel {
        kind: "category".to_string(),
        labels: Some(labels.to_vec()),
        min: None,
        max: None,
        step: None,
        ticks: None,
    }
}

#[cfg(test)]
fn temporal_axis(unix_millis: &[i64], ticks: &[TemporalTick]) -> AxisModel {
    let min = unix_millis.iter().copied().min().map(|value| value as f64);
    let max = unix_millis.iter().copied().max().map(|value| value as f64);
    temporal_axis_with_domain(min, max, ticks)
}

fn temporal_axis_with_domain(
    min: Option<f64>,
    max: Option<f64>,
    ticks: &[TemporalTick],
) -> AxisModel {
    AxisModel {
        kind: "temporal".to_string(),
        labels: Some(ticks.iter().map(|tick| tick.label.clone()).collect()),
        min,
        max,
        step: ticks.windows(2).next().and_then(|first| {
            let expected = i128::from(first[1].unix_millis) - i128::from(first[0].unix_millis);
            ticks
                .windows(2)
                .all(|window| {
                    i128::from(window[1].unix_millis) - i128::from(window[0].unix_millis)
                        == expected
                })
                .then_some(expected as f64)
        }),
        ticks: Some(ticks.iter().map(|tick| tick.unix_millis as f64).collect()),
    }
}

fn value_axis_model(
    axis: &crate::ir::AxisSpec,
    ticks: &crate::scale::NiceTicks,
    temporal_ticks: &[TemporalTick],
) -> AxisModel {
    if matches!(axis.scale_kind, ScaleKind::Time | ScaleKind::Timeseries) {
        temporal_axis_with_domain(Some(ticks.min), Some(ticks.max), temporal_ticks)
    } else if axis.scale_kind == ScaleKind::Logarithmic {
        logarithmic_axis(ticks)
    } else {
        linear_axis(ticks)
    }
}

/// 直交チャートの (x 軸, y 軸, y 目盛り数) を計算する。値(線形)軸は描画上の向きに
/// 関わらず常に `y` に載せ、カテゴリ軸を `x` に載せる — JS 抽出器の正規化規約
/// (線形値軸→y・カテゴリ→x)と揃え、apples-to-apples 照合を可能にするため。
/// 値域・nice_ticks は renderer の各 layout と同じ関数を共有し、描画との乖離を防ぐ。
/// 軸を持たないチャート(pie/radar/matrix/progress)は None を返す。
fn compute_axes(spec: &ChartSpec, m: &TextMeasurer) -> Option<(AxisModel, AxisModel, usize)> {
    use crate::scale::nice_ticks;
    if let (ChartKind::Line { .. }, XPositions::Temporal { unix_millis }) =
        (&spec.kind, &spec.x_positions)
    {
        let frame = crate::layout::common::compute(spec, m);
        let (min, max) = crate::layout::common::x_temporal_domain(spec, unix_millis);
        let x_model =
            temporal_axis_with_domain(Some(min as f64), Some(max as f64), &frame.temporal_ticks);
        let y_model = value_axis_model(&spec.y_axis, &frame.ticks, &frame.y_temporal_ticks);
        return Some((x_model, y_model, frame.ticks.ticks.len()));
    }

    match &spec.kind {
        // 縦棒・線・mixed: 値軸=y(layout::common::compute と共有)、カテゴリ=x。
        // Mixed は frontend 側で対数軸をスコープ外にしているため scale_kind は
        // 常に Linear だが、念のため他アームと同じ分岐を通す。
        ChartKind::Bar {
            horizontal: false, ..
        }
        | ChartKind::Line { .. }
        | ChartKind::Mixed => {
            let frame = crate::layout::common::compute(spec, m);
            let x_model = match &spec.x_positions {
                XPositions::Temporal { unix_millis } => {
                    let (min, max) = crate::layout::common::x_temporal_domain(spec, unix_millis);
                    temporal_axis_with_domain(
                        Some(min as f64),
                        Some(max as f64),
                        &frame.temporal_ticks,
                    )
                }
                XPositions::Category => category_axis(&spec.categories),
            };
            let y_model = value_axis_model(&spec.y_axis, &frame.ticks, &frame.y_temporal_ticks);
            Some((x_model, y_model, frame.ticks.ticks.len()))
        }
        // 横棒: 値軸は描画上 x だが照合のため y に載せる。値域は build_horizontal と
        // 同じく x_axis から読む。カテゴリ=x。対数軸の場合も build_horizontal と同じ
        // log_axis_ticks 経路(tight ドメイン、P1 修正済み)を使い、対数軸の step は
        // logarithmic_axis で None として公開する。
        ChartKind::Bar {
            horizontal: true, ..
        } => {
            let (lo, hi) = crate::layout::common::value_domain(spec, &spec.x_axis);
            let (t, value_model) = if spec.x_axis.scale_kind == ScaleKind::Logarithmic {
                let (nt, _) = crate::scale::log_axis_ticks(lo, hi);
                let model = logarithmic_axis(&nt);
                (nt, model)
            } else if matches!(
                spec.x_axis.scale_kind,
                ScaleKind::Time | ScaleKind::Timeseries
            ) {
                let ticks = crate::layout::common::temporal_axis_ticks(
                    &spec.x_axis,
                    lo as i64,
                    hi as i64,
                    spec.width,
                );
                let nt = crate::scale::NiceTicks {
                    min: lo,
                    max: hi,
                    step: 0.0,
                    ticks: ticks.iter().map(|tick| tick.unix_millis as f64).collect(),
                };
                let model = value_axis_model(&spec.x_axis, &nt, &ticks);
                (nt, model)
            } else {
                let nt = crate::layout::common::apply_hard_axis_bounds(
                    nice_ticks(lo, hi, 10),
                    &spec.x_axis,
                );
                let model = linear_axis(&nt);
                (nt, model)
            };
            let index_axis = match &spec.y_positions {
                XPositions::Temporal { unix_millis } => {
                    let (min, max) = crate::layout::common::temporal_index_domain(
                        unix_millis,
                        &spec.y_axis,
                        true,
                    );
                    let ticks = crate::layout::common::temporal_axis_ticks(
                        &spec.y_axis,
                        min,
                        max,
                        spec.height,
                    );
                    temporal_axis_with_domain(Some(min as f64), Some(max as f64), &ticks)
                }
                XPositions::Category => category_axis(&spec.categories),
            };
            Some((index_axis, value_model, t.ticks.len()))
        }
        // scatter/bubble/square: x・y とも数値軸。renderer と同じ layout/ticks を共有する。
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square => {
            let layout = crate::layout::scatter::compute_scatter_layout(spec, m);
            let x = if matches!(
                spec.x_axis.scale_kind,
                ScaleKind::Time | ScaleKind::Timeseries
            ) {
                temporal_axis_with_domain(
                    Some(layout.x_ticks.min),
                    Some(layout.x_ticks.max),
                    &layout.x_temporal_ticks,
                )
            } else if spec.x_axis.scale_kind == crate::ir::ScaleKind::Logarithmic {
                logarithmic_axis(&layout.x_ticks)
            } else {
                linear_axis(&layout.x_ticks)
            };
            let y = value_axis_model(&spec.y_axis, &layout.y_ticks, &layout.y_temporal_ticks);
            Some((x, y, layout.y_ticks.ticks.len()))
        }
        // boxplot: カテゴリ x、線形 y。ドメインは layout::boxplot と共有。
        ChartKind::BoxPlot => {
            let t = crate::layout::boxplot::compute_frame(spec, m).ticks;
            Some((
                category_axis(&spec.categories),
                linear_axis(&t),
                t.ticks.len(),
            ))
        }
        // violin は描画向きに関わらずモデル上 category=x / value=y に正規化する。
        // numeric domain と ticks は実際の描画 frame から得て、水平では x 軸設定を使う。
        ChartKind::Violin { horizontal } => {
            let frame = crate::layout::violin::compute_frame(spec, m);
            let value_axis = if *horizontal {
                &spec.x_axis
            } else {
                &spec.y_axis
            };
            let value_model = value_axis_model(value_axis, &frame.ticks, &[]);
            Some((
                category_axis(&spec.categories),
                value_model,
                frame.ticks.ticks.len(),
            ))
        }
        _ => None,
    }
}

/// IR + layout から完全な意味モデルを構築する。直交チャート(縦棒・横棒・線・
/// mixed・scatter・bubble)に軸を載せる。
pub fn build_model(spec: &ChartSpec, m: &TextMeasurer) -> ChartModel {
    let mut model = build_model_core(spec);
    (model.meta.width, model.meta.height) = model_dimensions(spec, m);
    if let Some((x, y, y_ticks)) = compute_axes(spec, m) {
        if x.kind == "temporal" {
            model.counts.x_ticks = x.ticks.as_ref().map_or(0, Vec::len);
        }
        model.counts.y_ticks = y_ticks;
        model.axes = Some(Axes { x, y });
    }
    model.geometry = compute_geometry(spec, m);
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT as DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::frontend::vegalite;
    use crate::ir::Color;
    use crate::text::TextMeasurer;

    #[test]
    fn violin_model_normalizes_axes_and_reports_category_slots() {
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        for chart_type in ["violin", "horizontalViolin"] {
            let value_scale = if chart_type == "violin" { "y" } else { "x" };
            let json = format!(
                r#"{{"type":"{chart_type}","data":{{"labels":["A","B"],"datasets":[{{"data":[[-10,null,20],[3,5]],"backgroundColor":["red","blue"]}}]}},"options":{{"scales":{{"{value_scale}":{{"min":-12,"max":22}}}}}}}}"#
            );
            let spec = chartjs::parse(&json, true).unwrap();
            let model = build_model(&spec, &measurer);

            assert_eq!(model.meta.r#type, chart_type);
            let axes = model.axes.expect("violin exposes normalized axes");
            assert_eq!(axes.x.kind, "category");
            assert_eq!(
                axes.x.labels.as_deref(),
                Some(["A".to_string(), "B".to_string()].as_slice())
            );
            assert_eq!(axes.y.kind, "linear");
            assert_eq!((axes.y.min, axes.y.max), (Some(-12.0), Some(22.0)));
            assert_eq!(
                model.series[0].fill.len(),
                2,
                "one color entry per category slot"
            );

            let auto_json = format!(
                r#"{{"type":"{chart_type}","data":{{"labels":["A","B"],"datasets":[{{"data":[[-10,null,20],[3,5]]}}]}}}}"#
            );
            let auto_spec = chartjs::parse(&auto_json, true).unwrap();
            let auto_model = build_model(&auto_spec, &measurer);
            let auto_y = auto_model.axes.expect("violin exposes value axis").y;
            assert!(
                auto_y.min.unwrap() <= -10.0,
                "sample minimum is covered: {auto_y:?}"
            );
            assert!(
                auto_y.max.unwrap() >= 20.0,
                "sample maximum is covered: {auto_y:?}"
            );
        }
    }

    #[test]
    fn horizontal_model_axis_preserves_hard_min_max_after_nice_ticks() {
        let json = r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[20,80]}]},
            "options":{"indexAxis":"y","scales":{"x":{"min":13,"max":87}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let model = build_model(&spec, &measurer);
        let axes = model.axes.expect("horizontal bar exposes normalized axes");

        assert_eq!((axes.y.min, axes.y.max), (Some(13.0), Some(87.0)));
    }

    #[test]
    fn scatter_model_axes_preserve_hard_min_max_after_nice_ticks() {
        let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":100,"y":200}]}]},
            "options":{"scales":{"x":{"min":13,"max":87},"y":{"min":25,"max":175}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let model = build_model(&spec, &measurer);
        let axes = model.axes.expect("scatter exposes x/y axes");

        assert_eq!((axes.x.min, axes.x.max), (Some(13.0), Some(87.0)));
        assert_eq!((axes.y.min, axes.y.max), (Some(25.0), Some(175.0)));
    }

    #[test]
    fn hard_bounds_keep_vertical_bar_geometry_inside_plot_area() {
        let json = r#"{"type":"bar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,50,100]}]},
            "options":{"scales":{"y":{"min":13,"max":87}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let geometry = build_model(&spec, &measurer)
            .geometry
            .expect("bar geometry");

        for bar in geometry.elements {
            assert!(bar.ny >= 0.0, "bar top escaped plot: {bar:?}");
            assert!(bar.ny + bar.nh <= 1.0, "bar bottom escaped plot: {bar:?}");
        }
    }

    #[test]
    fn hard_bounds_exclude_out_of_range_scatter_geometry() {
        let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":50},{"x":50,"y":50},{"x":100,"y":50}]}]},
            "options":{"scales":{"x":{"min":13,"max":87},"y":{"min":13,"max":87}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let geometry = build_model(&spec, &measurer)
            .geometry
            .expect("scatter geometry");

        assert_eq!(geometry.elements.len(), 1);
        assert_eq!(geometry.elements[0].index, 1);
        assert!((0.0..=1.0).contains(&geometry.elements[0].nx));
        assert!((0.0..=1.0).contains(&geometry.elements[0].ny));
    }

    #[test]
    fn hard_bounds_exclude_out_of_range_line_geometry() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,50,100]}]},
            "options":{"scales":{"y":{"min":13,"max":87}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let geometry = build_model(&spec, &measurer)
            .geometry
            .expect("line geometry");

        assert_eq!(geometry.elements.len(), 1);
        assert_eq!(geometry.elements[0].index, 1);
        assert!((0.0..=1.0).contains(&geometry.elements[0].ny));
    }

    #[test]
    fn temporal_axis_reports_only_uniform_step() {
        const DAY: i64 = 86_400_000;

        fn ticks(values: &[i64]) -> Vec<TemporalTick> {
            values
                .iter()
                .map(|&unix_millis| TemporalTick {
                    unix_millis,
                    label: unix_millis.to_string(),
                })
                .collect()
        }

        let fixed = ticks(&[0, DAY, 2 * DAY]);
        assert_eq!(temporal_axis(&[], &fixed).step, Some(DAY as f64));

        let calendar = ticks(&[0, 31 * DAY, (31 + 28) * DAY]);
        assert_eq!(temporal_axis(&[], &calendar).step, None);

        assert_eq!(temporal_axis(&[], &ticks(&[])).step, None);
        assert_eq!(temporal_axis(&[], &ticks(&[0])).step, None);
        assert_eq!(temporal_axis(&[], &ticks(&[0, DAY])).step, Some(DAY as f64));
        assert_eq!(
            temporal_axis(&[], &ticks(&[2 * DAY, DAY, 0])).step,
            Some(-(DAY as f64))
        );
    }

    #[test]
    fn scatter_model_exposes_temporal_x_and_y_axes() {
        let json = r#"{"type":"scatter","data":{"datasets":[{"data":[
            {"x":"1970-01-01","y":"1970-01-01"},
            {"x":"1970-01-02","y":"1970-01-02"},
            {"x":"1970-01-05","y":"1970-01-05"}]}]},
            "options":{"scales":{"x":{"type":"time"},"y":{"type":"timeseries"}}}}"#;
        let spec = chartjs::parse(json, true).unwrap();
        let model = build_model(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let axes = model.axes.expect("scatter exposes both axes");

        assert_eq!(axes.x.kind, "temporal");
        assert_eq!(axes.y.kind, "temporal");
        assert_eq!(axes.x.min, Some(0.0));
        assert_eq!(axes.x.max, Some(4.0 * 86_400_000.0));
        assert!(!axes.x.ticks.as_ref().unwrap().is_empty());
        assert!(!axes.y.labels.as_ref().unwrap().is_empty());
    }

    #[test]
    fn rgba_opaque_uses_1() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 1.0,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,1)");
    }

    #[test]
    fn rgba_half_alpha() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 0.5,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,0.5)");
    }

    #[test]
    fn rgba_transparent_uses_0() {
        let c = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
        assert_eq!(rgba_string(&c), "rgba(0,0,0,0)");
    }

    #[test]
    fn rgba_trims_trailing_zeros() {
        let c = Color {
            r: 1,
            g: 2,
            b: 3,
            a: 0.25,
        };
        assert_eq!(rgba_string(&c), "rgba(1,2,3,0.25)");
    }

    #[test]
    fn builds_meta_series_counts_for_bar() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"label":"売上","data":[120,200,150]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.meta.r#type, "bar");
        assert_eq!(model.series.len(), 1);
        assert_eq!(model.series[0].label, "売上");
        // 既定パレット先頭 #36A2EB、fill alpha=0.5 / stroke alpha=1.0(chart.js v4)
        assert_eq!(
            model.series[0].fill,
            vec!["rgba(54,162,235,0.5)".to_string()]
        );
        assert_eq!(
            model.series[0].stroke,
            vec!["rgba(54,162,235,1)".to_string()]
        );
        assert_eq!(
            model.series[0].values,
            vec![Some(120.0), Some(200.0), Some(150.0)]
        );
        assert_eq!(model.counts.datasets, 1);
        assert_eq!(model.counts.x_ticks, 3);
    }

    #[test]
    fn nan_series_values_serialize_as_null() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3]}]}}"#;
        let spec = crate::frontend::chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.series[0].values, vec![Some(1.0), None, Some(3.0)]);
        // JSON dump must succeed and produce null tokens
        let s = serde_json::to_string(&model).unwrap();
        assert!(
            s.contains("\"values\":[1.0,null,3.0]"),
            "values should serialize with null: {s}"
        );
    }

    #[test]
    fn pie_emits_per_slice_fill() {
        let json = r##"{"type":"pie","data":{"labels":["a","b","c"],
          "datasets":[{"data":[1,2,3],
          "backgroundColor":["#ff0000","#00ff00","#0000ff"]}]}}"##;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.series[0].fill.len(), 3);
        assert_eq!(model.series[0].fill[0], "rgba(255,0,0,1)");
    }

    #[test]
    fn bar_has_linear_y_and_category_x() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"data":[0,100,50]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("bar には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
        assert_eq!(axes.y.min, Some(0.0));
        assert_eq!(axes.x.kind, "category");
        assert_eq!(
            axes.x.labels.as_deref(),
            Some(&["1月".to_string(), "2月".to_string(), "3月".to_string()][..])
        );
        // y_ticks は目盛り数に同期
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn pie_has_no_axes() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.axes.is_none());
    }

    #[test]
    fn horizontal_bar_puts_value_axis_on_y() {
        // 横棒でも値(線形)軸は y に、カテゴリは x に載る(JS 抽出器の規約に揃える)。
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
          "datasets":[{"data":[10,90]}]},"options":{"indexAxis":"y"}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("横棒には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
        assert_eq!(axes.y.min, Some(0.0));
        assert_eq!(axes.x.kind, "category");
        assert!(model.counts.y_ticks > 0);
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn vertical_bar_reports_logarithmic_y_axis_without_leaking_step_sentinel() {
        // beginAtZero:false を明示: 最小値 1 はちょうど decade 境界(10^0)なので、
        // 既定の beginAtZero:true のままだとドメインが1桁広がり(0.1 まで)、
        // このテストの主眼(introspection API の kind/step)から逸れてしまう。
        let json = r#"{"type":"bar","data":{"labels":["a","b","c"],
          "datasets":[{"data":[1,10,100]}]},
          "options":{"scales":{"y":{"type":"logarithmic","beginAtZero":false}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);

        // JSON シリアライズも通り、step キーは省略され kind は正しい文字列。
        let s = serde_json::to_string(&model).unwrap();
        assert!(s.contains("\"kind\":\"logarithmic\""), "s={s}");
        let axes_json = serde_json::to_value(&model.axes).unwrap();
        assert!(
            axes_json["y"].get("step").is_none(),
            "step is None なので skip_serializing_if で省略されるべき: {axes_json}"
        );

        let axes = model.axes.expect("bar には軸があるべき");
        assert_eq!(axes.y.kind, "logarithmic");
        assert_eq!(
            axes.y.step, None,
            "対数軸は内部 0.0 番兵を漏らさず None にする"
        );
        assert_eq!(axes.y.min, Some(1.0));
        assert!(axes.y.max.unwrap() >= 100.0);
        let ticks = axes
            .y
            .ticks
            .as_ref()
            .expect("対数軸にも major ticks はある");
        assert!(!ticks.is_empty());
        assert_eq!(axes.x.kind, "category");
        assert_eq!(model.counts.y_ticks, ticks.len());
    }

    #[test]
    fn line_reports_logarithmic_y_axis() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
          "datasets":[{"data":[1,10,100]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("line には軸があるべき");
        assert_eq!(axes.y.kind, "logarithmic");
        assert_eq!(axes.y.step, None);
    }

    #[test]
    fn horizontal_bar_reports_logarithmic_value_axis_on_y() {
        // 横棒の値軸は x_axis.scale_kind から読むが、出力上は y に載る規約は
        // 線形時と同じ(horizontal_bar_puts_value_axis_on_y 参照)。
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
          "datasets":[{"data":[1,1000]}]},
          "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);

        let s = serde_json::to_string(&model).unwrap();
        assert!(s.contains("\"kind\":\"logarithmic\""), "s={s}");

        let axes = model.axes.expect("横棒には軸があるべき");
        assert_eq!(axes.y.kind, "logarithmic");
        assert_eq!(axes.y.step, None);
        assert_eq!(axes.x.kind, "category");
        assert!(model.counts.y_ticks > 0);
        assert_eq!(model.counts.y_ticks, axes.y.ticks.as_ref().unwrap().len());
    }

    #[test]
    fn horizontal_bar_reports_temporal_index_and_value_axes() {
        let json = r#"{"type":"bar","data":{"labels":["1970-01-01","1970-01-02","1970-01-05"],
          "datasets":[{"data":["1970-01-02","1970-01-03","1970-01-05"]}]},
          "options":{"indexAxis":"y","scales":{
            "x":{"type":"time","min":0,"max":345600000,"time":{"unit":"day","displayFormats":{"day":"%Y-%m-%d"}}},
            "y":{"type":"timeseries","time":{"unit":"day","displayFormats":{"day":"%Y-%m-%d"}}}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("横棒の temporal 軸が model に必要");

        assert_eq!(axes.x.kind, "temporal");
        assert_eq!(axes.y.kind, "temporal");
        assert!(
            axes.x
                .labels
                .as_ref()
                .unwrap()
                .iter()
                .any(|label| label == "1970-01-01")
        );
        assert!(
            axes.y
                .labels
                .as_ref()
                .unwrap()
                .iter()
                .any(|label| label == "1970-01-02")
        );
        assert_eq!(model.counts.y_ticks, axes.y.ticks.as_ref().unwrap().len());
    }

    #[test]
    fn horizontal_temporal_bar_exposes_normalized_geometry() {
        let json = r#"{"type":"bar","data":{"labels":["1970-01-01","1970-01-02","1970-01-05"],
          "datasets":[{"data":["1970-01-02","1970-01-03","1970-01-05"]}]},
          "options":{"indexAxis":"y","scales":{
            "x":{"type":"time","min":0,"max":345600000,"time":{"unit":"day"}},
            "y":{"type":"time","time":{"unit":"day"}}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let geometry = model
            .geometry
            .expect("temporal horizontal bars expose geometry");

        assert_eq!(geometry.elements.len(), 3);
        let centers = geometry
            .elements
            .iter()
            .map(|element| element.ny + element.nh / 2.0)
            .collect::<Vec<_>>();
        assert!((centers[1] - centers[0] - 0.2).abs() < 0.01);
        assert!((centers[2] - centers[1] - 0.6).abs() < 0.01);
        assert!(geometry.elements.iter().all(|element| {
            element.nx >= 0.0
                && element.nx <= 1.0
                && element.ny >= 0.0
                && element.ny <= 1.0
                && element.nw > 0.0
                && element.nw <= 1.0
                && element.nh > 0.0
                && element.nh <= 1.0
        }));
    }

    #[test]
    fn scatter_and_bubble_report_logarithmic_axes_but_boxplot_does_not() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let scatter_json = r#"{"type":"scatter","data":{"datasets":[{"data":[
          {"x":1,"y":2},{"x":3,"y":8}]}]},
          "options":{"scales":{"x":{"type":"logarithmic"},"y":{"type":"logarithmic"}}}}"#;
        let spec = chartjs::parse(scatter_json, false).unwrap();
        assert_eq!(spec.x_axis.scale_kind, crate::ir::ScaleKind::Logarithmic);
        assert_eq!(spec.y_axis.scale_kind, crate::ir::ScaleKind::Logarithmic);
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("scatter には軸があるべき");
        assert_eq!(axes.x.kind, "logarithmic");
        assert_eq!(axes.y.kind, "logarithmic");
        assert_eq!(axes.x.step, None);
        assert_eq!(axes.y.step, None);

        let bubble_json = r#"{"type":"bubble","data":{"datasets":[
          {"data":[{"x":1,"y":2,"r":10}]}]},
          "options":{"scales":{"x":{"type":"logarithmic"},"y":{"type":"logarithmic"}}}}"#;
        let spec = chartjs::parse(bubble_json, false).unwrap();
        assert_eq!(spec.x_axis.scale_kind, crate::ir::ScaleKind::Logarithmic);
        assert_eq!(spec.y_axis.scale_kind, crate::ir::ScaleKind::Logarithmic);
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("bubble には軸があるべき");
        assert_eq!(axes.x.kind, "logarithmic");
        assert_eq!(axes.y.kind, "logarithmic");

        let boxplot_json = r#"{"type":"boxplot","data":{"labels":["a"],
          "datasets":[{"data":[[1,2,3,4,5]]}]},
          "options":{"scales":{"y":{"type":"logarithmic"}}}}"#;
        let spec = chartjs::parse(boxplot_json, false).unwrap();
        assert_eq!(spec.y_axis.scale_kind, crate::ir::ScaleKind::Linear);
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("boxplot には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
    }

    #[test]
    fn scatter_has_linear_x_and_y_axes() {
        let json = r#"{"type":"scatter","data":{"datasets":[{"data":[
          {"x":1,"y":2},{"x":3,"y":8}]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("scatter には軸があるべき");
        assert_eq!(axes.x.kind, "linear");
        assert_eq!(axes.y.kind, "linear");
        assert!(model.counts.y_ticks > 0);
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn pie_stroke_claims_rendered_white() {
        // renderer は borderColor を無視し白でスライス境界を描くので、モデルも白を主張する。
        let json = r##"{"type":"pie","data":{"labels":["a","b"],
          "datasets":[{"data":[1,2],"borderColor":"#ff0000"}]}}"##;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(
            model.series[0].stroke,
            vec!["rgba(255,255,255,1)".to_string()]
        );
    }

    #[test]
    fn empty_dataset_emits_no_element_colors() {
        // 空データセットは描画マークが無いため fill/stroke とも空(chart.js と一致)。
        let json = r#"{"type":"bar","data":{"labels":[],"datasets":[{"data":[]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert!(model.series[0].fill.is_empty());
        assert!(model.series[0].stroke.is_empty());
    }

    #[test]
    fn bar_has_normalized_geometry() {
        let json = r#"{"type":"bar","data":{"labels":["A","B","C"],
          "datasets":[{"data":[10,20,30]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let g = model.geometry.expect("縦棒には geometry があるべき");
        // plot_area はキャンバス [0,1] 内、要素はプロット領域 [0,1] 内。
        assert!(g.plot_area.x > 0.0 && g.plot_area.x < 1.0);
        assert!(g.plot_area.w > 0.0 && g.plot_area.w <= 1.0);
        assert_eq!(g.elements.len(), 3);
        for e in &g.elements {
            assert_eq!(e.kind, "bar");
            assert!(e.nx >= 0.0 && e.nx <= 1.0, "nx={}", e.nx);
            assert!(e.nw > 0.0 && e.nw <= 1.0, "nw={}", e.nw);
            assert!(e.nh >= 0.0 && e.nh <= 1.0, "nh={}", e.nh);
        }
        // 左→右にカテゴリが並ぶ。
        assert!(g.elements[0].nx < g.elements[1].nx);
        assert!(g.elements[1].nx < g.elements[2].nx);
        // 値が大きいほど高い。
        assert!(g.elements[2].nh > g.elements[0].nh);
    }

    #[test]
    fn pie_has_no_geometry() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.geometry.is_none());
    }

    #[test]
    fn horizontal_bar_has_no_geometry_yet() {
        // 横棒は今回スコープ外: geometry=None。
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
          "datasets":[{"data":[10,90]}]},"options":{"indexAxis":"y"}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.geometry.is_none());
    }

    #[test]
    fn scatter_has_normalized_geometry() {
        let json = r#"{"type":"scatter","data":{"datasets":[
          {"data":[{"x":1,"y":2},{"x":3,"y":4},{"x":5,"y":6}]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let g = model.geometry.expect("scatter には geometry があるべき");
        assert_eq!(g.elements.len(), 3);
        for e in &g.elements {
            assert_eq!(e.kind, "scatter");
            assert_eq!(e.nw, 0.0);
            assert_eq!(e.nh, 0.0);
            assert!(e.nx >= 0.0 && e.nx <= 1.0, "nx={}", e.nx);
            assert!(e.ny >= 0.0 && e.ny <= 1.0, "ny={}", e.ny);
        }
        assert!(g.elements[0].nx < g.elements[1].nx);
        assert!(g.elements[1].nx < g.elements[2].nx);
    }

    #[test]
    fn bubble_has_normalized_geometry_with_radius() {
        let json = r#"{"type":"bubble","data":{"datasets":[
          {"data":[{"x":1,"y":2,"r":10},{"x":3,"y":4,"r":20}]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let g = model.geometry.expect("bubble には geometry があるべき");
        assert_eq!(g.elements.len(), 2);
        for e in &g.elements {
            assert_eq!(e.kind, "bubble");
            assert!(e.nw > 0.0, "bubble の nw(正規化半径)は正: nw={}", e.nw);
        }
        assert!(g.elements[1].nw > g.elements[0].nw, "大きい r は大きい nw");
    }

    #[test]
    fn square_has_normalized_geometry_and_linear_axes() {
        let json = r#"{
          "mark":"square",
          "data":{"values":[{"x":1,"y":2},{"x":3,"y":4}]},
          "encoding":{"x":{"field":"x"},"y":{"field":"y"}}
        }"#;
        let spec = vegalite::parse(json, true).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);

        assert_eq!(model.meta.r#type, "square");
        let axes = model.axes.expect("square exposes numeric axes");
        assert_eq!(axes.x.kind, "linear");
        assert_eq!(axes.y.kind, "linear");

        let geometry = model.geometry.expect("square exposes point geometry");
        assert_eq!(geometry.elements.len(), 2);
        for element in &geometry.elements {
            assert_eq!(element.kind, "square");
            assert!(element.nw > 0.0);
            assert!(element.nh > 0.0);
        }

        let layout = crate::layout::scatter::compute_scatter_layout(&spec, &m);
        let plot_width = layout.plot_right - layout.plot_left;
        let plot_height = layout.plot_bottom - layout.plot_top;
        for element in &geometry.elements {
            let width = element.nw * plot_width;
            let height = element.nh * plot_height;
            assert!((width - height).abs() < 1e-10, "marker is not square");
        }
    }

    #[test]
    fn line_has_normalized_geometry() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
          "datasets":[{"data":[10,20,30]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let g = model.geometry.expect("line には geometry があるべき");
        assert_eq!(g.elements.len(), 3);
        for e in &g.elements {
            assert_eq!(e.kind, "line");
            assert_eq!(e.nw, 0.0);
            assert_eq!(e.nh, 0.0);
        }
        assert!(g.elements[0].nx < g.elements[1].nx);
        assert!(
            g.elements[2].ny < g.elements[0].ny,
            "大きい値は小さい ny(上方向)"
        );
    }

    #[test]
    fn plot_area_categorical_line_model_matches_renderer_size() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
          "datasets":[{"data":[10,20,30]}]}}"#;
        let mut spec = chartjs::parse(json, false).unwrap();
        spec.size_mode = crate::ir::SizeMode::PlotArea;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = crate::layout::common::compute(&spec, &m);
        let scene = crate::layout::build_scene(&spec, &m);
        let model = build_model(&spec, &m);

        assert_eq!(
            (model.meta.width, model.meta.height),
            (scene.width, scene.height)
        );
        let geometry = model.geometry.expect("line geometry");
        assert_eq!(
            geometry.plot_area,
            RectN {
                x: frame.plot_left / scene.width,
                y: frame.plot_top / scene.height,
                w: (frame.plot_right - frame.plot_left) / scene.width,
                h: (frame.plot_bottom - frame.plot_top) / scene.height,
            }
        );
    }

    #[test]
    fn plot_area_non_cartesian_model_keeps_requested_canvas_size() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let mut spec = chartjs::parse(json, false).unwrap();
        spec.size_mode = crate::ir::SizeMode::PlotArea;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);

        assert_eq!((model.meta.width, model.meta.height), (800.0, 450.0));
    }

    fn assert_plot_area_legacy_scene_model_dimensions(json: &str) {
        let mut spec = chartjs::parse(json, false).unwrap();
        spec.size_mode = crate::ir::SizeMode::PlotArea;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = crate::layout::build_scene(&spec, &m);
        let model = build_model(&spec, &m);

        assert_eq!((scene.width, scene.height), (spec.width, spec.height));
        assert_eq!(
            (model.meta.width, model.meta.height),
            (scene.width, scene.height)
        );
        if let Some(geometry) = model.geometry {
            let frame = crate::layout::common::compute(&spec, &m);
            assert_eq!(
                geometry.plot_area,
                RectN {
                    x: frame.plot_left / spec.width,
                    y: frame.plot_top / spec.height,
                    w: (frame.plot_right - frame.plot_left) / spec.width,
                    h: (frame.plot_bottom - frame.plot_top) / spec.height,
                }
            );
        }
    }

    #[test]
    fn plot_area_vertical_bar_model_matches_legacy_renderer_size() {
        assert_plot_area_legacy_scene_model_dimensions(
            r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#,
        );
    }

    #[test]
    fn plot_area_horizontal_bar_model_matches_legacy_renderer_size() {
        assert_plot_area_legacy_scene_model_dimensions(
            r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},"options":{"indexAxis":"y"}}"#,
        );
    }

    #[test]
    fn plot_area_mixed_model_matches_legacy_renderer_size() {
        assert_plot_area_legacy_scene_model_dimensions(
            r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]},{"type":"line","data":[2,1]}]}}"#,
        );
    }

    /// クロス言語フィクスチャを Rust `rgba_string` と JS `fmtAlpha` の両方で使い、
    /// 両実装の乖離をどちらか一方のテストで必ず捕捉する。
    #[test]
    fn rgba_string_matches_cross_language_fixture() {
        let rows: Vec<(u8, u8, u8, f32, String)> = serde_json::from_str(include_str!(
            "../../../tools/chartjs-compat/rgba-fixture.json"
        ))
        .expect("valid shared RGBA fixture");
        for (r, g, b, a, expected) in rows {
            let c = Color { r, g, b, a };
            assert_eq!(rgba_string(&c), expected, "row r={r} g={g} b={b} a={a}");
        }
    }
}
