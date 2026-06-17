//! bar チャートのレイアウト: ChartSpec → Scene。
//! 縦棒・横棒に対応。決定的に組み立て、NaN/Inf/panic を出さない。

use crate::font::DEFAULT_FONT;
use crate::ir::ChartSpec;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

/// band 内のグループ幅比。
const GROUP_RATIO: f64 = 0.8;
/// band 左右パディング比。
const BAND_PAD_RATIO: f64 = 0.1;
/// bar 幅の塗り比。
const BAR_FILL_RATIO: f64 = 0.9;

pub fn build(spec: &ChartSpec) -> Scene {
    match spec.kind {
        crate::ir::ChartKind::Bar { horizontal: true } => build_horizontal(spec),
        _ => build_vertical(spec),
    }
}

fn build_vertical(spec: &ChartSpec) -> Scene {
    use super::common::{INK, LABEL_FONT, LABEL_GAP, value_label};
    use crate::scene::Anchor;

    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = super::common::compute(spec, &m);

    let mut items: Vec<Prim> = Vec::new();
    super::common::draw_frame(&mut items, spec, &frame, &m);

    // bar 本体: カテゴリ band 内に系列グループの矩形を重ねる。
    let n = spec.categories.len().max(1);
    let band_w = super::common::band_width(&frame, n);
    let s = spec.series.len().max(1);
    let group_w = band_w * GROUP_RATIO;
    let bar_w = group_w / s as f64;

    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);

    for i in 0..spec.categories.len() {
        let band_left = super::common::category_center(&frame, i, n) - band_w / 2.0;

        for (sidx, ser) in spec.series.iter().enumerate() {
            let bx = band_left + band_w * BAND_PAD_RATIO + sidx as f64 * bar_w;
            let v = ser.values.get(i).copied().unwrap_or(0.0);
            let vy = frame.ys.map(v);
            let y_top = vy.min(baseline_y);
            let h = (vy - baseline_y).abs();
            items.push(Prim::Rect {
                x: bx,
                y: y_top,
                w: (bar_w * BAR_FILL_RATIO).max(0.0),
                h,
                fill: ser.fill_at(i),
            });
            if spec.data_labels && ser.values.get(i).is_some() && v.is_finite() {
                let cx = bx + (bar_w * BAR_FILL_RATIO) / 2.0;
                // 正の棒は上に伸びるので上端の少し上(LABEL_GAP)に置く。
                // 負の棒は下端の下に置くが、テキストのベースラインが棒の下辺より
                // 下に来るよう ほぼ1行分(LABEL_FONT) 下げる(オフセットが非対称な理由)。
                let label_y = if v >= base_v {
                    y_top - LABEL_GAP
                } else {
                    y_top + h + LABEL_FONT
                };
                items.push(value_label(cx, label_y, Anchor::Middle, INK, v));
            }
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 横棒(indexAxis:"y"): 値軸=X(左→右非反転)、カテゴリ軸=Y(上→下)。
/// 縦向き前提の common::compute/draw_frame は使わず、転置レイアウトを自前で描く。
fn build_horizontal(spec: &ChartSpec) -> Scene {
    use crate::layout::common::*;
    use crate::num::fmt_num;
    use crate::scale::{LinearScale, nice_ticks};
    use crate::scene::Anchor;

    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let (dmin, dmax) = value_domain(spec);
    let ticks = nice_ticks(dmin, dmax, 5);

    // カテゴリラベル幅(左軸): 各 categories の最大幅 + 10。空なら最低でも 10。
    let mut max_cat_w = 0.0_f32;
    for c in &spec.categories {
        let w = m.width(c, LABEL_FONT as f32);
        if w > max_cat_w {
            max_cat_w = w;
        }
    }
    let cat_w = max_cat_w as f64 + 10.0;

    // 凡例の有無(縦棒と同じ判定)。
    let has_legend = matches!(
        spec.legend,
        crate::ir::LegendPos::Top | crate::ir::LegendPos::Bottom
    ) && spec.series.iter().any(|s| !s.name.is_empty());

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if has_legend && spec.legend == crate::ir::LegendPos::Top {
        LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == crate::ir::LegendPos::Bottom {
        LEGEND_BAND
    } else {
        0.0
    };

    let plot_left = OUTER_PAD + cat_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

    // 値→X(非反転)。
    let xs = LinearScale::new(ticks.min, ticks.max, plot_left, plot_right);

    let mut items: Vec<Prim> = Vec::new();

    // 1. タイトル。
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: INK,
            content: title.clone(),
        });
    }

    // 2. 縦グリッド + 値ラベル(下)。
    for &t in &ticks.ticks {
        let x = xs.map(t);
        items.push(Prim::Line {
            x1: x,
            y1: plot_top,
            x2: x,
            y2: plot_bottom,
            stroke: GRID,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: LABEL_FONT,
            anchor: Anchor::Middle,
            fill: INK,
            content: fmt_num(t),
        });
    }

    // 3. 左軸線(カテゴリ軸)。
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_top,
        x2: plot_left,
        y2: plot_bottom,
        stroke: INK,
        stroke_width: 1.0,
    });

    // 4. カテゴリ band と 横棒。
    let n = spec.categories.len().max(1);
    let band_h = (plot_bottom - plot_top) / n as f64;
    let s = spec.series.len().max(1);
    let group_h = band_h * GROUP_RATIO;
    let bar_h = group_h / s as f64;

    let base_v = 0.0_f64.clamp(ticks.min, ticks.max);
    let baseline_x = xs.map(base_v);

    for i in 0..spec.categories.len() {
        let band_top = plot_top + i as f64 * band_h;
        let center_y = band_top + band_h / 2.0;

        // カテゴリラベル(左)。
        if !spec.categories[i].is_empty() {
            items.push(Prim::Text {
                x: plot_left - 6.0,
                y: center_y + LABEL_FONT * TEXT_BASELINE_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::End,
                fill: INK,
                content: spec.categories[i].clone(),
            });
        }

        for (sidx, ser) in spec.series.iter().enumerate() {
            let by = band_top + band_h * BAND_PAD_RATIO + sidx as f64 * bar_h;
            let v = ser.values.get(i).copied().unwrap_or(0.0);
            let vx = xs.map(v);
            let x = vx.min(baseline_x);
            let w = (vx - baseline_x).abs();
            items.push(Prim::Rect {
                x,
                y: by,
                w,
                h: (bar_h * BAR_FILL_RATIO).max(0.0),
                fill: ser.fill_at(i),
            });
            if spec.data_labels && ser.values.get(i).is_some() && v.is_finite() {
                let cy = by + (bar_h * BAR_FILL_RATIO) / 2.0 + LABEL_FONT * TEXT_BASELINE_RATIO;
                // 正は棒右端の右(Start)、負は左端の左(End)に LABEL_GAP 分離す。
                let (lx, anchor) = if v >= base_v {
                    (vx + LABEL_GAP, Anchor::Start)
                } else {
                    (vx - LABEL_GAP, Anchor::End)
                };
                items.push(value_label(lx, cy, anchor, INK, v));
            }
        }
    }

    // 5. 凡例(common::draw_frame の配置を踏襲)。
    if has_legend {
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(&m, &ser.name);
            total += ew;
            if k == spec.series.len() - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == crate::ir::LegendPos::Top {
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
                y: legend_cy + LABEL_FONT * TEXT_BASELINE_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::Start,
                fill: INK,
                content: ser.name.clone(),
            });
            cursor += legend_entry_width(&m, &ser.name);
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
