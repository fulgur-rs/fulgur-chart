//! bar チャートのレイアウト: ChartSpec → Scene。
//! 縦棒のみ（horizontal は後続タスク）。決定的に組み立て、NaN/Inf/panic を出さない。

use crate::font::DEFAULT_FONT;
use crate::ir::{ChartSpec, Color, LegendPos};
use crate::num::fmt_num;
use crate::scale::{LinearScale, nice_ticks};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

const OUTER_PAD: f64 = 8.0;
const TITLE_FONT: f64 = 16.0;
const LABEL_FONT: f64 = 12.0;
const TITLE_BAND: f64 = 28.0;
const LEGEND_BAND: f64 = 26.0;
const X_LABEL_BAND: f64 = 22.0;

const GRID: Color = Color {
    r: 224,
    g: 224,
    b: 224,
    a: 1.0,
};
const INK: Color = Color {
    r: 102,
    g: 102,
    b: 102,
    a: 1.0,
};

pub fn build(spec: &ChartSpec) -> Scene {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

    // 2. y ドメイン。
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    for s in &spec.series {
        for &v in &s.values {
            if v.is_finite() {
                if v < data_min {
                    data_min = v;
                }
                if v > data_max {
                    data_max = v;
                }
            }
        }
    }
    if !data_min.is_finite() || !data_max.is_finite() {
        data_min = 0.0;
        data_max = 1.0;
    }
    let (domain_min, mut domain_max) = if spec.y_axis.begin_at_zero {
        (data_min.min(0.0), data_max.max(0.0))
    } else {
        (data_min, data_max)
    };
    // 上限>下限を保証（縮退時の保険。nice_ticks 自体も縮退対応するが念のため）。
    if domain_max <= domain_min {
        domain_max = domain_min + 1.0;
    }
    let nt = nice_ticks(domain_min, domain_max, 5);

    // 3. y 軸ラベル幅。
    let mut max_w = 0.0_f32;
    for &t in &nt.ticks {
        let s = fmt_num(t);
        let w = m.width(&s, LABEL_FONT as f32);
        if w > max_w {
            max_w = w;
        }
    }
    let y_axis_w = max_w as f64 + 10.0;

    // 4. 凡例の有無。
    let has_legend = matches!(spec.legend, LegendPos::Top | LegendPos::Bottom)
        && spec.series.iter().any(|s| !s.name.is_empty());

    // 5. プロット領域。
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if has_legend && spec.legend == LegendPos::Top {
        LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom {
        LEGEND_BAND
    } else {
        0.0
    };
    let plot_left = OUTER_PAD + y_axis_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

    // 6. y スケール（上下反転）。
    let ys = LinearScale::new(nt.min, nt.max, plot_bottom, plot_top);

    let mut items: Vec<Prim> = Vec::new();

    // a. タイトル。
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

    // b. 横グリッド + y 軸ラベル。
    for &t in &nt.ticks {
        let y = ys.map(t);
        items.push(Prim::Line {
            x1: plot_left,
            y1: y,
            x2: plot_right,
            y2: y,
            stroke: GRID,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: y + LABEL_FONT * 0.35,
            size: LABEL_FONT,
            anchor: Anchor::End,
            fill: INK,
            content: fmt_num(t),
        });
    }

    // c. x ベースライン。
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_bottom,
        x2: plot_right,
        y2: plot_bottom,
        stroke: INK,
        stroke_width: 1.0,
    });

    // d. カテゴリと bar。
    let n = spec.categories.len().max(1);
    let band_w = (plot_right - plot_left) / n as f64;
    let s = spec.series.len().max(1);
    let group_w = band_w * 0.8;
    let bar_w = group_w / s as f64;

    let base_v = 0.0_f64.clamp(nt.min, nt.max);
    let baseline_y = ys.map(base_v);

    for (i, cat) in spec.categories.iter().enumerate() {
        let band_left = plot_left + i as f64 * band_w;
        let center = band_left + band_w / 2.0;

        if !cat.is_empty() {
            items.push(Prim::Text {
                x: center,
                y: plot_bottom + X_LABEL_BAND * 0.7,
                size: LABEL_FONT,
                anchor: Anchor::Middle,
                fill: INK,
                content: cat.clone(),
            });
        }

        for (sidx, ser) in spec.series.iter().enumerate() {
            let bx = band_left + band_w * 0.1 + sidx as f64 * bar_w;
            let v = ser.values.get(i).copied().unwrap_or(0.0);
            let vy = ys.map(v);
            let y_top = vy.min(baseline_y);
            let h = (vy - baseline_y).abs();
            items.push(Prim::Rect {
                x: bx,
                y: y_top,
                w: (bar_w * 0.9).max(0.0),
                h,
                fill: ser.fill_at(i),
            });
        }
    }

    // e. 凡例。
    if has_legend {
        // 各エントリ幅と合計（末尾間隔 16 を最後だけ除く）。
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = 12.0 + 4.0 + m.width(&ser.name, LABEL_FONT as f32) as f64 + 16.0;
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
                y: legend_cy + LABEL_FONT * 0.35,
                size: LABEL_FONT,
                anchor: Anchor::Start,
                fill: INK,
                content: ser.name.clone(),
            });
            let ew = 12.0 + 4.0 + m.width(&ser.name, LABEL_FONT as f32) as f64 + 16.0;
            cursor += ew;
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
