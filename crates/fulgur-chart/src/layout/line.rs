//! line / area チャート。共有フレーム(common)の上に折れ線・面・マーカーを重ねる。

use super::common;
use crate::font::DEFAULT_FONT;
use crate::ir::{ChartSpec, Color};
use crate::num::fmt_num;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

/// area 塗りの不透明度（系列 fill の alpha に乗ずる）。
const AREA_FILL_ALPHA: f32 = 0.3;
/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

pub fn build(spec: &ChartSpec) -> Scene {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(spec, &m);

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, &m);

    let n = spec.categories.len().max(1);

    for ser in &spec.series {
        // 点列: カテゴリ位置 → (x, y)。
        let pts: Vec<(f64, f64)> = (0..spec.categories.len())
            .map(|i| {
                let x = common::category_center(&frame, i, n);
                let v = ser.values.get(i).copied().unwrap_or(0.0);
                (x, frame.ys.map(v))
            })
            .collect();

        // area（背面）。
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
            let f = ser.fill_at(0);
            let area_fill = Color {
                a: f.a * AREA_FILL_ALPHA,
                ..f
            };
            items.push(Prim::Path {
                d,
                fill: Some(area_fill),
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
            });
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する。
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
