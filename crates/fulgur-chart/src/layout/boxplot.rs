//! BoxPlot チャートのレイアウト。

use crate::ir::ChartSpec;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

const BOX_RATIO: f64 = 0.6;
const CAP_RATIO: f64 = 0.4;

/// Yドメインをbox_pointsから収集し、ユーザーのsuggested_min/maxを保持しながら
/// データ範囲で拡張したクローンspecを返す。begin_at_zeroはパーサーの解決値を保持する。
fn aux_spec(spec: &ChartSpec) -> ChartSpec {
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    for ser in &spec.series {
        for bp in &ser.box_points {
            // 5値すべてが finite な行のみドメインに含める。不完全行は build() でも
            // スキップされるため、ここで混入させるとスケールが狂う。
            if !bp.min.is_finite()
                || !bp.q1.is_finite()
                || !bp.median.is_finite()
                || !bp.q3.is_finite()
                || !bp.max.is_finite()
            {
                continue;
            }
            for &v in &[bp.min, bp.q1, bp.median, bp.q3, bp.max] {
                if v < data_min {
                    data_min = v;
                }
                if v > data_max {
                    data_max = v;
                }
            }
        }
    }
    let mut aux = spec.clone();
    if data_min.is_finite() {
        let prev = aux
            .y_axis
            .suggested_min
            .filter(|s| s.is_finite())
            .unwrap_or(data_min);
        aux.y_axis.suggested_min = Some(data_min.min(prev));
    }
    if data_max.is_finite() {
        let prev = aux
            .y_axis
            .suggested_max
            .filter(|s| s.is_finite())
            .unwrap_or(data_max);
        aux.y_axis.suggested_max = Some(data_max.max(prev));
    }
    aux
}

/// model.rs の compute_axes から再利用できるよう Frame を公開する。
pub fn compute_frame(spec: &ChartSpec, m: &TextMeasurer) -> super::common::Frame {
    super::common::compute(&aux_spec(spec), m)
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let frame = compute_frame(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    super::common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);
    let s = spec.series.len().max(1);
    let band_w = super::common::band_width(&frame, n);
    let box_w = (band_w * BOX_RATIO / s as f64).max(1.0);
    let cap_w = box_w * CAP_RATIO;
    let median_w = 2.5_f64;

    for i in 0..spec.categories.len() {
        let band_cx = super::common::category_center(&frame, i, n);
        let group_offset = -(s as f64 - 1.0) / 2.0;

        for (sidx, ser) in spec.series.iter().enumerate() {
            let Some(bp) = ser.box_points.get(i) else {
                continue;
            };
            if !bp.min.is_finite()
                || !bp.q1.is_finite()
                || !bp.median.is_finite()
                || !bp.q3.is_finite()
                || !bp.max.is_finite()
            {
                continue;
            }

            // borderWidth はパーサーが Series.stroke_width に格納(未指定時はデフォルト値)。
            let stroke_w = ser.stroke_width;

            let cx = band_cx + (group_offset + sidx as f64) * box_w;
            let left = cx - box_w / 2.0;

            let y_q1 = frame.ys.map(bp.q1);
            let y_q3 = frame.ys.map(bp.q3);
            let y_median = frame.ys.map(bp.median);
            let y_min = frame.ys.map(bp.min);
            let y_max = frame.ys.map(bp.max);

            let box_top = y_q3.min(y_q1);
            let box_bottom = y_q3.max(y_q1);
            let fill = ser.fill_at(i);
            let stroke = ser.stroke_at(i);

            items.push(Prim::Rect {
                x: left,
                y: box_top,
                w: box_w,
                h: (box_bottom - box_top).max(1.0),
                fill,
            });

            for &(x1, y1, x2, y2) in &[
                (left, box_top, left + box_w, box_top),
                (left + box_w, box_top, left + box_w, box_bottom),
                (left + box_w, box_bottom, left, box_bottom),
                (left, box_bottom, left, box_top),
            ] {
                items.push(Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    stroke_width: stroke_w,
                    dash: Vec::new(),
                });
            }

            items.push(Prim::Line {
                x1: left,
                y1: y_median,
                x2: left + box_w,
                y2: y_median,
                stroke,
                stroke_width: median_w,
                dash: Vec::new(),
            });

            items.push(Prim::Line {
                x1: cx,
                y1: box_top,
                x2: cx,
                y2: y_max,
                stroke,
                stroke_width: stroke_w,
                dash: Vec::new(),
            });
            items.push(Prim::Line {
                x1: cx - cap_w / 2.0,
                y1: y_max,
                x2: cx + cap_w / 2.0,
                y2: y_max,
                stroke,
                stroke_width: stroke_w,
                dash: Vec::new(),
            });

            items.push(Prim::Line {
                x1: cx,
                y1: box_bottom,
                x2: cx,
                y2: y_min,
                stroke,
                stroke_width: stroke_w,
                dash: Vec::new(),
            });
            items.push(Prim::Line {
                x1: cx - cap_w / 2.0,
                y1: y_min,
                x2: cx + cap_w / 2.0,
                y2: y_min,
                stroke,
                stroke_width: stroke_w,
                dash: Vec::new(),
            });
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;

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
        let rects: Vec<_> = scene
            .items
            .iter()
            .filter(|p| matches!(p, Prim::Rect { .. }))
            .collect();
        assert!(!rects.is_empty(), "should have at least one Rect (box)");
        let lines: Vec<_> = scene
            .items
            .iter()
            .filter(|p| matches!(p, Prim::Line { .. }))
            .collect();
        assert!(
            !lines.is_empty(),
            "should have at least one Line (whisker/median)"
        );
    }

    #[test]
    fn boxplot_scene_width_height_match_spec() {
        let spec = boxplot_spec();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(scene.width, spec.width);
        assert_eq!(scene.height, spec.height);
    }

    #[test]
    fn boxplot_box_top_is_above_box_bottom() {
        // Q3 > Q1 (値空間) → スクリーン座標ではQ3のy座標 < Q1のy座標 (上が小さい)
        // box_top = min(y_q3, y_q1) なのでbox_topはbox_bottom(max)より小さいはず
        let spec = boxplot_spec();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rect = scene
            .items
            .iter()
            .find_map(|p| {
                if let Prim::Rect { y, h, .. } = p {
                    Some((*y, *h))
                } else {
                    None
                }
            })
            .expect("should have at least one Rect");
        let (box_top, h) = rect;
        assert!(h > 0.0, "box height must be positive (Q1 != Q3)");
        assert!(box_top > 0.0, "box_top should be within the plot area");
    }
}
