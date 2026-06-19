use super::common::{
    OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND, X_LABEL_CENTER_RATIO,
};
use crate::ir::{ChartKind, ChartSpec, Color};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

const NAN_COLOR: Color = Color {
    r: 224,
    g: 224,
    b: 224,
    a: 1.0,
};

fn lerp_color(lo: Color, hi: Color, t: f64) -> Color {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Color {
        r: (lo.r as f64 + (hi.r as f64 - lo.r as f64) * t).round() as u8,
        g: (lo.g as f64 + (hi.g as f64 - lo.g as f64) * t).round() as u8,
        b: (lo.b as f64 + (hi.b as f64 - lo.b as f64) * t).round() as u8,
        a: lo.a + (hi.a - lo.a) * t as f32,
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let (color_lo, color_hi) = match spec.kind {
        ChartKind::Matrix { color_lo, color_hi } => (color_lo, color_hi),
        _ => unreachable!(),
    };

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let n_rows = spec.series.len();
    let n_cols = spec.categories.len();

    // y 軸ラベル最大幅
    let mut max_y_w = 0.0_f32;
    for s in &spec.series {
        let w = m.width(&s.name, label_font as f32);
        if w > max_y_w {
            max_y_w = w;
        }
    }
    let y_axis_w = max_y_w as f64 + 10.0;

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    let plot_left = OUTER_PAD + y_axis_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND;

    let plot_w = plot_right - plot_left;
    let plot_h = plot_bottom - plot_top;

    let cell_w = if n_cols > 0 {
        plot_w / n_cols as f64
    } else {
        plot_w
    };
    let cell_h = if n_rows > 0 {
        plot_h / n_rows as f64
    } else {
        plot_h
    };

    // min/max 収集（NaN スキップ）
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for s in &spec.series {
        for &v in &s.values {
            if v.is_finite() {
                if v < min_v {
                    min_v = v;
                }
                if v > max_v {
                    max_v = v;
                }
            }
        }
    }
    let range = if (max_v - min_v).abs() < f64::EPSILON {
        1.0
    } else {
        max_v - min_v
    };

    let mut items: Vec<Prim> = Vec::new();

    // タイトル
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

    // セル
    for (row, s) in spec.series.iter().enumerate() {
        let cell_y = plot_top + row as f64 * cell_h;
        for (col, &v) in s.values.iter().enumerate() {
            let cell_x = plot_left + col as f64 * cell_w;
            let fill = if v.is_finite() {
                lerp_color(color_lo, color_hi, (v - min_v) / range)
            } else {
                NAN_COLOR
            };
            items.push(Prim::Rect {
                x: cell_x,
                y: cell_y,
                w: cell_w,
                h: cell_h,
                fill,
            });
        }
    }

    // x 軸ラベル（各列中央下）
    for (col, label) in spec.categories.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left + col as f64 * cell_w + cell_w / 2.0,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: label.clone(),
        });
    }

    // y 軸ラベル（各行中央左、右寄せ）
    for (row, s) in spec.series.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: plot_top + row as f64 * cell_h + cell_h / 2.0 + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: s.name.clone(),
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
