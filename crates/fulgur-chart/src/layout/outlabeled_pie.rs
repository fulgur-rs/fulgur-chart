//! outlabeledPie / outlabeledDoughnut。スライス外側に引き出し線+ラベルを描く。

use super::common;
use super::pie::{Geom, make_slice};
use crate::ir::{ChartKind, ChartSpec, Color, LegendPos, OutlabelConfig};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;

/// 円半径の係数。利用可能領域のこの割合を円の直径として使う。
/// 残り 45% を引き出し線＋ラベルのスペースに使う。
const RADIUS_FACTOR: f64 = 0.55;

/// 引き出し線の水平シェルフ長(px)。
const SHELF_LEN: f64 = 20.0;

/// ラベルボックスの内側パディング(px)。
const LABEL_PAD: f64 = 3.0;

/// テキスト行間隔(px)。
const LINE_GAP: f64 = 2.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let (donut_ratio, outlabel) = match &spec.kind {
        ChartKind::OutlabeledPie { donut_ratio, outlabel } => (*donut_ratio, outlabel.clone()),
        _ => return Scene { width: spec.width, height: spec.height, items },
    };

    let series = spec.series.first();
    let empty: Vec<f64> = Vec::new();
    let values = series.map(|s| &s.values).unwrap_or(&empty);

    // 1. タイトル。
    let title_band = if spec.title.is_some() { common::TITLE_BAND } else { 0.0 };
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: common::OUTER_PAD + common::TITLE_FONT,
            size: common::TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }

    // 2. 凡例（Top/Bottom のみ対応、Left/Right は省略）。
    let has_legend = matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom
    ) && spec.categories.iter().any(|c| !c.is_empty());
    let legend_top = if has_legend && spec.legend == LegendPos::Top { common::LEGEND_BAND } else { 0.0 };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom { common::LEGEND_BAND } else { 0.0 };

    if has_legend {
        let mut total = 0.0_f64;
        let n = spec.categories.len();
        for (k, cat) in spec.categories.iter().enumerate() {
            total += common::legend_entry_width(m, cat, label_font);
            if k == n - 1 { total -= 16.0; }
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
            items.push(Prim::Rect { x: cursor, y: legend_cy - 6.0, w: 12.0, h: 12.0, fill: swatch });
            items.push(Prim::Text {
                x: cursor + 16.0,
                y: legend_cy + label_font * common::TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: cat.clone(),
            });
            cursor += common::legend_entry_width(m, cat, label_font);
        }
    }

    // 3. 円の配置。引き出し線スペースのため半径を小さくする。
    let area_top = common::OUTER_PAD + title_band + legend_top;
    let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
    let cx = spec.width / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let available = (spec.width).min(area_bottom - area_top) / 2.0;
    let radius = (available * RADIUS_FACTOR).max(0.0);
    let inner = radius * donut_ratio;

    // 4. スライス描画 → 引き出し線+ラベル。
    let total: f64 = values.iter().filter(|v| v.is_finite() && **v > 0.0).sum();
    let mut label_prims: Vec<Prim> = Vec::new();

    if total > 0.0 && radius > 0.0 {
        let mut a0 = -PI / 2.0;

        for (i, &v) in values.iter().enumerate() {
            if !(v.is_finite() && v > 0.0) { continue; }

            let frac = v / total;
            let a1 = a0 + frac * 2.0 * PI;
            let fill = series.map(|s| s.fill_at(i)).unwrap_or(ink);

            let g = Geom { cx, cy, r_outer: radius, r_inner: inner };

            // 単一スライス(100%)は SVG の arc 制約で 2 分割。
            if a1 - a0 >= 2.0 * PI - 1e-9 {
                let amid = a0 + (a1 - a0) / 2.0;
                items.push(make_slice(&g, a0, amid, fill));
                items.push(make_slice(&g, amid, a1, fill));
            } else {
                items.push(make_slice(&g, a0, a1, fill));
            }

            // 引き出し線 + ラベル。
            let amid = (a0 + a1) / 2.0;
            draw_outlabel(
                &mut label_prims,
                cx, cy, radius, amid, fill,
                i, v, frac,
                &spec.categories,
                &outlabel,
                label_font,
            );

            a0 = a1;
        }
    }

    // ラベルはスライスの上に描く。
    items.extend(label_prims);

    Scene { width: spec.width, height: spec.height, items }
}

/// 1スライス分の引き出し線とラベルを `out` に追加する。
fn draw_outlabel(
    out: &mut Vec<Prim>,
    cx: f64,
    cy: f64,
    radius: f64,
    amid: f64,
    slice_fill: Color,
    idx: usize,
    value: f64,
    frac: f64,
    categories: &[String],
    cfg: &OutlabelConfig,
    font_size: f64,
) {
    // P0: 外周上の点。
    let p0 = (cx + radius * amid.cos(), cy + radius * amid.sin());
    // P1: stretch 分だけ外側。
    let stretch_r = radius + cfg.stretch;
    let p1 = (cx + stretch_r * amid.cos(), cy + stretch_r * amid.sin());
    // P2: 水平シェルフの端点。
    let on_right = amid.cos() >= 0.0;
    let p2 = if on_right { (p1.0 + SHELF_LEN, p1.1) } else { (p1.0 - SHELF_LEN, p1.1) };

    // 引き出し線（P0 → P1 → P2）。
    out.push(Prim::Polyline {
        points: vec![p0, p1, p2],
        stroke: slice_fill,
        stroke_width: 1.5,
    });

    // テキスト生成。
    let label_str = categories.get(idx).map(|s| s.as_str()).unwrap_or("");
    let pct = (frac * 100.0).round() as i64;
    let lines: Vec<&str> = cfg.text.splitn(2, '\n').collect();
    let line1 = expand_template(lines.first().copied().unwrap_or("%l"), label_str, value, pct);
    let line2 = expand_template(lines.get(1).copied().unwrap_or("%p%"), label_str, value, pct);

    // テキスト位置。
    let (anchor, text_x) = if on_right {
        (Anchor::Start, p2.0 + LABEL_PAD)
    } else {
        (Anchor::End, p2.0 - LABEL_PAD)
    };
    let line_h = font_size + LINE_GAP;
    let text_y_top = p2.1 - line_h / 2.0;

    // ラベル背景ボックス。
    let bg_color = cfg.background.unwrap_or(slice_fill);
    let w1 = estimate_text_width(&line1, font_size);
    let w2 = estimate_text_width(&line2, font_size);
    let box_w = w1.max(w2) + LABEL_PAD * 2.0;
    let box_h = line_h * 2.0 + LABEL_PAD * 2.0;
    let box_x = if on_right { p2.0 } else { p2.0 - box_w };
    let box_y = text_y_top - LABEL_PAD;
    out.push(Prim::Rect { x: box_x, y: box_y, w: box_w, h: box_h, fill: bg_color });

    // 1行目テキスト。
    out.push(Prim::Text {
        x: text_x,
        y: text_y_top + font_size * common::TEXT_BASELINE_RATIO,
        size: font_size,
        anchor,
        fill: cfg.color,
        content: line1,
    });
    // 2行目テキスト。
    out.push(Prim::Text {
        x: text_x,
        y: text_y_top + line_h + font_size * common::TEXT_BASELINE_RATIO,
        size: font_size,
        anchor,
        fill: cfg.color,
        content: line2,
    });
}

/// `%l`, `%v`, `%p` をそれぞれカテゴリ名・値・パーセントに展開する。
fn expand_template(tmpl: &str, label: &str, value: f64, pct: i64) -> String {
    tmpl.replace("%l", label)
        .replace("%v", &fmt_num(value))
        .replace("%p", &pct.to_string())
}

/// テキスト幅の粗い見積もり（フォントメトリクスなしの近似）。
fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().count() as f64 * font_size * 0.6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::render::render_chart_with_font;

    fn make_spec(type_str: &str) -> crate::ir::ChartSpec {
        let json = format!(
            r#"{{"type":"{}","data":{{"labels":["A","B","C"],"datasets":[{{"data":[10,20,30]}}]}}}}"#,
            type_str
        );
        chartjs::parse(&json, false).expect("parse error")
    }

    #[test]
    fn outlabeled_pie_renders_to_svg() {
        let spec = make_spec("outlabeledPie");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"), "should produce valid SVG");
    }

    #[test]
    fn outlabeled_doughnut_renders_to_svg() {
        let spec = make_spec("outlabeledDoughnut");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"), "should produce valid SVG");
    }

    #[test]
    fn outlabeled_pie_has_text_primitives() {
        let spec = make_spec("outlabeledPie");
        let scene = build(&spec, &crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap());
        let has_text = scene.items.iter().any(|p| matches!(p, crate::scene::Prim::Text { .. }));
        assert!(has_text, "scene must contain Text primitives for labels");
    }

    #[test]
    fn outlabeled_pie_single_slice_renders() {
        let json = r#"{"type":"outlabeledPie","data":{"labels":["Only"],"datasets":[{"data":[100]}]}}"#;
        let spec = chartjs::parse(json, false).expect("parse error");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn outlabeled_doughnut_inner_radius_nonzero() {
        let spec = make_spec("outlabeledDoughnut");
        let scene = build(&spec, &crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap());
        let has_doughnut_path = scene.items.iter().any(|p| {
            if let crate::scene::Prim::Path { d, .. } = p {
                // doughnut の path には内弧 (sweep=0) が含まれる: "0 0 " パターン
                d.contains("0 0 ")
            } else {
                false
            }
        });
        assert!(has_doughnut_path, "doughnut must have inner arc paths");
    }
}
