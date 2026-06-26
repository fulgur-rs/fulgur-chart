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
    let max_v = values
        .iter()
        .filter(|v| v.is_finite() && **v > 0.0)
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let mut labels: Vec<Prim> = Vec::new();

    if max_v.is_finite() && max_v > 0.0 {
        let mut a0 = -PI / 2.0;
        for (i, &v) in values.iter().enumerate() {
            let a1 = a0 + angle_per;
            let r = if v.is_finite() && v > 0.0 {
                (max_radius * (v / max_v)).clamp(0.0, max_radius)
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
