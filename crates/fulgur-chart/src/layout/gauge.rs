//! gauge / radialGauge チャートのレイアウト: ChartSpec → Scene。
//! 軸なし。決定的に組み立て、NaN/Inf/panic を出さない。
//! すべての弧は standalone な空白区切り M/L/A/Z トークンで生成する
//! (raster_direct::parse_path_data 不変条件。pie.rs / progress.rs と同様)。

use super::common::{OUTER_PAD, TITLE_FONT};
use crate::ir::{ChartKind, ChartSpec, Color};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;

/// 中央値テキストのフォント倍率(ラベル基準フォントに対して大きめに表示)。
const CENTER_VALUE_FONT_SCALE: f64 = 1.6;
/// 中央値テキストのフォントサイズ(内径に対する比)。QuickChart 風に大きく見せる。
const CENTER_VALUE_RADIUS_RATIO: f64 = 0.45;

/// 半円 gauge の内径比(cutoutPercentage 50)。
const SEMI_CUTOUT_RATIO: f64 = 0.5;
/// 針の長さ(半径比, lengthPercentage 80)。
const NEEDLE_LEN_RATIO: f64 = 0.8;
/// 針の支点での半幅(半径比, widthPercentage 3.2)。
const NEEDLE_HALF_WIDTH_RATIO: f64 = 0.032;
/// 針支点の円の半径(半径比, radiusPercentage 2 は直径基準なので 0.04)。
const NEEDLE_PIVOT_RATIO: f64 = 0.04;
/// 値ラベルの内側パディング(px)。
const LABEL_PAD: f64 = 5.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let mut items: Vec<Prim> = Vec::new();

    let title_band = if spec.title.is_some() {
        super::common::TITLE_BAND
    } else {
        0.0
    };
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

    match &spec.kind {
        ChartKind::RadialGauge {
            min,
            max,
            track,
            inner_ratio,
            rounded,
            display_text,
            center_font_size,
        } => build_radial(
            &mut items,
            spec,
            title_band,
            *min,
            *max,
            *track,
            *inner_ratio,
            *rounded,
            *display_text,
            *center_font_size,
        ),
        ChartKind::Gauge {
            value,
            min,
            needle,
            label,
            label_color,
            label_bg,
        } => build_semi(
            &mut items,
            spec,
            m,
            title_band,
            *value,
            *min,
            *needle,
            *label,
            *label_color,
            *label_bg,
        ),
        _ => {}
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// プロット領域の中心と半径(タイトル帯を除いた領域に内接)。
fn area_geom(spec: &ChartSpec, title_band: f64) -> (f64, f64, f64) {
    let area_top = OUTER_PAD + title_band;
    let area_bottom = spec.height - OUTER_PAD;
    let area_left = OUTER_PAD;
    let area_right = spec.width - OUTER_PAD;
    let cx = (area_left + area_right) / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let r = ((area_right - area_left).min(area_bottom - area_top) / 2.0 * 0.9).max(0.0);
    (cx, cy, r)
}

#[allow(clippy::too_many_arguments)]
fn build_radial(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    title_band: f64,
    min: f64,
    max: f64,
    track: Color,
    inner_ratio: f64,
    rounded: bool,
    display_text: bool,
    center_font_size: Option<f64>,
) {
    let (cx, cy, r_outer) = area_geom(spec, title_band);
    let r_inner = (r_outer * inner_ratio).clamp(0.0, r_outer);
    if r_outer <= 0.0 {
        return;
    }
    let fill = spec.series.first().map(|s| s.fill_at(0)).unwrap_or(track);
    let value = spec
        .series
        .first()
        .and_then(|s| s.values.first().copied())
        .unwrap_or(min);

    // 値の割合(domain でスケール・クランプ)。range が 0 のとき 0。
    let frac = if (max - min).abs() > f64::EPSILON {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let start = -PI / 2.0; // 12 時。
    // トラック: 全周リング。全周は単一 A で描けないため中点で 2 分割。
    let mid = start + PI;
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, start, mid),
        fill: Some(track),
        stroke: None,
        stroke_width: 0.0,
    });
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, mid, start + 2.0 * PI),
        fill: Some(track),
        stroke: None,
        stroke_width: 0.0,
    });

    // 値弧: start から時計回りに frac×360°。frac>0 のみ。
    if frac > 0.0 {
        let end = start + frac * 2.0 * PI;
        // 半周超は単一 A で描けるが large-arc-flag は ring_segment_path が処理する。
        // 全周(frac==1)は 2 分割。
        if frac >= 1.0 - 1e-9 {
            let amid = start + PI;
            push_value_arc(items, cx, cy, r_outer, r_inner, start, amid, fill, false);
            push_value_arc(
                items,
                cx,
                cy,
                r_outer,
                r_inner,
                amid,
                start + 2.0 * PI,
                fill,
                false,
            );
        } else {
            push_value_arc(items, cx, cy, r_outer, r_inner, start, end, fill, rounded);
        }
    }

    if display_text {
        // centerArea.fontSize 指定時はそれを優先、未指定は内径比で自動算出。
        let size = center_font_size.unwrap_or_else(|| {
            (r_inner * CENTER_VALUE_RADIUS_RATIO)
                .max(spec.theme.font_size * CENTER_VALUE_FONT_SCALE)
        });
        items.push(Prim::Text {
            x: cx,
            y: cy + size * super::common::TEXT_BASELINE_RATIO,
            size,
            anchor: Anchor::Middle,
            fill: spec.theme.text_color,
            content: fmt_num(value.round()),
        });
    }
}

/// 値弧を描く。rounded のとき両端に直径=帯幅の円を置き、線端を丸く見せる。
#[allow(clippy::too_many_arguments)]
fn push_value_arc(
    items: &mut Vec<Prim>,
    cx: f64,
    cy: f64,
    r_outer: f64,
    r_inner: f64,
    a0: f64,
    a1: f64,
    fill: Color,
    rounded: bool,
) {
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, a0, a1),
        fill: Some(fill),
        stroke: None,
        stroke_width: 0.0,
    });
    if rounded {
        let cap_r = (r_outer - r_inner) / 2.0;
        let mid_r = (r_outer + r_inner) / 2.0;
        for a in [a0, a1] {
            items.push(Prim::Circle {
                cx: cx + mid_r * a.cos(),
                cy: cy + mid_r * a.sin(),
                r: cap_r.max(0.0),
                fill,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_semi(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    m: &TextMeasurer,
    title_band: f64,
    value: f64,
    min: f64,
    needle: Color,
    label: bool,
    label_color: Color,
    label_bg: Color,
) {
    let font = spec.theme.font_size;
    // 値ラベル帯(底部に確保)。label が無効なら 0。
    let label_band = if label {
        font + LABEL_PAD * 2.0 + 4.0
    } else {
        0.0
    };

    let area_top = OUTER_PAD + title_band;
    let area_bottom = spec.height - OUTER_PAD;
    let area_left = OUTER_PAD;
    let area_right = spec.width - OUTER_PAD;
    let cx = (area_left + area_right) / 2.0;
    // 半円の支点(底辺)は値ラベル帯の上に置く。
    let cy = area_bottom - label_band;
    let r_outer = (((area_right - area_left) / 2.0).min(cy - area_top) * 0.9).max(0.0);
    let r_inner = r_outer * SEMI_CUTOUT_RATIO; // cutout 50%。
    if r_outer <= 0.0 {
        return;
    }

    let series = spec.series.first();
    let thresholds: &[f64] = series.map(|s| s.values.as_slice()).unwrap_or(&[]);
    if thresholds.is_empty() {
        return;
    }
    // max = 閾値末尾(有限)。min との縮退は range=1 で防御。
    let max = thresholds
        .iter()
        .rev()
        .find(|v| v.is_finite())
        .copied()
        .unwrap_or(min + 1.0);
    let range = if (max - min).abs() > f64::EPSILON {
        max - min
    } else {
        1.0
    };
    let angle = |frac: f64| PI + frac.clamp(0.0, 1.0) * PI;

    // ゾーン: 各閾値境界を角度に変換し帯を塗る。
    let mut lo = min;
    for (i, &thr) in thresholds.iter().enumerate() {
        if !thr.is_finite() {
            continue;
        }
        let hi = thr;
        let a0 = angle((lo - min) / range);
        let a1 = angle((hi - min) / range);
        if a1 > a0 {
            let fill = series.map(|s| s.fill_at(i)).unwrap_or(needle);
            items.push(Prim::Path {
                d: ring_segment_path(cx, cy, r_outer, r_inner, a0, a1),
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
            });
        }
        lo = hi;
    }

    // 針の値も非有限なら min に倒す(ゾーンループの is_finite スキップと対称)。
    let value = if value.is_finite() { value } else { min };

    // 針: 支点から value 角へ向かう三角形 + 支点の小円。
    let va = angle((value - min) / range);
    let needle_len = r_outer * NEEDLE_LEN_RATIO;
    let tip = (cx + needle_len * va.cos(), cy + needle_len * va.sin());
    // 支点で針幅を取るための直交方向。
    let half_w = (r_outer * NEEDLE_HALF_WIDTH_RATIO).max(1.5); // widthPercentage 3.2。
    let perp = va + PI / 2.0;
    let base1 = (cx + half_w * perp.cos(), cy + half_w * perp.sin());
    let base2 = (cx - half_w * perp.cos(), cy - half_w * perp.sin());
    items.push(Prim::Path {
        d: format!(
            "M {} {} L {} {} L {} {} Z",
            fmt_num(tip.0),
            fmt_num(tip.1),
            fmt_num(base1.0),
            fmt_num(base1.1),
            fmt_num(base2.0),
            fmt_num(base2.1),
        ),
        fill: Some(needle),
        stroke: None,
        stroke_width: 0.0,
    });
    items.push(Prim::Circle {
        cx,
        cy,
        r: (r_outer * NEEDLE_PIVOT_RATIO).max(2.0), // radiusPercentage 2。
        fill: needle,
    });

    if label {
        let text = fmt_num(value.round());
        // ラベル幅は実測し、描画領域を超えないようにクランプ(狭い chart / 長い桁数で
        // box が viewBox 外に出ないようにする)。
        let text_w = m.width(&text, font as f32) as f64;
        let max_box_w = (area_right - area_left).max(0.0);
        let box_w = (text_w + LABEL_PAD * 2.0).min(max_box_w);
        let box_h = font + LABEL_PAD * 2.0;
        let box_x = (cx - box_w / 2.0).clamp(area_left, (area_right - box_w).max(area_left));
        let box_y = cy + (label_band - box_h) / 2.0; // 予約帯の中央(支点の直下、画面内)
        items.push(Prim::Path {
            d: crate::layout::progress::rounded_rect_path(box_x, box_y, box_w, box_h, 5.0),
            fill: Some(label_bg),
            stroke: None,
            stroke_width: 0.0,
        });
        items.push(Prim::Text {
            x: box_x + box_w / 2.0,
            y: box_y + box_h / 2.0 + font * super::common::TEXT_BASELINE_RATIO,
            size: font,
            anchor: Anchor::Middle,
            fill: label_color,
            content: text,
        });
    }
}

/// 内外半径ありの円弧帯(リングセグメント)の SVG path data。
/// a0→a1 を外弧(sweep 1)、a1→a0 を内弧(sweep 0)で閉じる。pie の doughnut と同形。
/// `a1 > a0` かつ `a1-a0 <= 2π` を前提(呼び出し側で保証)。
/// すべて fmt_num 整形 + 空白区切り(raster_direct 不変条件)。
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
