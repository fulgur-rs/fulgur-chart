//! line / area チャート。共有フレーム(common)の上に折れ線・面・マーカーを重ねる。

use super::common;
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

/// line チャートの全マーカー点（renderer とモデルの単一の真実源）。
/// カテゴリごとに `line_x + ys.map` で計算し、欠損値は 0.0 扱い。
pub fn line_points(
    spec: &crate::ir::ChartSpec,
    frame: &common::Frame,
) -> Vec<crate::layout::scatter::PointBox> {
    let n = spec.categories.len().max(1);
    let mut pts = Vec::new();
    for (sidx, ser) in spec.series.iter().enumerate() {
        for i in 0..spec.categories.len() {
            let x = common::line_x(frame, i, n);
            let v = ser.values.get(i).copied().unwrap_or(0.0);
            pts.push(crate::layout::scatter::PointBox {
                series: sidx,
                index: i,
                kind: "line",
                cx: x,
                cy: frame.ys.map(v),
                r: MARKER_R,
            });
        }
    }
    pts
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let frame = common::compute(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);

    for ser in &spec.series {
        // 有効点列: (x, y, 元カテゴリインデックス)。欠損・非有限値を除外。
        // 元インデックスはラベル lookup と gap 検出に使う。
        let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
            .filter_map(|i| {
                let v = ser.values.get(i).copied()?;
                if !v.is_finite() {
                    return None;
                }
                let x = common::line_x(&frame, i, n);
                Some((x, frame.ys.map(v), i))
            })
            .collect();

        // 元インデックスが連続しない箇所でセグメントを分割する。
        // chart.js の spanGaps=false デフォルトと同じ「欠損で線が途切れる」挙動。
        let segments: Vec<Vec<(f64, f64)>> = {
            let mut segs: Vec<Vec<(f64, f64)>> = Vec::new();
            let mut cur: Vec<(f64, f64)> = Vec::new();
            let mut prev_cat: Option<usize> = None;
            for &(x, y, cat) in &valid {
                if prev_cat.is_some_and(|pc| cat != pc + 1) && !cur.is_empty() {
                    segs.push(std::mem::take(&mut cur));
                }
                cur.push((x, y));
                prev_cat = Some(cat);
            }
            if !cur.is_empty() {
                segs.push(cur);
            }
            segs
        };

        // area（背面）: 有効点全体でひとつの閉多角形を描く。
        if ser.area && !valid.is_empty() {
            let baseline_y = frame
                .ys
                .map(0.0_f64.clamp(frame.ticks.min, frame.ticks.max));
            let mut d = String::new();
            for (k, &(x, y, _)) in valid.iter().enumerate() {
                let cmd = if k == 0 { 'M' } else { 'L' };
                write!(d, "{} {} {} ", cmd, fmt_num(x), fmt_num(y)).unwrap();
            }
            let (last_x, _, _) = valid[valid.len() - 1];
            let (first_x, _, _) = valid[0];
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

        // 線: セグメントごとに描く(gap で線が途切れる)。
        for seg in &segments {
            if seg.len() < 2 {
                continue;
            }
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline {
                    points: seg.clone(),
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                let d = catmull_rom_path(seg, ser.tension);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }

        // マーカー。
        for &(cx, cy, _) in &valid {
            items.push(Prim::Circle {
                cx,
                cy,
                r: MARKER_R,
                fill: ser.stroke_at(0),
                stroke: ser.stroke_at(0),
                stroke_width: 0.0,
            });
        }

        // データラベル(点の上、マーカー半径ぶん+余白だけ上)。
        // 元カテゴリインデックスで ser.values を引くことで filter 後のずれを防ぐ。
        if spec.data_labels {
            for &(x, y, cat) in &valid {
                items.push(common::value_label(
                    x,
                    y - MARKER_R - common::LABEL_GAP,
                    spec.theme.font_size,
                    Anchor::Middle,
                    spec.theme.text_color,
                    ser.values[cat],
                ));
            }
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する。
/// 端点は自身を複製して扱う。`pts.len() >= 2` を前提とする。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::layout::common;
    use crate::text::TextMeasurer;

    fn pts_for(json: &str) -> Vec<crate::layout::scatter::PointBox> {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        line_points(&spec, &frame)
    }

    #[test]
    fn line_points_count_is_series_times_categories() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b","c","d","e","f","g"],
               "datasets":[{"data":[1,2,3,4,5,6,7]},{"data":[7,6,5,4,3,2,1]}]}}"#,
        );
        assert_eq!(ps.len(), 14);
        for p in &ps {
            assert_eq!(p.kind, "line");
        }
    }

    #[test]
    fn line_points_x_is_edge_to_edge() {
        // chart.js offset:false: n=3 の点は plot_left / 中点 / plot_right に並ぶ。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let ps = line_points(&spec, &frame);
        let s0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!((s0[0].cx - frame.plot_left).abs() < 1e-9);
        assert!((s0[2].cx - frame.plot_right).abs() < 1e-9);
        assert!((s0[1].cx - (frame.plot_left + frame.plot_right) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn line_points_cx_monotone_with_category_order() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]}}"#,
        );
        let ser0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!(ser0[0].cx < ser0[1].cx && ser0[1].cx < ser0[2].cx);
    }

    #[test]
    fn line_points_cy_tracks_value() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b"],
               "datasets":[{"data":[10,100]}]}}"#,
        );
        let ser0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!(
            ser0[1].cy < ser0[0].cy,
            "大きい値は小さい cy(上方向): ser0[0].cy={}, ser0[1].cy={}",
            ser0[0].cy,
            ser0[1].cy
        );
    }
}
