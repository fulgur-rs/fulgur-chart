//! progress チャートのレイアウト: ChartSpec → Scene。
//! 軸なしの水平塗りつぶしバー。決定的に組み立て、NaN/Inf/panic を出さない。

use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartSpec, Color};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// トラック（背景）の淡灰色。
const TRACK_COLOR: Color = Color {
    r: 224,
    g: 224,
    b: 224,
    a: 1.0,
};
/// バンド高に対するバー高の比。
const BAR_HEIGHT_RATIO: f64 = 0.6;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // series[0] が各バーの値。無ければ空。
    let values: &[f64] = spec
        .series
        .first()
        .map(|s| s.values.as_slice())
        .unwrap_or(&[]);
    let n = values.len();

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    // 左ラベル帯: バー名(categories)の最大幅。全て空なら 0。
    let mut max_label_w = 0.0_f32;
    for name in &spec.categories {
        if !name.is_empty() {
            let w = m.width(name, label_font as f32);
            if w > max_label_w {
                max_label_w = w;
            }
        }
    }
    let label_band = if max_label_w > 0.0 {
        max_label_w as f64 + 10.0
    } else {
        0.0
    };

    let plot_left = OUTER_PAD + label_band;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD;
    let plot_w = (plot_right - plot_left).max(0.0);
    let plot_h = (plot_bottom - plot_top).max(0.0);

    let mut items: Vec<Prim> = Vec::new();

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

    if n == 0 {
        return Scene {
            width: spec.width,
            height: spec.height,
            items,
        };
    }

    let band_h = plot_h / n as f64;
    let bar_h = (band_h * BAR_HEIGHT_RATIO).max(0.0);

    for (i, &v) in values.iter().enumerate() {
        let band_top = plot_top + i as f64 * band_h;
        let bar_y = band_top + (band_h - bar_h) / 2.0;
        let center_y = band_top + band_h / 2.0;

        // バー名（左・右寄せ）。categories[i] があり非空のときのみ。
        if let Some(name) = spec.categories.get(i) {
            if !name.is_empty() {
                items.push(Prim::Text {
                    x: plot_left - 6.0,
                    y: center_y + label_font * TEXT_BASELINE_RATIO,
                    size: label_font,
                    anchor: Anchor::End,
                    fill: ink,
                    content: name.clone(),
                });
            }
        }

        // per-bar max: series.get(1).values[i]。非有限/≤0 は 100。
        let max_i = spec
            .series
            .get(1)
            .and_then(|s| s.values.get(i).copied())
            .filter(|mx| mx.is_finite() && *mx > 0.0)
            .unwrap_or(100.0);

        let frac = if v.is_finite() {
            (v / max_i).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // トラック（角丸・全幅）
        let track_r = (bar_h / 2.0).min(plot_w / 2.0);
        items.push(Prim::Path {
            d: rounded_rect_path(plot_left, bar_y, plot_w, bar_h, track_r),
            fill: Some(TRACK_COLOR),
            stroke: None,
            stroke_width: 0.0,
        });

        // 前景（角丸・幅 = frac × 全幅）。0 幅は描かない。
        let fg_w = plot_w * frac;
        if fg_w > 0.0 {
            let fg_r = (bar_h / 2.0).min(fg_w / 2.0);
            items.push(Prim::Path {
                d: rounded_rect_path(plot_left, bar_y, fg_w, bar_h, fg_r),
                fill: Some(spec.series[0].fill_at(i)),
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // パーセンテージ（バー中央・整数%に丸め）。
        if spec.data_labels {
            let pct = frac * 100.0;
            items.push(Prim::Text {
                x: plot_left + plot_w / 2.0,
                y: center_y + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Middle,
                fill: ink,
                content: format!("{}%", fmt_num(pct.round())),
            });
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 角丸矩形の SVG path data。半径は w/2, h/2, 0 でクランプして破綻を防ぐ。
/// すべての座標を `fmt_num` で整形し決定的に出力する（Prim::Path の d 規約に準拠）。
fn rounded_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.max(0.0).min(w / 2.0).min(h / 2.0);
    let x1 = x + w;
    let y1 = y + h;
    format!(
        "M{} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}Z",
        fmt_num(x + r),
        fmt_num(y),
        fmt_num(x1 - r),
        fmt_num(y),
        fmt_num(r),
        fmt_num(r),
        fmt_num(x1),
        fmt_num(y + r),
        fmt_num(x1),
        fmt_num(y1 - r),
        fmt_num(r),
        fmt_num(r),
        fmt_num(x1 - r),
        fmt_num(y1),
        fmt_num(x + r),
        fmt_num(y1),
        fmt_num(r),
        fmt_num(r),
        fmt_num(x),
        fmt_num(y1 - r),
        fmt_num(x),
        fmt_num(y + r),
        fmt_num(r),
        fmt_num(r),
        fmt_num(x + r),
        fmt_num(y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_path_is_closed_and_clean() {
        let d = rounded_rect_path(10.0, 20.0, 100.0, 30.0, 15.0);
        assert!(d.starts_with('M'), "must start with moveto: {d}");
        assert!(d.ends_with('Z'), "must close: {d}");
        assert!(d.matches('A').count() == 4, "4 corner arcs: {d}");
        assert!(!d.contains("NaN") && !d.contains("inf"), "{d}");
    }

    #[test]
    fn rounded_rect_path_clamps_radius() {
        // 半径が w/2, h/2 を超えても破綻しない（幅 4 → r は 2 にクランプ）
        let d = rounded_rect_path(0.0, 0.0, 4.0, 30.0, 15.0);
        assert!(!d.contains("NaN") && d.ends_with('Z'), "{d}");
    }

    #[test]
    fn rounded_rect_path_deterministic() {
        let a = rounded_rect_path(1.0, 2.0, 50.0, 12.0, 6.0);
        let b = rounded_rect_path(1.0, 2.0, 50.0, 12.0, 6.0);
        assert_eq!(a, b);
    }
}
