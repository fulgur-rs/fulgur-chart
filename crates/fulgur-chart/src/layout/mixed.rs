//! 混合チャート(bar+line): 共有のカテゴリ x・線形 y 軸に棒系列と折れ線系列を重ねる。
//! frame は common::compute / common::draw_frame を共有する(byte 一致のため bar/line は不変)。
//! 棒/折れ線の幾何定数とヘルパは bar.rs / line.rs から複製している(意図的な重複)。

use super::common;
use crate::ir::{ChartSpec, SeriesType};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

// --- bar.rs から複製した縦棒の幾何定数 ---
/// band 内のグループ幅比。
const GROUP_RATIO: f64 = 0.8;
/// band 左右パディング比。
const BAND_PAD_RATIO: f64 = 0.1;
/// bar 幅の塗り比。
const BAR_FILL_RATIO: f64 = 0.9;

// --- line.rs から複製した折れ線の定数 ---
/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 共有フレーム(カテゴリ x・全系列 values からの y ドメイン)。
    let frame = common::compute(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);

    // 棒系列の本数(スロット分割の分母)。
    let bar_count = spec
        .series
        .iter()
        .filter(|s| s.series_type == SeriesType::Bar)
        .count();

    // --- 1. 棒系列(背面) ---
    if bar_count > 0 {
        let band_w = common::band_width(&frame, n);
        let group_w = band_w * GROUP_RATIO;
        let bar_w = group_w / bar_count as f64;
        let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
        let baseline_y = frame.ys.map(base_v);

        for i in 0..spec.categories.len() {
            let band_left = common::category_center(&frame, i, n) - band_w / 2.0;
            // 棒系列のスロット番号(全系列ではなく棒系列内の位置)。
            let mut bar_slot = 0_usize;
            for ser in &spec.series {
                if ser.series_type != SeriesType::Bar {
                    continue;
                }
                let bx = band_left + band_w * BAND_PAD_RATIO + bar_slot as f64 * bar_w;
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
                    let label_y = if v >= base_v {
                        y_top - common::LABEL_GAP
                    } else {
                        y_top + h + label_font
                    };
                    items.push(common::value_label(
                        cx,
                        label_y,
                        label_font,
                        Anchor::Middle,
                        ink,
                        v,
                    ));
                }
                bar_slot += 1;
            }
        }
    }

    // --- 2. 折れ線系列(前面) ---
    for ser in &spec.series {
        if ser.series_type != SeriesType::Line {
            continue;
        }
        // 点列: カテゴリ中心 → (x, y)。
        let pts: Vec<(f64, f64)> = (0..spec.categories.len())
            .map(|i| {
                let x = common::category_center(&frame, i, n);
                let v = ser.values.get(i).copied().unwrap_or(0.0);
                (x, frame.ys.map(v))
            })
            .collect();

        // area（背面・半透明）。
        if ser.area && !pts.is_empty() {
            let baseline_y = frame
                .ys
                .map(0.0_f64.clamp(frame.ticks.min, frame.ticks.max));
            let mut d = String::new();
            for (k, (x, y)) in pts.iter().enumerate() {
                let cmd = if k == 0 { 'M' } else { 'L' };
                write!(d, "{} {} {} ", cmd, fmt_num(*x), fmt_num(*y)).unwrap();
            }
            let (last_x, _) = pts[pts.len() - 1];
            let (first_x, _) = pts[0];
            write!(
                d,
                "L {} {} L {} {} Z",
                fmt_num(last_x),
                fmt_num(baseline_y),
                fmt_num(first_x),
                fmt_num(baseline_y)
            )
            .unwrap();
            items.push(Prim::Path {
                d,
                fill: Some(ser.fill_at(0)),
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // 線。
        if pts.len() >= 2 {
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline {
                    points: pts.clone(),
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                let d = catmull_rom_path(&pts, ser.tension);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }

        // マーカー。
        for (cx, cy) in &pts {
            items.push(Prim::Circle {
                cx: *cx,
                cy: *cy,
                r: MARKER_R,
                fill: ser.stroke_at(0),
                stroke: ser.stroke_at(0),
                stroke_width: 0.0,
            });
        }

        // データラベル(点の上)。
        if spec.data_labels {
            for (i, (x, y)) in pts.iter().enumerate() {
                if let Some(&v) = ser.values.get(i) {
                    if v.is_finite() {
                        items.push(common::value_label(
                            *x,
                            *y - MARKER_R - common::LABEL_GAP,
                            label_font,
                            Anchor::Middle,
                            spec.theme.text_color,
                            v,
                        ));
                    }
                }
            }
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する(line.rs から複製)。
/// 端点は自身を複製して扱う。`pts.len() >= 2` を前提とする。
fn catmull_rom_path(pts: &[(f64, f64)], tension: f64) -> String {
    let k = pts.len();
    let mut d = String::new();
    write!(d, "M {} {} ", fmt_num(pts[0].0), fmt_num(pts[0].1)).unwrap();
    for i in 0..k - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(k - 1)];
        let cp1 = (
            p1.0 + (p2.0 - p0.0) / 6.0 * tension,
            p1.1 + (p2.1 - p0.1) / 6.0 * tension,
        );
        let cp2 = (
            p2.0 - (p3.0 - p1.0) / 6.0 * tension,
            p2.1 - (p3.1 - p1.1) / 6.0 * tension,
        );
        write!(
            d,
            "C {} {} {} {} {} {} ",
            fmt_num(cp1.0),
            fmt_num(cp1.1),
            fmt_num(cp2.0),
            fmt_num(cp2.1),
            fmt_num(p2.0),
            fmt_num(p2.1)
        )
        .unwrap();
    }
    d
}
