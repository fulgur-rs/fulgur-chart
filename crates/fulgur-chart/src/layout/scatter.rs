//! scatter チャート: 線形 x × 線形 y 軸に点(円)を描く。
//! カテゴリ系の `common::compute` は x をカテゴリ前提にするため、ここでは
//! 線形フレームを自前で組む。共有できる凡例/定数/テーマは `common` を再利用する。

use super::common::{
    LEGEND_BAND, OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND,
    X_LABEL_CENTER_RATIO, draw_vertical_legend, legend_band_width_vertical, legend_entry_width,
};
use crate::ir::{ChartKind, ChartSpec, Color, LegendPos, Point};
use crate::num::fmt_num;
use crate::scale::{LinearScale, nice_ticks};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// scatter のマーカー既定半径。chart.js scatter の pointRadius 既定値 ~3.0。
const DEFAULT_POINT_R: f64 = 3.0;

/// bubble で `point.r` が無い場合の既定半径。bubble は通常 r を持つが保険。
const DEFAULT_BUBBLE_R: f64 = 5.0;

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
fn axis_domain(spec: &ChartSpec, select: impl Fn(&Point) -> f64) -> (f64, f64) {
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
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 1.0)
    } else {
        (lo, hi)
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // x/y ドメイン → nice ticks。
    let (xmin, xmax) = axis_domain(spec, |p| p.x);
    let (ymin, ymax) = axis_domain(spec, |p| p.y);
    let x_ticks = nice_ticks(xmin, xmax, 10);
    let y_ticks = nice_ticks(ymin, ymax, 10);

    // y 軸ラベル幅(目盛りラベルの最大幅 + 余白)。
    let mut max_y_w = 0.0_f32;
    for &t in &y_ticks.ticks {
        let w = m.width(&fmt_num(t), label_font as f32);
        if w > max_y_w {
            max_y_w = w;
        }
    }
    let y_axis_w = max_y_w as f64 + 10.0;

    // 凡例帯(カテゴリ系 common と同じロジック)。
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
    let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    let legend_left = if legend && spec.legend == LegendPos::Left {
        legend_band_width_vertical(m, &series_names, label_font)
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        legend_band_width_vertical(m, &series_names, label_font)
    } else {
        0.0
    };

    let plot_left = OUTER_PAD + y_axis_w + legend_left;
    let plot_right = spec.width - OUTER_PAD - legend_right;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

    // 線形スケール: x は左→右(非反転)、y は下→上(反転)。
    let xs = LinearScale::new(x_ticks.min, x_ticks.max, plot_left, plot_right);
    let ys = LinearScale::new(y_ticks.min, y_ticks.max, plot_bottom, plot_top);

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
        });
    }

    // 2. 横グリッド + y 目盛りラベル(右寄せ)。
    for &t in &y_ticks.ticks {
        let y = ys.map(t);
        items.push(Prim::Line {
            x1: plot_left,
            y1: y,
            x2: plot_right,
            y2: y,
            stroke: spec.theme.grid_color,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: y + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: fmt_num(t),
        });
    }

    // 3. 縦グリッド + x 目盛りラベル(軸下に中央寄せ)。
    for &t in &x_ticks.ticks {
        let x = xs.map(t);
        items.push(Prim::Line {
            x1: x,
            y1: plot_top,
            x2: x,
            y2: plot_bottom,
            stroke: spec.theme.grid_color,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: fmt_num(t),
        });
    }

    // 4. 軸ベースライン(x 下辺 + y 左辺)。
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_bottom,
        x2: plot_right,
        y2: plot_bottom,
        stroke: ink,
        stroke_width: 1.0,
    });
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_top,
        x2: plot_left,
        y2: plot_bottom,
        stroke: ink,
        stroke_width: 1.0,
    });

    // 5. 点(円)。系列・点とも入力順。非有限座標はスキップ。
    for ser in &spec.series {
        for (i, p) in ser.points.iter().enumerate() {
            if !p.x.is_finite() || !p.y.is_finite() {
                continue;
            }
            items.push(Prim::Circle {
                cx: xs.map(p.x),
                cy: ys.map(p.y),
                r: point_radius(&spec.kind, p, ser.point_radius),
                fill: ser.fill_at(i),
            });
        }
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

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
