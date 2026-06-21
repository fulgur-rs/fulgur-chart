//! gauge / radialGauge チャートのレイアウト: ChartSpec → Scene。
//! 軸なし。決定的に組み立て、NaN/Inf/panic を出さない。
//! すべての弧は standalone な空白区切り M/L/A/Z トークンで生成する
//! (raster_direct::parse_path_data 不変条件。pie.rs / progress.rs と同様)。

use super::common::{OUTER_PAD, TITLE_FONT};
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
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

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 内外半径ありの円弧帯(リングセグメント)の SVG path data。
/// a0→a1 を外弧(sweep 1)、a1→a0 を内弧(sweep 0)で閉じる。pie の doughnut と同形。
/// `a1 > a0` かつ `a1-a0 <= 2π` を前提(呼び出し側で保証)。
/// すべて fmt_num 整形 + 空白区切り(raster_direct 不変条件)。
// Task 3+ の build_radial/build_semi から使用する(本タスクでは tests のみが使用)。
#[allow(dead_code)]
fn ring_segment_path(cx: f64, cy: f64, r_outer: f64, r_inner: f64, a0: f64, a1: f64) -> String {
    let laf = if (a1 - a0) > PI { 1 } else { 0 };
    let o0 = (cx + r_outer * a0.cos(), cy + r_outer * a0.sin());
    let o1 = (cx + r_outer * a1.cos(), cy + r_outer * a1.sin());
    let i0 = (cx + r_inner * a0.cos(), cy + r_inner * a0.sin());
    let i1 = (cx + r_inner * a1.cos(), cy + r_inner * a1.sin());
    format!(
        "M {} {} A {} {} 0 {} 1 {} {} L {} {} A {} {} 0 {} 0 {} {} Z",
        fmt_num(o0.0),
        fmt_num(o0.1),
        fmt_num(r_outer),
        fmt_num(r_outer),
        laf,
        fmt_num(o1.0),
        fmt_num(o1.1),
        fmt_num(i1.0),
        fmt_num(i1.1),
        fmt_num(r_inner),
        fmt_num(r_inner),
        laf,
        fmt_num(i0.0),
        fmt_num(i0.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn ring_segment_path_is_closed_and_clean() {
        let d = ring_segment_path(100.0, 100.0, 80.0, 40.0, -PI / 2.0, 0.0);
        assert!(d.starts_with('M'), "must start with moveto: {d}");
        assert!(d.ends_with('Z'), "must close: {d}");
        assert!(!d.contains("NaN") && !d.contains("inf"), "{d}");
    }

    #[test]
    fn ring_segment_path_uses_standalone_command_tokens() {
        // PNG 用 raster_direct::parse_path_data は split_ascii_whitespace で
        // トークン化し、スタンドアロンの M/L/A/Z しか解釈しない。
        let d = ring_segment_path(100.0, 100.0, 80.0, 40.0, -PI / 2.0, PI / 2.0);
        let tokens: Vec<&str> = d.split_ascii_whitespace().collect();
        assert!(tokens.contains(&"M"), "{d}");
        assert_eq!(tokens.iter().filter(|t| **t == "A").count(), 2, "{d}");
        assert_eq!(tokens.iter().filter(|t| **t == "Z").count(), 1, "{d}");
    }

    #[test]
    fn ring_segment_path_deterministic() {
        let a = ring_segment_path(1.0, 2.0, 50.0, 25.0, 0.0, 1.0);
        let b = ring_segment_path(1.0, 2.0, 50.0, 25.0, 0.0, 1.0);
        assert_eq!(a, b);
    }
}
