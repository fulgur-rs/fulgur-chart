//! BoxPlot チャートのレイアウト。

use crate::ir::ChartSpec;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

const BOX_RATIO: f64 = 0.6;
const CAP_RATIO: f64 = 0.4;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    // 1. Yドメインをbox_pointsから収集する
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    for ser in &spec.series {
        for bp in &ser.box_points {
            if bp.min.is_finite() && bp.min < data_min { data_min = bp.min; }
            if bp.max.is_finite() && bp.max > data_max { data_max = bp.max; }
        }
    }

    // 2. ドメインヒントを注入したspecクローンでcommon::computeを再利用する。
    //    Series.valuesが空のためvalue_domainがsuggested_min/maxにフォールバックする。
    let mut aux = spec.clone();
    if data_min.is_finite() {
        aux.y_axis.suggested_min = Some(data_min);
    }
    if data_max.is_finite() {
        aux.y_axis.suggested_max = Some(data_max);
    }
    aux.y_axis.begin_at_zero = false;

    let frame = super::common::compute(&aux, m);

    let mut items: Vec<Prim> = Vec::new();
    super::common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);
    let s = spec.series.len().max(1);
    let band_w = super::common::band_width(&frame, n);
    let box_w = (band_w * BOX_RATIO / s as f64).max(1.0);
    let cap_w = box_w * CAP_RATIO;
    let stroke_w = 1.5;
    let median_w = 2.5;

    for i in 0..spec.categories.len() {
        let band_cx = super::common::category_center(&frame, i, n);
        let group_offset = -(s as f64 - 1.0) / 2.0;

        for (sidx, ser) in spec.series.iter().enumerate() {
            let Some(bp) = ser.box_points.get(i) else { continue };
            if !bp.min.is_finite() || !bp.max.is_finite() { continue }

            let cx = band_cx + (group_offset + sidx as f64) * box_w;
            let left = cx - box_w / 2.0;

            let y_q1     = frame.ys.map(bp.q1);
            let y_q3     = frame.ys.map(bp.q3);
            let y_median = frame.ys.map(bp.median);
            let y_min    = frame.ys.map(bp.min);
            let y_max    = frame.ys.map(bp.max);

            // Y軸は上が小さい(screen coords)のでQ3のy座標が小さい(上)
            let box_top    = y_q3.min(y_q1);
            let box_bottom = y_q3.max(y_q1);
            let fill   = ser.fill_at(i);
            let stroke = ser.stroke_at(i);

            // ボックス本体(Q1–Q3)
            items.push(Prim::Rect {
                x: left, y: box_top, w: box_w, h: (box_bottom - box_top).max(1.0), fill,
            });

            // ボックス枠(4本のLine)
            for &(x1, y1, x2, y2) in &[
                (left,          box_top,    left + box_w, box_top),
                (left + box_w,  box_top,    left + box_w, box_bottom),
                (left + box_w,  box_bottom, left,         box_bottom),
                (left,          box_bottom, left,         box_top),
            ] {
                items.push(Prim::Line { x1, y1, x2, y2, stroke, stroke_width: stroke_w });
            }

            // 中央値線
            items.push(Prim::Line {
                x1: left, y1: y_median, x2: left + box_w, y2: y_median,
                stroke, stroke_width: median_w,
            });

            // 上ヒゲ(Q3→max)
            items.push(Prim::Line { x1: cx, y1: box_top,    x2: cx, y2: y_max, stroke, stroke_width: stroke_w });
            items.push(Prim::Line { x1: cx - cap_w / 2.0, y1: y_max, x2: cx + cap_w / 2.0, y2: y_max, stroke, stroke_width: stroke_w });

            // 下ヒゲ(Q1→min)
            items.push(Prim::Line { x1: cx, y1: box_bottom, x2: cx, y2: y_min, stroke, stroke_width: stroke_w });
            items.push(Prim::Line { x1: cx - cap_w / 2.0, y1: y_min, x2: cx + cap_w / 2.0, y2: y_min, stroke, stroke_width: stroke_w });
        }
    }

    Scene { width: spec.width, height: spec.height, items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::chartjs;
    use crate::font::DEFAULT_FONT;

    fn boxplot_spec() -> ChartSpec {
        let json = r#"{
            "type": "boxplot",
            "data": {
                "labels": ["A", "B"],
                "datasets": [{
                    "label": "S1",
                    "data": [
                        [10, 25, 50, 75, 90],
                        [5, 20, 45, 70, 95]
                    ]
                }]
            }
        }"#;
        chartjs::parse(json, false).expect("parse error")
    }

    #[test]
    fn boxplot_scene_has_rect_and_lines() {
        let spec = boxplot_spec();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rects: Vec<_> = scene.items.iter().filter(|p| matches!(p, Prim::Rect { .. })).collect();
        assert!(!rects.is_empty(), "should have at least one Rect (box)");
        let lines: Vec<_> = scene.items.iter().filter(|p| matches!(p, Prim::Line { .. })).collect();
        assert!(!lines.is_empty(), "should have at least one Line (whisker/median)");
    }

    #[test]
    fn boxplot_scene_width_height_match_spec() {
        let spec = boxplot_spec();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(scene.width, spec.width);
        assert_eq!(scene.height, spec.height);
    }
}
