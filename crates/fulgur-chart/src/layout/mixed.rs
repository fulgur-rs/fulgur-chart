//! 混合チャート(bar+line): 共有のカテゴリ x・線形 y 軸に棒系列と折れ線系列を重ねる。
//! frame は common::compute / common::draw_frame を共有する(byte 一致のため bar/line は不変)。
//! 棒/折れ線の幾何定数とヘルパは bar.rs / line.rs から複製している(意図的な重複)。

use super::common;
use crate::ir::{ChartSpec, SeriesType};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

// --- bar.rs から複製した縦棒の幾何定数 ---
/// band 内のグループ幅比。
const GROUP_RATIO: f64 = 0.8;
/// band 左右パディング比。
const BAND_PAD_RATIO: f64 = 0.1;
/// bar 幅の塗り比。
const BAR_FILL_RATIO: f64 = 0.9;

// --- line.rs から複製した折れ線の定数 ---
/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 共有フレーム(カテゴリ x・全系列 values からの y ドメイン)。
    let frame = common::compute(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, m);

    let n = spec.categories.len().max(1);

    // 棒系列の本数(スロット分割の分母)。
    let bar_count = spec
        .series
        .iter()
        .filter(|s| s.series_type == SeriesType::Bar)
        .count();

    // --- 1. 棒系列(背面) ---
    if bar_count > 0 {
        let band_w = common::band_width(&frame, n);
        let group_w = band_w * GROUP_RATIO;
        let bar_w = group_w / bar_count as f64;
        let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
        let baseline_y = frame.ys.map(base_v);

        for i in 0..spec.categories.len() {
            let band_left = common::category_center(&frame, i, n) - band_w / 2.0;
            // 棒系列のスロット番号(全系列ではなく棒系列内の位置)。
            let mut bar_slot = 0_usize;
            for ser in &spec.series {
                if ser.series_type != SeriesType::Bar {
                    continue;
                }
                let bx = band_left + band_w * BAND_PAD_RATIO + bar_slot as f64 * bar_w;
                // 欠損 / 非有限値はスロットを空けて次系列へ(dodge の色・位置整合を保つ)。
                let Some(&v) = ser.values.get(i) else {
                    bar_slot += 1;
                    continue;
                };
                if !v.is_finite() {
                    bar_slot += 1;
                    continue;
                }
                let vy = frame.ys.map(v);
                let y_top = vy.min(baseline_y);
                let h = (vy - baseline_y).abs();
                items.push(Prim::Rect {
                    x: bx,
                    y: y_top,
                    w: (bar_w * BAR_FILL_RATIO).max(0.0),
                    h,
                    fill: ser.fill_at(i),
                });
                if spec.data_labels {
                    let cx = bx + (bar_w * BAR_FILL_RATIO) / 2.0;
                    let label_y = if v >= base_v {
                        y_top - common::LABEL_GAP
                    } else {
                        y_top + h + label_font
                    };
                    items.push(common::value_label(
                        cx,
                        label_y,
                        label_font,
                        Anchor::Middle,
                        ink,
                        v,
                    ));
                }
                bar_slot += 1;
            }
        }
    }

    // --- 2. 折れ線系列(前面) ---
    for ser in &spec.series {
        if ser.series_type != SeriesType::Line {
            continue;
        }
        // 有効点列: (x, y, 元カテゴリインデックス)。欠損・非有限値を除外。
        // 元インデックスは gap 検出とラベル lookup に使う。
        let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
            .filter_map(|i| {
                let v = ser.values.get(i).copied()?;
                if !v.is_finite() {
                    return None;
                }
                let x = common::category_center(&frame, i, n);
                Some((x, frame.ys.map(v), i))
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

        // 線: セグメントごとに描く(gap で線が途切れる)。
        for seg in &segments {
            if seg.len() < 2 {
                continue;
            }
            let xy: Vec<(f64, f64)> = seg.iter().map(|&(x, y, _)| (x, y)).collect();
            match ser.interpolation {
                crate::ir::LineInterpolation::Linear | crate::ir::LineInterpolation::Monotone => {
                    items.push(Prim::Polyline {
                        points: xy,
                        stroke: ser.stroke_at(0),
                        stroke_width: ser.stroke_width,
                    });
                }
                crate::ir::LineInterpolation::CatmullRom { tension } => {
                    let d = catmull_rom_path(&xy, tension);
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
                    label_font,
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

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する(line.rs から複製)。
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
