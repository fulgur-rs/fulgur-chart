//! progress チャートのレイアウト: ChartSpec → Scene。
//! 軸なしの水平塗りつぶしバー。決定的に組み立て、NaN/Inf/panic を出さない。

use super::common::{OUTER_PAD, TITLE_BAND, TITLE_FONT};
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

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
    let _ = TITLE_BAND; // 後続タスクで使用

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
