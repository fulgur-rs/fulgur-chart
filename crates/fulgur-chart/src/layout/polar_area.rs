//! polarArea チャート。角度等分・半径が値に比例する極座標チャート。

use super::common;
use super::pie::{Geom, LABEL_COLOR, make_slice};
use crate::ir::{ChartSpec, LegendPos};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

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

    // 2. 凡例(カテゴリ別、pie.rs と同構造)。
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
    if has_legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, crate::ir::Color)> = spec
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
        let area_top = common::OUTER_PAD + title_band + legend_top;
        let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
        common::draw_vertical_legend(
            &mut items,
            &entries,
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
    let max_radius = ((area_right - area_left).min(area_bottom - area_top) / 2.0 * 0.9).max(0.0);

    // 4. polarArea スライス: 角度等分、半径は値に比例。
    let n = values.len();
    if n == 0 || max_radius <= 0.0 {
        return Scene {
            width: spec.width,
            height: spec.height,
            items,
        };
    }

    let angle_per = 2.0 * PI / n as f64;
    // 既存 default パス (radial_axis == None) 用の max_v は byte-identical を維持するため
    // 既存の v > 0 フィルタを保つ。
    let max_v = values
        .iter()
        .filter(|v| v.is_finite() && **v > 0.0)
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    // ドメイン [lo, hi] を解決。radial_axis 有り → override、無し → 既存 [0, max_v]。
    // 既存 default path (radial_axis == None) は byte-identical を維持。
    let (lo, hi) = if let Some(ra) = &spec.radial_axis {
        // Codex Fix 8: radial_axis ブランチでは data_min / data_max を「全 finite 値」から求める。
        // v > 0 フィルタだと min: -10, data: [0] のケースで max_v = -inf になり hi が壊れる。
        let mut data_min = f64::INFINITY;
        let mut data_max = f64::NEG_INFINITY;
        for &v in values.iter() {
            if v.is_finite() {
                if v < data_min {
                    data_min = v;
                }
                if v > data_max {
                    data_max = v;
                }
            }
        }
        // Codex Fix 6: beginAtZero=false かつ min 未指定なら lo は data_min から。
        // (chart.js の semantics: beginAtZero: false はドメインをデータ範囲密着にする)
        let mut lo = ra.min.unwrap_or_else(|| {
            if !ra.begin_at_zero && data_min.is_finite() {
                data_min
            } else {
                0.0
            }
        });
        let mut hi = ra.max.unwrap_or(data_max);
        if let Some(s) = ra.suggested_min
            && s < lo
        {
            lo = s;
        }
        if let Some(s) = ra.suggested_max
            && s > hi
        {
            hi = s;
        }
        if ra.begin_at_zero && ra.min.is_none() {
            lo = lo.min(0.0);
        }
        // Fixes 1 + 4 (coderabbit + gemini): 縮退時 (hi <= lo, NaN, inf) は 1 ユニット救済。
        // radar.rs:204-206 と同一パターン。
        if !hi.is_finite() || hi <= lo {
            hi = lo + 1.0;
        }
        (lo, hi)
    } else {
        (0.0, max_v)
    };
    let span = hi - lo;

    let mut labels: Vec<Prim> = Vec::new();

    if span.is_finite() && span > 0.0 {
        let mut a0 = -PI / 2.0;
        for (i, &v) in values.iter().enumerate() {
            let a1 = a0 + angle_per;
            // radial_axis 無しの場合は既存挙動: v > 0 のみ描く (max_v > 0 が
            // 外側条件で保証されている; v/max_v == (v-0)/(max_v-0))。
            // 有りの場合は下限クランプ + 上限クランプ。
            let r = if v.is_finite() {
                let ratio = if spec.radial_axis.is_some() {
                    ((v - lo) / span).clamp(0.0, 1.0)
                } else if v > 0.0 {
                    (v / hi).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                max_radius * ratio
            } else {
                0.0
            };

            if r > 0.0 {
                let fill = series.map(|s| s.fill_at(i)).unwrap_or(ink);
                let g = Geom {
                    cx,
                    cy,
                    r_outer: r,
                    r_inner: 0.0,
                };
                if angle_per >= 2.0 * PI - 1e-9 {
                    let amid = a0 + angle_per / 2.0;
                    items.push(make_slice(&g, a0, amid, fill));
                    items.push(make_slice(&g, amid, a1, fill));
                } else {
                    items.push(make_slice(&g, a0, a1, fill));
                }

                if spec.data_labels {
                    let amid = (a0 + a1) / 2.0;
                    let label_r = r * 0.6;
                    labels.push(common::value_label(
                        cx + label_r * amid.cos(),
                        cy + label_r * amid.sin() + label_font * common::TEXT_BASELINE_RATIO,
                        label_font,
                        Anchor::Middle,
                        LABEL_COLOR,
                        v,
                    ));
                }
            }
            a0 = a1;
        }
    }

    items.extend(labels);

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
