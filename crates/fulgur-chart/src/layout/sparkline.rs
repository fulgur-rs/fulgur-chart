//! sparkline チャート: 軸・ラベル・凡例なしのミニマル折れ線。

use super::common;
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scale::LinearScale;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

const PAD: f64 = common::OUTER_PAD;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let (domain_min, domain_max) = common::value_domain(spec, &spec.y_axis);

    // y スケール（画面上下反転）
    let ys = LinearScale::new(domain_min, domain_max, spec.height - PAD, PAD);

    let plot_left = PAD;
    let plot_right = spec.width - PAD;

    let mut items: Vec<Prim> = Vec::new();

    for ser in &spec.series {
        let count = ser.values.len();
        if count == 0 {
            continue;
        }
        // 等間隔でプロット
        let pts: Vec<(f64, f64)> = (0..count)
            .map(|i| {
                let x = plot_left + (i as f64 + 0.5) * (plot_right - plot_left) / count as f64;
                let v = ser.values[i];
                (x, ys.map(v))
            })
            .collect();

        // area（背面）
        if ser.area && pts.len() >= 2 {
            let baseline_y = ys.map(0.0_f64.clamp(domain_min, domain_max));
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

        // 折れ線
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
        // マーカーなし・データラベルなし
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

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
