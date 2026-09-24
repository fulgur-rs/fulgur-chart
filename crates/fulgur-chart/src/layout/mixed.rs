//! 混合チャート(bar+line): 共有のカテゴリ x・線形 y 軸に棒系列と折れ線系列を重ねる。
//! frame は common::compute / common::draw_frame を共有する(byte 一致のため bar/line は不変)。
//! 棒/折れ線の幾何定数とヘルパは bar.rs / line.rs から複製している(意図的な重複)。

use super::bar::{BarBounds, BarSide, bar_primitive};
use super::common;
use crate::ir::{ChartSpec, SeriesType};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

// --- line.rs から複製した折れ線の定数 ---
/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

/// Builds a mixed chart, painting datasets from higher order to lower order.
pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    // 共有フレーム(カテゴリ x・全系列 values からの y ドメイン)。
    let frame = common::compute(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);

    // order 昇順の系列列を逆順に描くことで、高い order を背面へ置く。
    // bar の横位置は系列列の昇順スロットに固定してから、描画だけを逆順にする。
    let mut bar_slots = vec![None; spec.series.len()];
    let mut bar_count = 0;
    for (i, ser) in spec.series.iter().enumerate() {
        if ser.series_type == SeriesType::Bar {
            bar_slots[i] = Some(bar_count);
            bar_count += 1;
        }
    }
    let legacy_geometry = matches!(spec.x_positions, crate::ir::XPositions::Category)
        && spec.series.iter().all(|series| {
            series
                .bar_geometry
                .is_none_or(|geometry| !geometry.has_geometry_controls())
        });

    for (series_index, ser) in spec.series.iter().enumerate().rev() {
        match ser.series_type {
            SeriesType::Bar => {
                if let Some(bar_slot) = bar_slots[series_index] {
                    draw_bar_dataset(
                        &mut items,
                        spec,
                        &frame,
                        ser,
                        bar_slot,
                        bar_count,
                        legacy_geometry,
                    );
                }
            }
            SeriesType::Line => draw_line_dataset(&mut items, spec, &frame, n, series_index, ser),
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// Paints one bar dataset at its slot in ascending Chart.js order.
fn draw_bar_dataset(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    frame: &common::Frame,
    ser: &crate::ir::Series,
    bar_slot: usize,
    bar_count: usize,
    legacy_geometry: bool,
) {
    let n = spec.categories.len().max(1);
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);

    for i in 0..spec.categories.len() {
        // 欠損 / 非有限値はスロットを空けて次系列へ(dodge の色・位置整合を保つ)。
        let Some(&v) = ser.values.get(i) else {
            continue;
        };
        if !v.is_finite() {
            continue;
        }
        let (center, band_left, band_w) = common::x_index_band(spec, frame, i, n);
        let (bx, bar_w) = super::bar::category_bar_bounds(
            center,
            band_left,
            band_w,
            bar_slot,
            bar_count,
            ser.bar_geometry,
            legacy_geometry,
        );
        let vy = frame.ys.map(common::clip_axis_value(v, &frame.ticks));
        let (base, head) = super::bar::enforce_min_bar_length(
            baseline_y,
            vy,
            v,
            super::bar::min_bar_length_for_visible_interval(
                ser.bar_geometry
                    .and_then(|geometry| geometry.min_bar_length),
                0.0,
                v,
                &frame.ticks,
            ),
            -1.0,
            frame.plot_top,
            frame.plot_bottom,
        );
        let y_top = base.min(head);
        let h = (head - base).abs();
        items.push(bar_primitive(
            BarBounds {
                x: bx,
                y: y_top,
                w: bar_w,
                h,
            },
            ser.fill_at(i),
            ser.bar_geometry.and_then(|geometry| geometry.border_radius),
            if base >= head {
                BarSide::Bottom
            } else {
                BarSide::Top
            },
            true,
        ));
        if spec.data_labels && h > 0.0 && common::axis_value_in_bounds(v, &frame.ticks) {
            let cx = bx + bar_w / 2.0;
            let label_y = if v >= 0.0 {
                y_top - common::LABEL_GAP
            } else {
                y_top + h + spec.theme.font_size
            };
            items.push(common::value_label(
                cx,
                label_y,
                spec.theme.font_size,
                Anchor::Middle,
                spec.theme.text_color,
                v,
                false, // Mixed は対数軸をとりえない(frontend でスコープ外)
            ));
        }
    }
}

/// Paints the area, line, markers, and labels for one mixed-chart line dataset.
fn draw_line_dataset(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    frame: &common::Frame,
    n: usize,
    series_index: usize,
    ser: &crate::ir::Series,
) {
    // 有効点列: (x, y, 元カテゴリインデックス)。欠損・非有限値を除外。
    // 元インデックスは gap 検出とラベル lookup に使う。
    let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
        .filter_map(|i| {
            let v = ser.values.get(i).copied()?;
            if !v.is_finite() {
                return None;
            }
            let x = common::x_index_band(spec, frame, i, n).0;
            Some((x, frame.ys.map(common::clip_axis_value(v, &frame.ticks)), i))
        })
        .collect();

    // 元インデックスが連続しない箇所でセグメントを分割する
    // (chart.js の spanGaps=false 既定と同じ「欠損で線が途切れる」挙動)。
    let segments: Vec<Vec<(f64, f64, usize)>> = {
        let mut segs: Vec<Vec<(f64, f64, usize)>> = Vec::new();
        let mut cur: Vec<(f64, f64, usize)> = Vec::new();
        let mut prev_cat: Option<usize> = None;
        for &(x, y, cat) in &valid {
            if prev_cat.is_some_and(|pc| cat != pc + 1) && !cur.is_empty() {
                segs.push(std::mem::take(&mut cur));
            }
            cur.push((x, y, cat));
            prev_cat = Some(cat);
        }
        if !cur.is_empty() {
            segs.push(cur);
        }
        segs
    };

    // area(背面): 線と同じくセグメント単位で 1 つずつ閉多角形を描く(line.rs と同挙動)。
    // gap を跨いだ塗りを防ぐ。非 null / 非 gap 系列では 1 セグメントで従来と同一のパス
    // データを出力する(バイト不変)。
    if ser.area {
        if ser.area_fill.is_some() {
            items.extend(super::line::chartjs_area_fill_primitives(
                spec,
                frame,
                series_index,
                &segments,
                None,
            ));
        } else {
            let baseline_y = frame
                .ys
                .map(0.0_f64.clamp(frame.ticks.min, frame.ticks.max));
            for seg in &segments {
                if seg.is_empty() {
                    continue;
                }
                let mut d = String::new();
                for (k, &(x, y, _)) in seg.iter().enumerate() {
                    let cmd = if k == 0 { 'M' } else { 'L' };
                    write!(d, "{} {} {} ", cmd, fmt_num(x), fmt_num(y)).unwrap();
                }
                let (last_x, _, _) = seg[seg.len() - 1];
                let (first_x, _, _) = seg[0];
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
        }
    }

    // 線: セグメントごとに描く(gap で線が途切れる)。
    let line_style = ser.line_style.as_ref();
    let show_line = line_style.is_none_or(|style| style.show_line);
    let border_dash = line_style.map_or(&[][..], |style| style.border_dash.as_slice());
    let border_dash_offset = line_style.map_or(0.0, |style| style.border_dash_offset);
    for seg in &segments {
        if !show_line || seg.len() < 2 {
            continue;
        }
        let xy: Vec<(f64, f64)> = seg.iter().map(|&(x, y, _)| (x, y)).collect();
        if !border_dash.is_empty() {
            match ser.interpolation {
                crate::ir::LineInterpolation::Linear => {
                    items.push(Prim::StyledPolyline {
                        points: xy,
                        stroke: ser.stroke_at(0),
                        stroke_width: ser.stroke_width,
                        dash: border_dash.to_vec(),
                        dash_offset: border_dash_offset,
                    });
                }
                crate::ir::LineInterpolation::Monotone => {
                    items.push(Prim::StyledPath {
                        d: super::monotone::monotone_path(&xy),
                        stroke: ser.stroke_at(0),
                        stroke_width: ser.stroke_width,
                        dash: border_dash.to_vec(),
                        dash_offset: border_dash_offset,
                    });
                }
                crate::ir::LineInterpolation::CatmullRom { tension } => {
                    items.push(Prim::StyledPath {
                        d: catmull_rom_path(&xy, tension, frame.plot_top, frame.plot_bottom),
                        stroke: ser.stroke_at(0),
                        stroke_width: ser.stroke_width,
                        dash: border_dash.to_vec(),
                        dash_offset: border_dash_offset,
                    });
                }
            }
            continue;
        }
        match ser.interpolation {
            crate::ir::LineInterpolation::Linear => {
                items.push(Prim::Polyline {
                    points: xy,
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            }
            crate::ir::LineInterpolation::Monotone => {
                items.push(Prim::Path {
                    d: super::monotone::monotone_path(&xy),
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
            crate::ir::LineInterpolation::CatmullRom { tension } => {
                let d = catmull_rom_path(&xy, tension, frame.plot_top, frame.plot_bottom);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }
    }

    // マーカー: 有効点のみ。
    for &(cx, cy, cat) in &valid {
        if !common::axis_value_in_bounds(ser.values[cat], &frame.ticks) {
            continue;
        }
        common::dataset_point_marker(
            items,
            cx,
            cy,
            MARKER_R,
            ser.stroke_at(0),
            ser.stroke_at(0),
            0.0,
            line_style.and_then(|style| style.point_style),
        );
    }

    // データラベル(点の上、マーカー半径ぶん+余白だけ上)。
    // 元カテゴリインデックスで ser.values を引くことで filter 後のずれを防ぐ。
    if spec.data_labels {
        for &(x, y, cat) in &valid {
            if !common::axis_value_in_bounds(ser.values[cat], &frame.ticks) {
                continue;
            }
            items.push(common::value_label(
                x,
                y - MARKER_R - common::LABEL_GAP,
                spec.theme.font_size,
                Anchor::Middle,
                spec.theme.text_color,
                ser.values[cat],
                false, // Mixed は対数軸をとりえない(frontend でスコープ外)
            ));
        }
    }
}

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する(line.rs から複製)。
/// 端点は自身を複製して扱う。`pts.len() >= 2` を前提とする。
fn catmull_rom_path(pts: &[(f64, f64)], tension: f64, min_y: f64, max_y: f64) -> String {
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
            (p1.1 + (p2.1 - p0.1) / 6.0 * tension).clamp(min_y, max_y),
        );
        let cp2 = (
            p2.0 - (p3.0 - p1.0) / 6.0 * tension,
            (p2.1 - (p3.1 - p1.1) / 6.0 * tension).clamp(min_y, max_y),
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
    use crate::text::TextMeasurer;

    #[test]
    fn mixed_line_with_null_and_fill_splits_area_at_gap() {
        // 混合(bar+line)の line 系列で fill:true + 欠損があるとき、area は gap を跨がず
        // 2 つの閉多角形に分割される(line.rs と同挙動)。
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c","d","e"],
               "datasets":[
                 {"type":"line","data":[1, 2, null, 4, 5], "fill": true}
               ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let area_paths = scene
            .items
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Prim::Path {
                        fill: Some(_),
                        stroke: None,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            area_paths, 2,
            "mixed line area should split into 2 polygons at the gap"
        );
    }

    #[test]
    fn mixed_bar_and_line_skip_nan() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[
                 {"type":"bar","data":[10, null, 30]},
                 {"type":"line","data":[1, null, 3]}
               ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        for prim in &scene.items {
            match prim {
                Prim::Rect { x, y, w, h, .. } => {
                    assert!(
                        x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite(),
                        "Rect must not contain NaN: {x} {y} {w} {h}"
                    );
                }
                Prim::Circle { cx, cy, .. } => {
                    assert!(
                        cx.is_finite() && cy.is_finite(),
                        "Circle must not contain NaN: {cx} {cy}"
                    );
                }
                Prim::Polyline { points, .. } => {
                    for (px, py) in points {
                        assert!(
                            px.is_finite() && py.is_finite(),
                            "Polyline point must not contain NaN: {px} {py}"
                        );
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn mixed_temporal_bar_and_line_share_irregular_time_positions() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["1970-01-01","1970-01-02","1970-01-05"],
              "datasets":[{"type":"bar","data":[2,3,4]},{"type":"line","data":[1,2,3]}]},
              "options":{"scales":{"x":{"type":"time"}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let bars = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, .. } => Some(x + w / 2.0),
                _ => None,
            })
            .collect::<Vec<_>>();
        let line_points = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, .. } => Some(*cx),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(bars.len(), 3);
        assert_eq!(line_points.len(), 3);
        let first_gap = bars[1] - bars[0];
        let second_gap = bars[2] - bars[1];
        assert!((second_gap / first_gap - 3.0).abs() < 1e-9);
        for (bar, line) in bars.iter().zip(line_points) {
            assert!((bar - line).abs() < 1e-9);
        }
    }

    #[test]
    fn line_root_bar_override_uses_its_dataset_thickness() {
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b"],"datasets":[
                {"type":"bar","data":[1,2],"barThickness":12},
                {"data":[2,1]}
            ]}}"#,
            true,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let bars: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { w, fill, .. } if *fill == spec.series[0].fill_at(0) => Some(*w),
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 2);
        assert!(bars.iter().all(|width| (*width - 12.0).abs() < 1e-9));
    }

    #[test]
    fn mixed_dodge_slots_remain_even_when_only_one_bar_has_geometry_options() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a"],"datasets":[
                {"type":"bar","data":[1]},
                {"type":"bar","data":[2],"barPercentage":0.9},
                {"type":"line","data":[3]}
            ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rect_for = |series_index: usize| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Rect { x, w, fill, .. }
                        if *fill == spec.series[series_index].fill_at(0) =>
                    {
                        Some((*x, *w))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (first_x, first_w) = rect_for(0);
        let (second_x, second_w) = rect_for(1);
        let center_distance = (second_x + second_w / 2.0) - (first_x + first_w / 2.0);
        let expected_center_distance = first_w / 0.9;

        assert!((first_w - second_w).abs() < 1e-9);
        assert!(
            (center_distance - expected_center_distance).abs() < 1e-9,
            "mixed bar slots should be evenly spaced"
        );
    }

    #[test]
    fn mixed_negative_bar_label_follows_bar_at_axis_edge() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
                {"type":"bar","data":[-1],"minBarLength":15},
                {"type":"line","data":[2]}
            ]},"options":{"scales":{"y":{"min":-100,"max":-1}},
              "plugins":{"datalabels":{"display":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (bar_x, bar_y, bar_w, bar_h) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Rect { x, y, w, h, fill } if *fill == spec.series[0].fill_at(0) => {
                    Some((*x, *y, *w, *h))
                }
                _ => None,
            })
            .expect("missing bar rectangle");
        let label_y = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text {
                    x,
                    y,
                    content,
                    anchor: Anchor::Middle,
                    ..
                } if content == "-1" && (x - (bar_x + bar_w / 2.0)).abs() < 1e-9 => Some(*y),
                _ => None,
            })
            .expect("missing data label for -1");

        let expected_y = bar_y + bar_h + spec.theme.font_size;
        assert!((label_y - expected_y).abs() < 1e-9);
    }

    #[test]
    fn mixed_bar_and_line_stay_inside_hard_y_bounds() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],"datasets":[
                {"type":"bar","data":[1,50,100]}, {"type":"line","data":[1,50,100]}]},
                "options":{"scales":{"y":{"min":13,"max":87}}}}"#,
            false,
        )
        .unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &measurer);

        let scene = build(&spec, &measurer);

        for item in &scene.items {
            match item {
                Prim::Rect { y, h, .. } => {
                    assert!(*y >= frame.plot_top, "bar top escaped plot: y={y}");
                    assert!(
                        *y + *h <= frame.plot_bottom,
                        "bar bottom escaped plot: y={y}, h={h}"
                    );
                }
                Prim::Polyline { points, .. } => assert!(
                    points
                        .iter()
                        .all(|(_, y)| *y >= frame.plot_top && *y <= frame.plot_bottom)
                ),
                _ => {}
            }
        }
        assert_eq!(
            scene
                .items
                .iter()
                .filter(|item| matches!(item, Prim::Circle { .. }))
                .count(),
            1,
            "only the in-range line value should have a marker"
        );
    }

    #[test]
    fn mixed_min_bar_length_skips_values_outside_hard_bounds() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["below","inside"],"datasets":[
                {"type":"bar","data":[5,11],"minBarLength":20},
                {"type":"line","data":[5,11]}]},
                "options":{"scales":{"y":{"min":10,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let fills = [spec.series[0].fill_at(0), spec.series[0].fill_at(1)];
        let scene = build(&spec, &m);
        let heights: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { h, fill, .. } if fills.contains(fill) => Some(*h),
                _ => None,
            })
            .collect();

        assert_eq!(heights, [0.0, 20.0]);
    }

    #[test]
    fn mixed_catmull_rom_line_emits_a_path() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"type":"line","data":[1,3,2],"tension":0.4}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert!(scene.items.iter().any(|item| matches!(
            item,
            Prim::Path {
                fill: None,
                stroke: Some(_),
                ..
            }
        )));
    }
}
