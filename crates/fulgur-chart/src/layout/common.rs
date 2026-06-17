//! bar/line が共有するプロット領域・軸・グリッド・凡例の構築。

use crate::ir::{ChartSpec, Color, LegendPos};
use crate::num::fmt_num;
use crate::scale::{LinearScale, NiceTicks, nice_ticks};
use crate::scene::{Anchor, Prim};
use crate::text::TextMeasurer;

pub const OUTER_PAD: f64 = 8.0;
pub const TITLE_FONT: f64 = 16.0;
pub const LABEL_FONT: f64 = 12.0;
pub const TITLE_BAND: f64 = 28.0;
pub const LEGEND_BAND: f64 = 26.0;
pub const X_LABEL_BAND: f64 = 22.0;
pub const TEXT_BASELINE_RATIO: f64 = 0.35;
pub const X_LABEL_CENTER_RATIO: f64 = 0.7;
pub const GRID: Color = Color {
    r: 224,
    g: 224,
    b: 224,
    a: 1.0,
};
pub const INK: Color = Color {
    r: 102,
    g: 102,
    b: 102,
    a: 1.0,
};

/// プロット領域と y スケール・目盛り。
pub struct Frame {
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub ticks: NiceTicks,
    pub ys: LinearScale,
}

/// 凡例の有無を判定する（Top/Bottom かつ名前付き系列が 1 つ以上）。
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(spec.legend, LegendPos::Top | LegendPos::Bottom)
        && spec.series.iter().any(|s| !s.name.is_empty())
}

/// spec から y ドメイン(begin_at_zero尊重)・nice_ticks・y軸ラベル幅・プロット領域・凡例帯を計算。
pub fn compute(spec: &ChartSpec, m: &TextMeasurer) -> Frame {
    // y ドメイン。
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
    // 上限>下限を保証（縮退時の保険）。
    if domain_max <= domain_min {
        domain_max = domain_min + 1.0;
    }
    let ticks = nice_ticks(domain_min, domain_max, 5);

    // y 軸ラベル幅。
    let mut max_w = 0.0_f32;
    for &t in &ticks.ticks {
        let s = fmt_num(t);
        let w = m.width(&s, LABEL_FONT as f32);
        if w > max_w {
            max_w = w;
        }
    }
    let y_axis_w = max_w as f64 + 10.0;

    // 凡例の有無。
    let legend = has_legend(spec);

    // プロット領域。
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
    let plot_left = OUTER_PAD + y_axis_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

    // y スケール（上下反転）。
    let ys = LinearScale::new(ticks.min, ticks.max, plot_bottom, plot_top);

    Frame {
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
        ticks,
        ys,
    }
}

/// n カテゴリ中 i 番目の x 中心。band_w=(plot_right-plot_left)/n。
pub fn category_center(frame: &Frame, i: usize, n: usize) -> f64 {
    let band_w = (frame.plot_right - frame.plot_left) / n.max(1) as f64;
    frame.plot_left + (i as f64 + 0.5) * band_w
}

pub fn band_width(frame: &Frame, n: usize) -> f64 {
    (frame.plot_right - frame.plot_left) / n.max(1) as f64
}

/// 共有フレーム描画: タイトル→横グリッド+yラベル→xベースライン→xカテゴリラベル→凡例。
/// チャート本体(bar/line)はこの後に重ねて描く。
pub fn draw_frame(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &Frame, m: &TextMeasurer) {
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

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

    // 2. 横グリッド + y 軸ラベル。
    for &t in &frame.ticks.ticks {
        let y = frame.ys.map(t);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: y,
            x2: frame.plot_right,
            y2: y,
            stroke: GRID,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x: frame.plot_left - 6.0,
            y: y + LABEL_FONT * TEXT_BASELINE_RATIO,
            size: LABEL_FONT,
            anchor: Anchor::End,
            fill: INK,
            content: fmt_num(t),
        });
    }

    // 3. x ベースライン。
    items.push(Prim::Line {
        x1: frame.plot_left,
        y1: frame.plot_bottom,
        x2: frame.plot_right,
        y2: frame.plot_bottom,
        stroke: INK,
        stroke_width: 1.0,
    });

    // 4. x カテゴリラベル。
    let n = spec.categories.len().max(1);
    for (i, cat) in spec.categories.iter().enumerate() {
        if !cat.is_empty() {
            items.push(Prim::Text {
                x: category_center(frame, i, n),
                y: frame.plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::Middle,
                fill: INK,
                content: cat.clone(),
            });
        }
    }

    // 5. 凡例。
    if has_legend(spec) {
        // 各エントリ幅と合計（末尾間隔 16 を最後だけ除く）。
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(m, &ser.name);
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
                y: legend_cy + LABEL_FONT * TEXT_BASELINE_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::Start,
                fill: INK,
                content: ser.name.clone(),
            });
            let ew = legend_entry_width(m, &ser.name);
            cursor += ew;
        }
    }
}

/// 凡例 1 エントリの占有幅: swatch幅(12) + gap(4) + ラベル幅 + trailing間隔(16)。
pub fn legend_entry_width(m: &TextMeasurer, name: &str) -> f64 {
    12.0 + 4.0 + m.width(name, LABEL_FONT as f32) as f64 + 16.0
}
