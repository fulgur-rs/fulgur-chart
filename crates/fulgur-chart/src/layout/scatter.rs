//! scatter チャート: 数値 x/y 軸に点を描く。
//! カテゴリ系の `common::compute` は x をカテゴリ前提にするため、ここでは
//! scatter 固有のフレームを自前で組む。共有できる凡例/定数/テーマは `common` を再利用する。

use super::common::{
    AXIS_TITLE_BAND, OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND,
    X_LABEL_CENTER_RATIO, draw_horizontal_legend, draw_vertical_legend_styled,
    legend_band_width_vertical_styled, legend_horizontal_band_height, legend_label_font_size,
};
use crate::ir::{
    AxisSpec, AxisTitleAlign, ChartKind, ChartSpec, Color, DatasetPointStyle, LegendPos, Point,
    ScaleKind,
};
use crate::scale::{LinearScale, NiceTicks, ValueScale};
use crate::scene::{Anchor, Prim, Scene};
use crate::temporal::{TemporalScale, TemporalTick};
use crate::text::TextMeasurer;

/// scatter のマーカー既定半径。chart.js scatter の pointRadius 既定値 ~3.0。
const DEFAULT_POINT_R: f64 = 3.0;

/// Vega-Lite's default square size is 30px²; `PointBox.r` stores half the side.
const DEFAULT_SQUARE_HALF_SIDE: f64 = 2.738_612_787_525_830_6;

/// bubble で `point.r` が無い場合の既定半径。bubble は通常 r を持つが保険。
const DEFAULT_BUBBLE_R: f64 = 5.0;

/// 単一データ点の画素空間情報（scatter/line/bubble/square 共用）。
/// モデル geometry とレンダラが共有する単一の真実源。
#[derive(Debug, Clone, PartialEq)]
pub struct PointBox {
    pub series: usize,
    pub index: usize,
    pub kind: &'static str, // "scatter" | "line" | "bubble"
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

/// scatter/bubble の自前フレーム（`common::compute` はカテゴリ軸前提のため使わない）。
#[derive(Debug, Clone)]
pub struct ScatterLayout {
    pub xs: ValueScale,
    pub ys: ValueScale,
    pub x_ticks: NiceTicks,
    pub y_ticks: NiceTicks,
    /// 対数軸のラベルなし minor 目盛。線形軸では空。
    pub x_minor_ticks: Vec<f64>,
    pub y_minor_ticks: Vec<f64>,
    pub x_temporal_ticks: Vec<TemporalTick>,
    pub y_temporal_ticks: Vec<TemporalTick>,
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
}

/// scatter/bubble/square チャートのフレームを計算して返す。
/// `build` のインライン計算と同一の式（単一の真実源）。
pub fn compute_scatter_layout(spec: &ChartSpec, m: &TextMeasurer) -> ScatterLayout {
    let label_font = spec.theme.font_size;
    let (xmin, xmax) = axis_domain(spec, &spec.x_axis, |p| p.x);
    let (ymin, ymax) = axis_domain(spec, &spec.y_axis, |p| p.y);
    let x_values = axis_values(spec, |point| point.x);
    let y_values = axis_values(spec, |point| point.y);
    let (x_ticks, x_minor_ticks, x_temporal_ticks) =
        axis_ticks(&spec.x_axis, xmin, xmax, spec.width);
    let (y_ticks, y_minor_ticks, y_temporal_ticks) =
        axis_ticks(&spec.y_axis, ymin, ymax, spec.height);
    let mut max_y_w = 0.0_f32;
    for (index, &t) in y_ticks.ticks.iter().enumerate() {
        let label =
            super::common::axis_temporal_tick_label(&spec.y_axis, &y_temporal_ticks, index, t);
        let w = m.width(&label, label_font as f32);
        if w > max_y_w {
            max_y_w = w;
        }
    }
    // Y 軸タイトル(回転テキスト)の帯幅。text 幅(font_size)+ ベースラインギャップ(6px)。
    // title=None(既定)なら 0.0 で、既存レイアウトは変わらない。
    let y_title_w = spec
        .y_axis
        .title
        .as_ref()
        .map(|t| t.font_size.unwrap_or(spec.theme.font_size * 1.1) + 6.0)
        .unwrap_or(0.0);
    let y_axis_w = max_y_w as f64 + 10.0 + y_title_w;
    let legend = has_legend(spec);
    let legend_title = super::common::legend_title(spec);
    let legend_font = legend_label_font_size(&spec.legend_options, label_font);
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_height =
        legend_horizontal_band_height(&spec.legend_options, label_font, legend_title.is_some());
    let legend_top = if legend && spec.legend == LegendPos::Top {
        legend_height
    } else {
        0.0
    };
    let legend_bottom = if legend && spec.legend == LegendPos::Bottom {
        legend_height
    } else {
        0.0
    };
    // series_names の割り当ては凡例が左右にあるときだけ必要なため遅延評価する。
    let (legend_left, legend_right_w) = if legend
        && (spec.legend == LegendPos::Left || spec.legend == LegendPos::Right)
    {
        let mut series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
        series_names.extend(legend_title.map(str::to_owned));
        let w =
            legend_band_width_vertical_styled(m, &series_names, legend_font, &spec.legend_options);
        if spec.legend == LegendPos::Left {
            (w, 0.0)
        } else {
            (0.0, w)
        }
    } else {
        (0.0, 0.0)
    };
    // X 軸タイトルがあれば、x ラベル帯の下側にさらにタイトル帯を確保して plot_bottom を上へ押し上げる。
    // title=None(既定)なら 0.0 で、既存レイアウトは変わらない。
    let x_title_h = if spec.x_axis.title.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    };
    let plot_left = OUTER_PAD + y_axis_w + legend_left;
    let plot_right = spec.width - OUTER_PAD - legend_right_w;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;
    ScatterLayout {
        xs: axis_scale(&spec.x_axis, &x_ticks, &x_values, plot_left, plot_right),
        ys: axis_scale(&spec.y_axis, &y_ticks, &y_values, plot_bottom, plot_top),
        x_ticks,
        y_ticks,
        x_minor_ticks,
        y_minor_ticks,
        x_temporal_ticks,
        y_temporal_ticks,
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
    }
}

fn axis_ticks(
    axis: &AxisSpec,
    data_min: f64,
    data_max: f64,
    pixel_extent: f64,
) -> (NiceTicks, Vec<f64>, Vec<TemporalTick>) {
    if axis.scale_kind == ScaleKind::Logarithmic {
        let (ticks, minor_ticks) = crate::scale::log_axis_ticks(data_min, data_max);
        (ticks, minor_ticks, Vec::new())
    } else if super::common::is_temporal_scale(axis) {
        let temporal_ticks = super::common::temporal_axis_ticks(
            axis,
            data_min as i64,
            data_max as i64,
            pixel_extent,
        );
        (
            NiceTicks {
                min: data_min,
                max: data_max,
                step: 0.0,
                ticks: temporal_ticks
                    .iter()
                    .map(|tick| tick.unix_millis as f64)
                    .collect(),
            },
            Vec::new(),
            temporal_ticks,
        )
    } else {
        (
            super::common::configured_axis_ticks(data_min, data_max, axis),
            Vec::new(),
            Vec::new(),
        )
    }
}

fn axis_values(spec: &ChartSpec, select: impl Fn(&Point) -> f64) -> Vec<i64> {
    spec.series
        .iter()
        .flat_map(|series| &series.points)
        .map(select)
        .filter(|value| value.is_finite() && value.abs() <= 8.64e15)
        .map(|value| value.trunc() as i64)
        .collect()
}

fn axis_scale(
    axis: &AxisSpec,
    ticks: &NiceTicks,
    values: &[i64],
    pixel_min: f64,
    pixel_max: f64,
) -> ValueScale {
    if axis.scale_kind == ScaleKind::Logarithmic {
        ValueScale::Log {
            inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), pixel_min, pixel_max),
            floor: ticks.min,
        }
    } else if super::common::is_temporal_scale(axis) {
        ValueScale::Temporal(TemporalScale::with_domain(
            axis.scale_kind,
            values,
            ticks.min as i64,
            ticks.max as i64,
            pixel_min,
            pixel_max,
        ))
    } else {
        ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, pixel_min, pixel_max))
    }
}

fn map_scatter_line_axis(scale: &ValueScale, value: f64) -> Option<f64> {
    let pixel = match scale {
        ValueScale::Linear(inner) => inner.map(value),
        ValueScale::Temporal(inner) => inner.map_value(value),
        ValueScale::Log { inner, .. } if value > 0.0 => inner.map(value.log10()),
        ValueScale::Log { .. } => return None,
    };
    pixel.is_finite().then_some(pixel)
}

/// Clip a pixel-space line segment to the scatter plot rectangle using Liang–Barsky.
fn clip_segment_to_plot(
    start: (f64, f64),
    end: (f64, f64),
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
) -> Option<((f64, f64), (f64, f64))> {
    if ![start.0, start.1, end.0, end.1, left, right, top, bottom]
        .iter()
        .all(|value| value.is_finite())
        || left > right
        || top > bottom
    {
        return None;
    }

    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }

    let mut entering = 0.0_f64;
    let mut leaving = 1.0_f64;
    for (p, q) in [
        (-dx, start.0 - left),
        (dx, right - start.0),
        (-dy, start.1 - top),
        (dy, bottom - start.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if !ratio.is_finite() {
            return None;
        }
        if p < 0.0 {
            if ratio > leaving {
                return None;
            }
            entering = entering.max(ratio);
        } else {
            if ratio < entering {
                return None;
            }
            leaving = leaving.min(ratio);
        }
    }

    if entering > leaving {
        return None;
    }
    Some((
        (start.0 + entering * dx, start.1 + entering * dy),
        (start.0 + leaving * dx, start.1 + leaving * dy),
    ))
}

fn finish_scatter_line_segment(segments: &mut Vec<Vec<(f64, f64)>>, current: &mut Vec<(f64, f64)>) {
    if current.len() >= 2 {
        segments.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

fn scatter_line_segments(points: &[Point], layout: &ScatterLayout) -> Vec<Vec<(f64, f64)>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    let mut previous = None;

    for point in points {
        let pixel = if point.x.is_finite() && point.y.is_finite() {
            map_scatter_line_axis(&layout.xs, point.x)
                .zip(map_scatter_line_axis(&layout.ys, point.y))
        } else {
            None
        };
        let Some(pixel) = pixel else {
            finish_scatter_line_segment(&mut segments, &mut current);
            previous = None;
            continue;
        };

        if let Some(previous_pixel) = previous {
            if let Some((clipped_start, clipped_end)) = clip_segment_to_plot(
                previous_pixel,
                pixel,
                layout.plot_left,
                layout.plot_right,
                layout.plot_top,
                layout.plot_bottom,
            ) {
                if current.last() == Some(&clipped_start) {
                    if current.last() != Some(&clipped_end) {
                        current.push(clipped_end);
                    }
                } else {
                    finish_scatter_line_segment(&mut segments, &mut current);
                    current.push(clipped_start);
                    if clipped_start != clipped_end {
                        current.push(clipped_end);
                    }
                }
            } else {
                finish_scatter_line_segment(&mut segments, &mut current);
            }
        }
        previous = Some(pixel);
    }
    finish_scatter_line_segment(&mut segments, &mut current);
    segments
}

/// scatter/bubble の全点を返す（renderer とモデルの単一の真実源）。
/// 非有限座標と hard axis domain の範囲外の点はスキップする。bubble は `PointBox.r` に
/// 実ピクセル半径、square は正方形の半辺長を格納する。
pub fn scatter_points(spec: &ChartSpec, layout: &ScatterLayout) -> Vec<PointBox> {
    let kind = match &spec.kind {
        ChartKind::Bubble => "bubble",
        ChartKind::Square => "square",
        _ => "scatter",
    };
    let mut pts = Vec::new();
    for (sidx, ser) in spec.series.iter().enumerate() {
        for (i, p) in ser.points.iter().enumerate() {
            if !p.x.is_finite() || !p.y.is_finite() {
                continue;
            }
            if !super::common::axis_value_in_bounds(p.x, &layout.x_ticks)
                || !super::common::axis_value_in_bounds(p.y, &layout.y_ticks)
            {
                continue;
            }
            pts.push(PointBox {
                series: sidx,
                index: i,
                kind,
                cx: layout.xs.map(p.x),
                cy: layout.ys.map(p.y),
                r: point_radius(&spec.kind, p, ser.point_radius),
            });
        }
    }
    pts
}

/// マーカーの半径/半辺長を返す。bubble はデータの第3次元 `point.r` を優先し、無ければ
/// dataset の `pointRadius`、それも無ければ既定値。square は `point.r` に面積から変換した
/// 半辺長を持つ。scatter は dataset の `pointRadius` を使い、無指定なら既定値。
/// 非有限/負の値は不正な SVG を避けるためそれぞれの既定値にフォールバックする。
fn point_radius(kind: &ChartKind, point: &Point, dataset_radius: Option<f64>) -> f64 {
    let valid = |r: f64, fallback: f64| {
        if r.is_finite() && r >= 0.0 {
            r
        } else {
            fallback
        }
    };
    match kind {
        ChartKind::Bubble => {
            let r = point.r.or(dataset_radius).unwrap_or(DEFAULT_BUBBLE_R);
            valid(r, DEFAULT_BUBBLE_R)
        }
        ChartKind::Square => {
            let half_side = point.r.unwrap_or(DEFAULT_SQUARE_HALF_SIDE);
            valid(half_side, DEFAULT_SQUARE_HALF_SIDE)
        }
        _ => valid(dataset_radius.unwrap_or(DEFAULT_POINT_R), DEFAULT_POINT_R),
    }
}

/// 凡例の有無(Top/Bottom/Left/Right かつ名前付き系列が 1 つ以上)。
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && (spec.series.iter().any(|s| !s.name.is_empty())
        || super::common::legend_title(spec).is_some())
}

/// 全系列の全点から 1 軸ぶんのドメインを求める。`select` で x/y を選ぶ。
/// 線形軸では非有限値を無視し、有限値が無ければ 0.0..1.0 にフォールバックする。
/// 対数軸では正値だけをデータ範囲に使い、ゼロの扱い・bounds・suggestions は
/// common の log domain 解決を共有する。
pub(crate) fn axis_domain(
    spec: &ChartSpec,
    axis_spec: &AxisSpec,
    select: impl Fn(&Point) -> f64,
) -> (f64, f64) {
    if axis_spec.scale_kind == ScaleKind::Logarithmic {
        let mut min_positive = f64::INFINITY;
        let mut max_positive = f64::NEG_INFINITY;
        let mut has_zero = false;
        for s in &spec.series {
            for p in &s.points {
                let value = select(p);
                if !value.is_finite() {
                    continue;
                }
                if value == 0.0 {
                    has_zero = true;
                } else if value > 0.0 {
                    min_positive = min_positive.min(value);
                    max_positive = max_positive.max(value);
                }
            }
        }
        return super::common::log_axis_domain_from_extrema(
            axis_spec,
            min_positive,
            max_positive,
            has_zero,
        );
    }

    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in &spec.series {
        for p in &s.points {
            let v = select(p);
            if v.is_finite()
                && (!super::common::is_temporal_scale(axis_spec)
                    || super::common::temporal_value_is_valid(v))
            {
                if v < lo {
                    lo = v;
                }
                if v > hi {
                    hi = v;
                }
            }
        }
    }
    if super::common::is_temporal_scale(axis_spec) {
        return super::common::resolve_temporal_domain(axis_spec, lo, hi);
    }
    super::common::resolve_axis_domain(axis_spec, lo, hi)
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let layout = compute_scatter_layout(spec, m);
    let xs = layout.xs.clone();
    let ys = layout.ys.clone();
    let plot_left = layout.plot_left;
    let plot_right = layout.plot_right;
    let plot_top = layout.plot_top;
    let plot_bottom = layout.plot_bottom;

    // グリッド描画用 ticks は compute_scatter_layout で計算済み。
    let x_ticks = &layout.x_ticks;
    let y_ticks = &layout.y_ticks;
    let x_minor_ticks = &layout.x_minor_ticks;
    let y_minor_ticks = &layout.y_minor_ticks;

    // 凡例描画用フラグ(フレーム計算ではなく表示用)。
    let legend = has_legend(spec);
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_title = super::common::legend_title(spec);
    let legend_height =
        legend_horizontal_band_height(&spec.legend_options, label_font, legend_title.is_some());
    let legend_right = if legend && spec.legend == LegendPos::Right {
        let mut series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
        series_names.extend(legend_title.map(str::to_owned));
        legend_band_width_vertical_styled(
            m,
            &series_names,
            legend_label_font_size(&spec.legend_options, label_font),
            &spec.legend_options,
        )
    } else {
        0.0
    };

    let mut items: Vec<Prim> = Vec::new();

    // 1. タイトル。
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // 2. 横グリッド + y 目盛りラベル(右寄せ)。y_axis.grid.display=false のときは
    // Prim::Line を落とすが、目盛りラベルは常に残す。
    let y_grid_cfg = &spec.y_axis.grid;
    let y_grid_color = y_grid_cfg.color.unwrap_or(spec.theme.grid_color);
    if y_grid_cfg.display {
        let minor_color = Color {
            a: y_grid_color.a * 0.5,
            ..y_grid_color
        };
        for &t in y_minor_ticks {
            let y = ys.map(t);
            items.push(Prim::Line {
                x1: plot_left,
                y1: y,
                x2: plot_right,
                y2: y,
                stroke: minor_color,
                stroke_width: y_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }
    for (index, &t) in y_ticks.ticks.iter().enumerate() {
        let y = ys.map(t);
        if y_grid_cfg.display {
            items.push(Prim::Line {
                x1: plot_left,
                y1: y,
                x2: plot_right,
                y2: y,
                stroke: y_grid_color,
                stroke_width: y_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: y + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: super::common::axis_temporal_tick_label(
                &spec.y_axis,
                &layout.y_temporal_ticks,
                index,
                t,
            ),
            rotate_deg: None,
        });
    }

    // 3. 縦グリッド + x 目盛りラベル(軸下に中央寄せ)。x_axis.grid.display=false のときは
    // Prim::Line を落とすが、目盛りラベルは常に残す。
    let x_grid_cfg = &spec.x_axis.grid;
    let x_grid_color = x_grid_cfg.color.unwrap_or(spec.theme.grid_color);
    if x_grid_cfg.display {
        let minor_color = Color {
            a: x_grid_color.a * 0.5,
            ..x_grid_color
        };
        for &t in x_minor_ticks {
            let x = xs.map(t);
            items.push(Prim::Line {
                x1: x,
                y1: plot_top,
                x2: x,
                y2: plot_bottom,
                stroke: minor_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }
    for (index, &t) in x_ticks.ticks.iter().enumerate() {
        let x = xs.map(t);
        if x_grid_cfg.display {
            items.push(Prim::Line {
                x1: x,
                y1: plot_top,
                x2: x,
                y2: plot_bottom,
                stroke: x_grid_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
        items.push(Prim::Text {
            x,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: super::common::axis_temporal_tick_label(
                &spec.x_axis,
                &layout.x_temporal_ticks,
                index,
                t,
            ),
            rotate_deg: None,
        });
    }

    // 4. 軸ベースライン(x 下辺 + y 左辺)。border.display/color/width/dash を反映。
    let x_border = &spec.x_axis.border;
    if x_border.display {
        let border_color = x_border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: plot_left,
            y1: plot_bottom,
            x2: plot_right,
            y2: plot_bottom,
            stroke: border_color,
            stroke_width: x_border.width,
            dash: x_border.dash.clone(),
        });
    }
    let y_border = &spec.y_axis.border;
    if y_border.display {
        let border_color = y_border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: plot_left,
            y1: plot_top,
            x2: plot_left,
            y2: plot_bottom,
            stroke: border_color,
            stroke_width: y_border.width,
            dash: y_border.dash.clone(),
        });
    }

    // 4b. tick 短線。y_axis/x_axis の grid.draw_ticks が true のとき、プロット外側へ短線を描く。
    // 色は grid.color を継承(既定 ink)、線幅は grid.line_width。Chart.js の既定に合わせた挙動。
    const TICK_LEN: f64 = 4.0;
    if y_grid_cfg.draw_ticks {
        let tick_color = y_grid_cfg.color.unwrap_or(ink);
        for &t in y_ticks.ticks.iter().chain(y_minor_ticks.iter()) {
            let y = ys.map(t);
            items.push(Prim::Line {
                x1: plot_left - TICK_LEN,
                y1: y,
                x2: plot_left,
                y2: y,
                stroke: tick_color,
                stroke_width: y_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }
    if x_grid_cfg.draw_ticks {
        let tick_color = x_grid_cfg.color.unwrap_or(ink);
        for &t in x_ticks.ticks.iter().chain(x_minor_ticks.iter()) {
            let x = xs.map(t);
            items.push(Prim::Line {
                x1: x,
                y1: plot_bottom,
                x2: x,
                y2: plot_bottom + TICK_LEN,
                stroke: tick_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }

    // 5. showLine=true の scatter dataset は入力順に点をつなぐ。
    for ser in &spec.series {
        let Some(line_style) = ser.line_style.as_ref() else {
            continue;
        };
        if !line_style.show_line {
            continue;
        }

        let segments = scatter_line_segments(&ser.points, &layout);
        for points in segments.into_iter().filter(|segment| segment.len() >= 2) {
            if line_style.border_dash.is_empty() {
                items.push(Prim::Polyline {
                    points,
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                items.push(Prim::StyledPolyline {
                    points,
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                    dash: line_style.border_dash.clone(),
                    dash_offset: line_style.border_dash_offset,
                });
            }
        }
    }

    // 6. 点。共有 scatter_points(単一真実源)から描画。
    for b in scatter_points(spec, &layout) {
        let ser = &spec.series[b.series];
        let point_style = if matches!(spec.kind, ChartKind::Square) {
            Some(DatasetPointStyle::Rect)
        } else {
            ser.line_style.as_ref().and_then(|style| style.point_style)
        };
        super::common::dataset_point_marker(
            &mut items,
            b.cx,
            b.cy,
            b.r,
            ser.fill_at(b.index),
            ser.stroke_at(b.index),
            ser.stroke_width,
            point_style,
        );
    }

    // 7. 凡例(Top/Bottom: 横並び。draw_frame と同じ配置)。
    if legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|series| (series.name.clone(), series.fill_at(0)))
            .collect();
        let legend_cy = if spec.legend == LegendPos::Top {
            OUTER_PAD + title_band + legend_height / 2.0
        } else {
            spec.height - OUTER_PAD - legend_height / 2.0
        };
        draw_horizontal_legend(
            &mut items,
            &entries,
            legend_title,
            spec.width,
            legend_cy,
            label_font,
            ink,
            m,
            &spec.legend_options,
        );
    }

    // 6b. 凡例(Left/Right: 縦並び)。
    if legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|s| (s.name.clone(), s.fill_at(0)))
            .collect();
        let band_x = if spec.legend == LegendPos::Left {
            OUTER_PAD
        } else {
            spec.width - OUTER_PAD - legend_right
        };
        draw_vertical_legend_styled(
            &mut items,
            &entries,
            legend_title,
            band_x,
            plot_top,
            plot_bottom,
            ink,
            label_font,
            &spec.legend_options,
        );
    }

    // 7. Y 軸タイトル(-90deg 回転)。common::draw_frame と同じアンカー幾何:
    //   Anchor::Start + -90deg → cy=plot_bottom(bottom-to-top 読みの起点)
    //   Anchor::End   + -90deg → cy=plot_top
    //   Anchor::Middle + -90deg → cy=中央
    if let Some(title) = &spec.y_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let cy_center = (plot_top + plot_bottom) / 2.0;
        let (cy, anchor) = match title.align {
            AxisTitleAlign::Start => (plot_bottom, Anchor::Start),
            AxisTitleAlign::End => (plot_top, Anchor::End),
            AxisTitleAlign::Center => (cy_center, Anchor::Middle),
        };
        let x = OUTER_PAD + font / 2.0;
        items.push(Prim::Text {
            x,
            y: cy,
            size: font,
            anchor,
            fill: color,
            content: title.text.clone(),
            rotate_deg: Some(-90.0),
        });
    }

    // 8. X 軸タイトル(水平)。x ラベル帯のさらに下側に描く。
    // Chart.js の x 軸は Start=left / End=right(Y 軸のような入れ替えは不要)。
    if let Some(title) = &spec.x_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let (cx, anchor) = match title.align {
            AxisTitleAlign::Start => (plot_left, Anchor::Start),
            AxisTitleAlign::End => (plot_right, Anchor::End),
            AxisTitleAlign::Center => ((plot_left + plot_right) / 2.0, Anchor::Middle),
        };
        let y = plot_bottom + X_LABEL_BAND + font * 0.9;
        items.push(Prim::Text {
            x: cx,
            y,
            size: font,
            anchor,
            fill: color,
            content: title.text.clone(),
            rotate_deg: None,
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT as DEFAULT_FONT;
    use crate::ir::{
        AxisBorder, AxisGrid, AxisSpec, AxisTitle, AxisTitleAlign, ChartKind, ChartSpec, Color,
        LegendPos, LineInterpolation, Point, ScaleKind, Series, SeriesType, SizeMode, TimeOptions,
        XPositions,
    };
    use crate::text::TextMeasurer;

    fn make_scatter_spec(points: &[(f64, f64)]) -> ChartSpec {
        let palette = crate::palette::PALETTE.to_vec();
        ChartSpec {
            kind: ChartKind::Scatter,
            categories: vec![],
            x_positions: XPositions::Category,
            y_positions: XPositions::Category,
            series: vec![Series {
                name: String::new(),
                values: vec![],
                points: points
                    .iter()
                    .map(|&(x, y)| Point { x, y, r: None })
                    .collect(),
                fill: vec![palette[0]],
                stroke: vec![],
                stroke_width: 1.0,
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
                violin_samples: vec![],
                box_points: vec![],
                tree: vec![],
                links: vec![],
            }],
            x_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: false,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
                time: None,
                ticks: crate::ir::AxisTickOptions::default(),
            },
            y_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: false,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
                time: None,
                ticks: crate::ir::AxisTickOptions::default(),
            },
            legend: LegendPos::None,
            legend_options: crate::ir::LegendOptions::default(),
            legend_title: None,
            title: None,
            width: 600.0,
            height: 400.0,
            size_mode: SizeMode::Canvas,
            data_labels: false,
            theme: crate::ir::Theme::default(),
            decimation: crate::ir::Decimation::default(),
            radial_axis: None,
        }
    }

    #[test]
    fn axis_domain_hard_min_max_override_data_and_suggestions() {
        let mut spec = make_scatter_spec(&[(1.0, 10.0), (100.0, 1000.0)]);
        spec.x_axis.min = Some(10.0);
        spec.x_axis.max = Some(90.0);
        spec.x_axis.suggested_min = Some(-100.0);
        spec.x_axis.suggested_max = Some(1000.0);

        assert_eq!(axis_domain(&spec, &spec.x_axis, |p| p.x), (10.0, 90.0));
    }

    #[test]
    fn scatter_layout_preserves_hard_bounds_after_nice_tick_rounding() {
        let mut spec = make_scatter_spec(&[(1.0, 2.0), (100.0, 200.0)]);
        spec.x_axis.min = Some(13.0);
        spec.x_axis.max = Some(87.0);
        spec.y_axis.min = Some(25.0);
        spec.y_axis.max = Some(175.0);
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let layout = compute_scatter_layout(&spec, &measurer);

        assert_eq!((layout.x_ticks.min, layout.x_ticks.max), (13.0, 87.0));
        assert_eq!((layout.y_ticks.min, layout.y_ticks.max), (25.0, 175.0));
    }

    #[test]
    fn scatter_temporal_axes_use_time_and_timeseries_spacing() {
        let day = 86_400_000_i64;
        let mut spec = make_scatter_spec(&[
            (0.0, 0.0),
            (day as f64, day as f64),
            ((4 * day) as f64, (4 * day) as f64),
        ]);
        spec.x_axis.scale_kind = ScaleKind::Time;
        spec.x_axis.time = Some(TimeOptions::default());
        spec.y_axis.scale_kind = ScaleKind::Timeseries;
        spec.y_axis.time = Some(TimeOptions::default());
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let layout = compute_scatter_layout(&spec, &measurer);
        let x0 = layout.xs.map(0.0);
        let x1 = layout.xs.map(day as f64);
        let x4 = layout.xs.map((4 * day) as f64);
        let y0 = layout.ys.map(0.0);
        let y1 = layout.ys.map(day as f64);
        let y4 = layout.ys.map((4 * day) as f64);

        assert!(((x1 - x0) / (x4 - x0) - 0.25).abs() < 1e-9);
        assert!(((y0 - y1) / (y0 - y4) - 0.5).abs() < 1e-9);
        assert!(!layout.x_temporal_ticks.is_empty());
        assert!(!layout.y_temporal_ticks.is_empty());
    }

    #[test]
    fn temporal_scatter_domain_ignores_values_outside_javascript_date_range() {
        let mut spec = make_scatter_spec(&[
            (1_000.0, 1_000.0),
            (10_000.0, 10_000.0),
            (8.64e15 + 1.0, 8.64e15 + 1.0),
        ]);
        spec.x_axis.scale_kind = ScaleKind::Time;
        spec.x_axis.time = Some(TimeOptions::default());

        assert_eq!(
            axis_domain(&spec, &spec.x_axis, |point| point.x),
            (1_000.0, 10_000.0)
        );
    }

    #[test]
    fn axis_domain_suggested_min_expands_below_data() {
        // x データが [1.0, 10.0]、suggested_min=-5.0 → ドメインが -5.0 まで広がる。
        let mut spec = make_scatter_spec(&[(1.0, 0.0), (10.0, 0.0)]);
        spec.x_axis.suggested_min = Some(-5.0);
        let (lo, _hi) = axis_domain(&spec, &spec.x_axis, |p| p.x);
        assert_eq!(
            lo, -5.0,
            "suggested_min=-5 はドメインを正確に -5.0 に設定すべき: 実際 lo={lo}"
        );
    }

    #[test]
    fn axis_domain_suggested_min_noop_when_data_lower() {
        // x データが [1.0, 10.0]、suggested_min=5.0 → データ(1.0)が優先されるので no-op。
        let mut spec = make_scatter_spec(&[(1.0, 0.0), (10.0, 0.0)]);
        spec.x_axis.suggested_min = Some(5.0);
        let (lo, _hi) = axis_domain(&spec, &spec.x_axis, |p| p.x);
        assert_eq!(
            lo, 1.0,
            "suggested_min=5 はデータの下端(1.0)を維持すべき: 実際 lo={lo}"
        );
    }

    #[test]
    fn axis_domain_suggested_max_expands_above_data() {
        // x データが [1.0, 10.0]、suggested_max=15.0 → ドメインが 15.0 まで広がる。
        let mut spec = make_scatter_spec(&[(1.0, 0.0), (10.0, 0.0)]);
        spec.x_axis.suggested_max = Some(15.0);
        let (_lo, hi) = axis_domain(&spec, &spec.x_axis, |p| p.x);
        assert_eq!(
            hi, 15.0,
            "suggested_max=15 はドメインを正確に 15.0 に設定すべき: 実際 hi={hi}"
        );
    }

    #[test]
    fn axis_domain_suggested_max_noop_when_data_higher() {
        // x データが [1.0, 10.0]、suggested_max=5.0 → データ(10.0)が優先されるので no-op。
        let mut spec = make_scatter_spec(&[(1.0, 0.0), (10.0, 0.0)]);
        spec.x_axis.suggested_max = Some(5.0);
        let (_lo, hi) = axis_domain(&spec, &spec.x_axis, |p| p.x);
        assert_eq!(
            hi, 10.0,
            "suggested_max=5 はデータの上端(10.0)を縮小してはいけない: 実際 hi={hi}"
        );
    }

    #[test]
    fn scatter_points_covers_all_series_and_indices() {
        let spec = make_scatter_spec(&[(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &m);
        let pts = scatter_points(&spec, &layout);
        assert_eq!(pts.len(), 3);
        for (i, p) in pts.iter().enumerate() {
            assert_eq!(p.series, 0);
            assert_eq!(p.index, i);
            assert_eq!(p.kind, "scatter");
        }
    }

    #[test]
    fn scatter_points_excludes_values_outside_hard_bounds() {
        let mut spec = make_scatter_spec(&[
            (1.0, 50.0),
            (50.0, 10.0),
            (50.0, 50.0),
            (50.0, 90.0),
            (100.0, 50.0),
        ]);
        spec.x_axis.min = Some(13.0);
        spec.x_axis.max = Some(87.0);
        spec.y_axis.min = Some(13.0);
        spec.y_axis.max = Some(87.0);
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &measurer);

        let points = scatter_points(&spec, &layout);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].index, 2);
        assert_eq!(
            (points[0].cx, points[0].cy),
            (
                (layout.plot_left + layout.plot_right) / 2.0,
                (layout.plot_top + layout.plot_bottom) / 2.0,
            )
        );
    }

    #[test]
    fn scatter_points_cx_monotone_with_x_values() {
        let spec = make_scatter_spec(&[(1.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &m);
        let pts = scatter_points(&spec, &layout);
        assert!(pts[0].cx < pts[1].cx && pts[1].cx < pts[2].cx);
    }

    #[test]
    fn scatter_points_skips_non_finite() {
        let spec = make_scatter_spec(&[(1.0, 2.0), (f64::NAN, 3.0), (5.0, 6.0)]);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &m);
        let pts = scatter_points(&spec, &layout);
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn log_axis_domain_uses_positive_values_and_zero_extends_one_decade() {
        let mut spec = make_scatter_spec(&[(0.0, 1.0), (-100.0, 2.0), (0.01, 3.0), (100.0, 4.0)]);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;

        let (min, max) = axis_domain(&spec, &spec.x_axis, |p| p.x);

        assert_eq!((min, max), (0.001, 100.0));
    }

    #[test]
    fn log_scatter_domain_for_single_f64_max_value_has_positive_width() {
        let mut spec = make_scatter_spec(&[(f64::MAX, f64::MAX)]);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;

        let x_domain = axis_domain(&spec, &spec.x_axis, |p| p.x);
        let y_domain = axis_domain(&spec, &spec.y_axis, |p| p.y);

        assert_eq!(x_domain, (f64::MAX / 10.0, f64::MAX));
        assert_eq!(y_domain, (f64::MAX / 10.0, f64::MAX));

        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &m);
        let points = scatter_points(&spec, &layout);
        assert_eq!(points.len(), 1);
        assert!(points[0].cx.is_finite() && points[0].cy.is_finite());
        assert!((layout.plot_left..=layout.plot_right).contains(&points[0].cx));
        assert!((layout.plot_top..=layout.plot_bottom).contains(&points[0].cy));
    }

    #[test]
    fn log_x_scale_places_each_decade_at_equal_pixel_intervals() {
        let mut spec = make_scatter_spec(&[(1.0, 1.0), (10.0, 2.0), (100.0, 3.0)]);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;
        spec.x_axis.min = Some(1.0);
        spec.x_axis.max = Some(100.0);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let layout = compute_scatter_layout(&spec, &m);
        let points = scatter_points(&spec, &layout);

        assert_eq!(layout.x_ticks.step, 0.0);
        assert_eq!(
            layout.x_ticks.ticks,
            vec![1.0, 10.0, 100.0],
            "major ticks should mark each decade"
        );
        assert!(!layout.x_minor_ticks.is_empty());
        assert!((points[0].cx - layout.plot_left).abs() < 1e-9);
        assert!((points[1].cx - (layout.plot_left + layout.plot_right) / 2.0).abs() < 1e-9);
        assert!((points[2].cx - layout.plot_right).abs() < 1e-9);
    }

    #[test]
    fn scatter_points_skips_non_positive_values_on_log_axes() {
        let mut spec = make_scatter_spec(&[(-1.0, 5.0), (0.0, 6.0), (1.0, 7.0), (10.0, 8.0)]);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let layout = compute_scatter_layout(&spec, &m);

        let points = scatter_points(&spec, &layout);

        assert_eq!(
            points.iter().map(|point| point.index).collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn log_axis_tick_labels_keep_small_values_full_precision() {
        let mut spec = make_scatter_spec(&[(0.0001, 0.001), (1.0, 1000.0)]);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let layout = compute_scatter_layout(&spec, &m);
        let scene = build(&spec, &m);

        assert!(
            scene.items.iter().any(|item| matches!(
                item,
                Prim::Text { content, anchor: Anchor::Middle, .. } if content == "0.0001"
            )),
            "logarithmic x-axis labels should preserve small values"
        );
        assert!(
            scene.items.iter().any(|item| matches!(
                item,
                Prim::Text { content, anchor: Anchor::End, .. } if content == "0.001"
            )),
            "logarithmic y-axis labels should preserve small values"
        );
        let x_minor_lines = scene
            .items
            .iter()
            .filter(|item| {
                matches!(item,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (x1 - x2).abs() < 1e-9
                            && (*y1 - layout.plot_top).abs() < 1e-9
                            && (*y2 - layout.plot_bottom).abs() < 1e-9
                            && (stroke.a - spec.theme.grid_color.a * 0.5).abs() < 1e-6
                )
            })
            .count();
        assert_eq!(x_minor_lines, layout.x_minor_ticks.len());

        let y_minor_lines = scene
            .items
            .iter()
            .filter(|item| {
                matches!(item,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (y1 - y2).abs() < 1e-9
                            && (*x1 - layout.plot_left).abs() < 1e-9
                            && (*x2 - layout.plot_right).abs() < 1e-9
                            && (stroke.a - spec.theme.grid_color.a * 0.5).abs() < 1e-6
                )
            })
            .count();
        assert_eq!(y_minor_lines, layout.y_minor_ticks.len());
    }

    #[test]
    fn y_grid_display_false_drops_horizontal_gridlines_but_keeps_labels() {
        let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
        spec.y_axis.grid.display = false;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let layout = compute_scatter_layout(&spec, &m);
        // 水平グリッド線: y1==y2 かつ x1==plot_left, x2==plot_right, 色=grid_color(ink ではない)
        // ベースライン(y=plot_bottom, ink 色)は border.display=true(既定)で残る点に注意。
        let grid = layout
            .y_ticks
            .ticks
            .iter()
            .filter(|_| {
                scene.items.iter().any(|p| {
                    matches!(p,
                        Prim::Line { y1, y2, x1, x2, stroke, .. }
                            if (y1 - y2).abs() < 0.01
                                && (*x1 - layout.plot_left).abs() < 0.01
                                && (*x2 - layout.plot_right).abs() < 0.01
                                && stroke.r == spec.theme.grid_color.r
                                && stroke.g == spec.theme.grid_color.g
                                && stroke.b == spec.theme.grid_color.b
                    )
                })
            })
            .count();
        assert_eq!(grid, 0, "y_axis.grid.display=false → 水平グリッド 0 本");
        // y 軸ラベル(text-anchor=End)は残る。
        let y_labels = scene
            .items
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Prim::Text {
                        anchor: Anchor::End,
                        ..
                    }
                )
            })
            .count();
        assert!(y_labels > 0, "grid を消しても y 目盛りラベルは残る");
    }

    #[test]
    fn x_border_display_false_drops_bottom_baseline() {
        let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
        spec.x_axis.border.display = false;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let layout = compute_scatter_layout(&spec, &m);
        let ink = spec.theme.text_color;
        // ベースラインの識別: y=plot_bottom の水平線 かつ ink 色 かつ x2==plot_right。
        // 一番下の水平グリッド線も y=plot_bottom だが色は grid_color。
        let baseline = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { y1, y2, x1, x2, stroke, .. }
                        if (y1 - y2).abs() < 0.01
                            && (*y1 - layout.plot_bottom).abs() < 0.01
                            && (*x1 - layout.plot_left).abs() < 0.01
                            && (*x2 - layout.plot_right).abs() < 0.01
                            && stroke.r == ink.r && stroke.g == ink.g && stroke.b == ink.b
                )
            })
            .count();
        assert_eq!(
            baseline, 0,
            "x_axis.border.display=false → 下側ベースライン無し"
        );
    }

    #[test]
    fn y_border_display_false_drops_left_baseline() {
        let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
        spec.y_axis.border.display = false;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let layout = compute_scatter_layout(&spec, &m);
        let ink = spec.theme.text_color;
        // 左辺ベースライン: x1==x2==plot_left, y=plot_top..plot_bottom, ink 色。
        let baseline = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (x1 - x2).abs() < 0.01
                            && (*x1 - layout.plot_left).abs() < 0.01
                            && (*y1 - layout.plot_top).abs() < 0.01
                            && (*y2 - layout.plot_bottom).abs() < 0.01
                            && stroke.r == ink.r && stroke.g == ink.g && stroke.b == ink.b
                )
            })
            .count();
        assert_eq!(
            baseline, 0,
            "y_axis.border.display=false → 左辺ベースライン無し"
        );
    }

    #[test]
    fn y_axis_title_renders_rotated() {
        let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
        spec.y_axis.title = Some(AxisTitle {
            text: "測定値".into(),
            color: Some(Color {
                r: 128,
                g: 0,
                b: 128,
                a: 1.0,
            }),
            font_size: Some(18.0),
            align: AxisTitleAlign::Center,
        });
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rotated = scene.items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg: Some(deg), size, fill, .. }
                    if content == "測定値"
                        && (deg.abs() - 90.0).abs() < 0.1
                        && (size - 18.0).abs() < 1e-9
                        && fill.r == 128 && fill.b == 128
            )
        });
        assert!(rotated, "Y 軸タイトルは -90deg で描画される");
    }

    #[test]
    fn x_axis_title_renders_horizontal() {
        let mut spec = make_scatter_spec(&[(0.0, 0.0), (10.0, 20.0)]);
        spec.x_axis.title = Some(AxisTitle {
            text: "時刻".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::End,
        });
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let layout = compute_scatter_layout(&spec, &m);
        let has_x = scene.items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg: None, x, .. }
                    if content == "時刻" && (x - layout.plot_right).abs() < 0.1
            )
        });
        assert!(
            has_x,
            "X 軸タイトル: align=End → x=plot_right(水平テキスト)"
        );
    }
}
