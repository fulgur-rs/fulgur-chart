//! scatter チャート: 線形 x × 線形 y 軸に点(円)を描く。
//! カテゴリ系の `common::compute` は x をカテゴリ前提にするため、ここでは
//! 線形フレームを自前で組む。共有できる凡例/定数/テーマは `common` を再利用する。

use super::common::{
    AXIS_TITLE_BAND, LEGEND_BAND, OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT,
    X_LABEL_BAND, X_LABEL_CENTER_RATIO, draw_vertical_legend, legend_band_width_vertical,
    legend_entry_width,
};
use crate::ir::{AxisSpec, AxisTitleAlign, ChartKind, ChartSpec, Color, LegendPos, Point};
use crate::num::fmt_num;
use crate::scale::{LinearScale, NiceTicks, nice_ticks};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// scatter のマーカー既定半径。chart.js scatter の pointRadius 既定値 ~3.0。
const DEFAULT_POINT_R: f64 = 3.0;

/// bubble で `point.r` が無い場合の既定半径。bubble は通常 r を持つが保険。
const DEFAULT_BUBBLE_R: f64 = 5.0;

/// 単一データ点の画素空間情報（scatter/line/bubble 共用）。
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

/// scatter/bubble の自前フレーム（`common::compute` を使わない線形軸系）。
#[derive(Debug, Clone)]
pub struct ScatterLayout {
    pub xs: LinearScale,
    pub ys: LinearScale,
    pub x_ticks: NiceTicks,
    pub y_ticks: NiceTicks,
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
}

/// scatter/bubble チャートのフレームを計算して返す。
/// `build` のインライン計算と同一の式（単一の真実源）。
pub fn compute_scatter_layout(spec: &ChartSpec, m: &TextMeasurer) -> ScatterLayout {
    let label_font = spec.theme.font_size;
    let (xmin, xmax) = axis_domain(spec, &spec.x_axis, |p| p.x);
    let (ymin, ymax) = axis_domain(spec, &spec.y_axis, |p| p.y);
    let x_ticks = nice_ticks(xmin, xmax, 10);
    let y_ticks = nice_ticks(ymin, ymax, 10);
    let mut max_y_w = 0.0_f32;
    for &t in &y_ticks.ticks {
        let w = m.width(&crate::num::fmt_num(t), label_font as f32);
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
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if legend && spec.legend == LegendPos::Top {
        LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if legend && spec.legend == LegendPos::Bottom {
        LEGEND_BAND
    } else {
        0.0
    };
    // series_names の割り当ては凡例が左右にあるときだけ必要なため遅延評価する。
    let (legend_left, legend_right_w) =
        if legend && (spec.legend == LegendPos::Left || spec.legend == LegendPos::Right) {
            let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
            let w = legend_band_width_vertical(m, &series_names, label_font);
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
        xs: LinearScale::new(x_ticks.min, x_ticks.max, plot_left, plot_right),
        ys: LinearScale::new(y_ticks.min, y_ticks.max, plot_bottom, plot_top),
        x_ticks,
        y_ticks,
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
    }
}

/// scatter/bubble の全点を返す（renderer とモデルの単一の真実源）。
/// 非有限座標はスキップ。bubble は `PointBox.r` に実ピクセル半径を格納。
pub fn scatter_points(spec: &ChartSpec, layout: &ScatterLayout) -> Vec<PointBox> {
    let kind = match &spec.kind {
        ChartKind::Bubble => "bubble",
        _ => "scatter",
    };
    let mut pts = Vec::new();
    for (sidx, ser) in spec.series.iter().enumerate() {
        for (i, p) in ser.points.iter().enumerate() {
            if !p.x.is_finite() || !p.y.is_finite() {
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

/// 1 点の半径を返す。bubble はデータの第3次元 `point.r` を優先し、無ければ
/// dataset の `pointRadius`、それも無ければ既定値。scatter は dataset の `pointRadius`
/// (chart.js の指定)を使い、無指定なら既定値。非有限/負の半径は不正な SVG を避けるため
/// それぞれの既定値にフォールバックする。
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
        _ => valid(dataset_radius.unwrap_or(DEFAULT_POINT_R), DEFAULT_POINT_R),
    }
}

/// 凡例の有無(Top/Bottom/Left/Right かつ名前付き系列が 1 つ以上)。
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.series.iter().any(|s| !s.name.is_empty())
}

/// 全系列の全点から 1 軸ぶんのドメインを求める。`select` で x/y を選ぶ。
/// 非有限値は無視し、有限値が無ければ 0.0..1.0 にフォールバックする(NaN/panic 回避)。
/// nice_ticks 側が min==max(縮退)を吸収するため、ここでは追加の拡張はしない。
/// `axis_spec` の suggested_min/suggested_max はドメインを広げるだけ(データが優先)。
pub(crate) fn axis_domain(
    spec: &ChartSpec,
    axis_spec: &AxisSpec,
    select: impl Fn(&Point) -> f64,
) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in &spec.series {
        for p in &s.points {
            let v = select(p);
            if v.is_finite() {
                if v < lo {
                    lo = v;
                }
                if v > hi {
                    hi = v;
                }
            }
        }
    }
    // データなし: suggested を初期シードとして使う(chart.js 互換)。suggested もなければ 0..1。
    if !lo.is_finite() || !hi.is_finite() {
        lo = axis_spec
            .suggested_min
            .filter(|s| s.is_finite())
            .unwrap_or(0.0);
        hi = axis_spec
            .suggested_max
            .filter(|s| s.is_finite())
            .unwrap_or(if lo == 0.0 { 1.0 } else { lo + 1.0 });
        if axis_spec.begin_at_zero {
            lo = lo.min(0.0);
            hi = hi.max(0.0);
        }
        return (lo, if hi > lo { hi } else { lo + 1.0 });
    }
    // begin_at_zero でドメインに 0 を含める。
    if axis_spec.begin_at_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    // suggestedMin/suggestedMax: データが優先、suggested はドメインを広げるだけ。
    // 非有限値（Infinity/NaN）は nice_ticks で無限 range を生じさせるため無視する。
    if let Some(s) = axis_spec.suggested_min
        && s.is_finite()
        && s < lo
    {
        lo = s;
    }
    if let Some(s) = axis_spec.suggested_max
        && s.is_finite()
        && s > hi
    {
        hi = s;
    }
    (lo, hi)
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

    // 凡例描画用フラグ(フレーム計算ではなく表示用)。
    let legend = has_legend(spec);
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
        legend_band_width_vertical(m, &series_names, label_font)
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
    for &t in &y_ticks.ticks {
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
            content: fmt_num(t),
            rotate_deg: None,
        });
    }

    // 3. 縦グリッド + x 目盛りラベル(軸下に中央寄せ)。x_axis.grid.display=false のときは
    // Prim::Line を落とすが、目盛りラベルは常に残す。
    let x_grid_cfg = &spec.x_axis.grid;
    let x_grid_color = x_grid_cfg.color.unwrap_or(spec.theme.grid_color);
    for &t in &x_ticks.ticks {
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
            content: fmt_num(t),
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
        for &t in &y_ticks.ticks {
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
        for &t in &x_ticks.ticks {
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

    // 5. 点(円)。共有 scatter_points(単一真実源)から描画。
    for b in scatter_points(spec, &layout) {
        let ser = &spec.series[b.series];
        items.push(Prim::Circle {
            cx: b.cx,
            cy: b.cy,
            r: b.r,
            fill: ser.fill_at(b.index),
            stroke: ser.stroke_at(b.index),
            stroke_width: ser.stroke_width,
        });
    }

    // 6. 凡例(Top/Bottom: 横並び。draw_frame と同じ配置)。
    if legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(m, &ser.name, label_font);
            total += ew;
            if k == spec.series.len() - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == LegendPos::Top {
            OUTER_PAD + title_band + LEGEND_BAND / 2.0
        } else {
            spec.height - OUTER_PAD - LEGEND_BAND / 2.0
        };
        let mut cursor = start_x;
        for ser in &spec.series {
            items.push(Prim::Rect {
                x: cursor,
                y: legend_cy - 6.0,
                w: 12.0,
                h: 12.0,
                fill: ser.fill_at(0),
            });
            items.push(Prim::Text {
                x: cursor + 16.0,
                y: legend_cy + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: ser.name.clone(),
                rotate_deg: None,
            });
            cursor += legend_entry_width(m, &ser.name, label_font);
        }
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
        draw_vertical_legend(
            &mut items,
            &entries,
            band_x,
            plot_top,
            plot_bottom,
            ink,
            label_font,
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
    use crate::font::DEFAULT_FONT;
    use crate::ir::{
        AxisBorder, AxisGrid, AxisSpec, AxisTitle, AxisTitleAlign, ChartKind, ChartSpec, Color,
        LegendPos, Point, Series, SeriesType,
    };
    use crate::text::TextMeasurer;

    fn make_scatter_spec(points: &[(f64, f64)]) -> ChartSpec {
        let palette = crate::palette::PALETTE.to_vec();
        ChartSpec {
            kind: ChartKind::Scatter,
            categories: vec![],
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
                tension: 0.0,
                series_type: SeriesType::Bar,
                point_radius: None,
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
            },
            legend: LegendPos::None,
            title: None,
            width: 600.0,
            height: 400.0,
            data_labels: false,
            theme: crate::ir::Theme::default(),
            decimation: crate::ir::Decimation::default(),
        }
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
