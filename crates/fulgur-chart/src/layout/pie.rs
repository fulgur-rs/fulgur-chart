//! pie / doughnut チャート。軸・グリッドを持たず、タイトルと凡例(カテゴリ別)を自前で描く。

use super::common;
use crate::ir::{ChartKind, ChartSpec, Color, LegendPos};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;
use std::fmt::Write;

/// スライス境界の白線（chart.js 風）。
pub(crate) const SLICE_STROKE: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};

/// データラベルの文字色(スライス上で読めるよう白)。
pub(crate) const LABEL_COLOR: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // doughnut の内径比。
    let donut_ratio = match spec.kind {
        ChartKind::Pie { donut_ratio } => donut_ratio,
        _ => 0.0,
    };

    let series = spec.series.first();
    let empty: Vec<f64> = Vec::new();
    let values = series.map(|s| &s.values).unwrap_or(&empty);

    // 1. タイトル。
    let title_band = if spec.title.is_some() {
        common::TITLE_BAND
    } else {
        0.0
    };
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: common::OUTER_PAD + common::TITLE_FONT,
            size: common::TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // 2. 凡例(カテゴリ別)。
    let has_legend = matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.categories.iter().any(|c| !c.is_empty());
    let legend_top = if has_legend && spec.legend == LegendPos::Top {
        common::LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom {
        common::LEGEND_BAND
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(カテゴリ名から算出)。
    let legend_left = if has_legend && spec.legend == LegendPos::Left {
        common::legend_band_width_vertical(m, &spec.categories, label_font)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == LegendPos::Right {
        common::legend_band_width_vertical(m, &spec.categories, label_font)
    } else {
        0.0
    };
    if has_legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        // 各カテゴリのエントリ幅と合計（末尾の trailing 16 を最後だけ除く）。
        let mut total = 0.0_f64;
        let n = spec.categories.len();
        for (k, cat) in spec.categories.iter().enumerate() {
            total += common::legend_entry_width(m, cat, label_font);
            if k == n - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == LegendPos::Top {
            common::OUTER_PAD + title_band + common::LEGEND_BAND / 2.0
        } else {
            spec.height - common::OUTER_PAD - common::LEGEND_BAND / 2.0
        };
        let mut cursor = start_x;
        for (i, cat) in spec.categories.iter().enumerate() {
            let swatch = series.map(|s| s.fill_at(i)).unwrap_or(ink);
            items.push(Prim::Rect {
                x: cursor,
                y: legend_cy - 6.0,
                w: 12.0,
                h: 12.0,
                fill: swatch,
            });
            items.push(Prim::Text {
                x: cursor + 16.0,
                y: legend_cy + label_font * common::TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: cat.clone(),
                rotate_deg: None,
            });
            cursor += common::legend_entry_width(m, cat, label_font);
        }
    }

    // 2b. 凡例(Left/Right: 縦並び、カテゴリ別)。
    if has_legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .categories
            .iter()
            .enumerate()
            .map(|(i, cat)| {
                let swatch = series.map(|s| s.fill_at(i)).unwrap_or(ink);
                (cat.clone(), swatch)
            })
            .collect();
        let band_w = if spec.legend == LegendPos::Left {
            legend_left
        } else {
            legend_right
        };
        let band_x = if spec.legend == LegendPos::Left {
            common::OUTER_PAD
        } else {
            spec.width - common::OUTER_PAD - band_w
        };
        // 円の縦スパン(area_top..area_bottom)中央に揃える。
        let area_top = common::OUTER_PAD + title_band + legend_top;
        let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
        common::draw_vertical_legend(
            &mut items,
            &entries,
            None,
            band_x,
            area_top,
            area_bottom,
            ink,
            label_font,
        );
    }

    // 3. 円の領域。
    let area_top = common::OUTER_PAD + title_band + legend_top;
    let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
    let area_left = common::OUTER_PAD + legend_left;
    let area_right = spec.width - common::OUTER_PAD - legend_right;
    let cx = (area_left + area_right) / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let radius = ((area_right - area_left).min(area_bottom - area_top) / 2.0 * 0.9).max(0.0);
    let inner = radius * donut_ratio;

    // 4. スライス。正の有限値のみ合計。
    let total: f64 = values.iter().filter(|v| v.is_finite() && **v > 0.0).sum();
    let mut labels: Vec<Prim> = Vec::new();
    if total > 0.0 && radius > 0.0 {
        let mut a0 = -PI / 2.0; // 12 時方向。
        for (i, &v) in values.iter().enumerate() {
            if !(v.is_finite() && v > 0.0) {
                continue; // v<=0 は角度を進めずスキップ。
            }
            let frac = v / total;
            let a1 = a0 + frac * 2.0 * PI;
            let fill = series.map(|s| s.fill_at(i)).unwrap_or(ink);

            let g = Geom {
                cx,
                cy,
                r_outer: radius,
                r_inner: inner,
            };
            // 全周(単一スライス=100%)は SVG の単一 A で描けないため中点で 2 分割。
            if a1 - a0 >= 2.0 * PI - 1e-9 {
                let amid = a0 + (a1 - a0) / 2.0;
                items.push(make_slice(&g, a0, amid, fill));
                items.push(make_slice(&g, amid, a1, fill));
            } else {
                items.push(make_slice(&g, a0, a1, fill));
            }
            if spec.data_labels {
                let amid = (a0 + a1) / 2.0;
                let label_r = if inner > 0.0 {
                    (inner + radius) / 2.0
                } else {
                    radius * 0.6
                };
                labels.push(common::value_label(
                    cx + label_r * amid.cos(),
                    cy + label_r * amid.sin() + label_font * common::TEXT_BASELINE_RATIO,
                    label_font,
                    Anchor::Middle,
                    LABEL_COLOR,
                    v,
                ));
            }
            a0 = a1;
        }
    }

    // ラベルは全スライスの上に描く（最後に push）。
    items.extend(labels);

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 円スライスのジオメトリ（中心と内外半径）。
pub(crate) struct Geom {
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    pub(crate) r_outer: f64,
    pub(crate) r_inner: f64,
}

/// 1 スライス分の Path プリミティブを生成する。
pub(crate) fn make_slice(g: &Geom, a0: f64, a1: f64, fill: Color) -> Prim {
    Prim::Path {
        d: slice_path(g, a0, a1),
        fill: Some(fill),
        stroke: Some(SLICE_STROKE),
        stroke_width: 1.0,
    }
}

/// 円弧スライスの SVG path data を生成する。`a1 > a0` かつ `a1-a0 < 2π` を前提とする。
/// 角度増加方向は SVG 座標(y 下向き)で時計回り＝sweep 1。
fn slice_path(g: &Geom, a0: f64, a1: f64) -> String {
    let Geom {
        cx,
        cy,
        r_outer,
        r_inner,
    } = *g;
    let laf = if (a1 - a0) > PI { 1 } else { 0 };
    let o0 = (cx + r_outer * a0.cos(), cy + r_outer * a0.sin());
    let o1 = (cx + r_outer * a1.cos(), cy + r_outer * a1.sin());
    let mut d = String::new();
    if r_inner > 0.0 {
        // doughnut: 外弧 a0→a1 (sweep 1)、内弧 a1→a0 (sweep 0) で戻る。
        let i0 = (cx + r_inner * a0.cos(), cy + r_inner * a0.sin());
        let i1 = (cx + r_inner * a1.cos(), cy + r_inner * a1.sin());
        write!(
            d,
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
        .unwrap();
    } else {
        // pie: 中心→外周→外弧→閉じる。
        write!(
            d,
            "M {} {} L {} {} A {} {} 0 {} 1 {} {} Z",
            fmt_num(cx),
            fmt_num(cy),
            fmt_num(o0.0),
            fmt_num(o0.1),
            fmt_num(r_outer),
            fmt_num(r_outer),
            laf,
            fmt_num(o1.0),
            fmt_num(o1.1),
        )
        .unwrap();
    }
    d
}
