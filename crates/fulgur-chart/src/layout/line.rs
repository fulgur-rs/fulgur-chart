//! line / area チャート。共有フレーム(common)の上に折れ線・面・マーカーを重ねる。

use super::common;
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

/// line チャートのモデル幾何用の全マーカー点（`model::build_model` が参照）。
/// レンダリング経路の `build()` は点を独立に計算しデシメーションするため、巨大データでは
/// この全点列と実際の描画点は乖離する（モデルは chart.js 数値照合用＝間引きなしが正しい）。
/// カテゴリごとに `line_category_x + ys.map` で計算し、欠損値は 0.0 扱い。
pub fn line_points(
    spec: &crate::ir::ChartSpec,
    frame: &common::Frame,
) -> Vec<crate::layout::scatter::PointBox> {
    let n = spec.categories.len().max(1);
    let mut pts = Vec::new();
    for (sidx, ser) in spec.series.iter().enumerate() {
        for i in 0..spec.categories.len() {
            let x = common::line_category_x(spec, frame, i, n);
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
                let x = common::line_category_x(spec, &frame, i, n);
                Some((x, frame.ys.map(v), i))
            })
            .collect();

        // 元インデックスが連続しない箇所でセグメントを分割する。
        // chart.js の spanGaps=false デフォルトと同じ「欠損で線が途切れる」挙動。
        // 間引きは cat を保持したまま各セグメントへ適用するため、cat を含めて分割する
        // （間引き後に cat で再分割すると全点が gap 扱いになり線が消えるため、再分割しない）。
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

        // デシメーション判定は系列全体の点数で（gap 分割の前後で一貫）。
        // 各セグメントを個別に間引き、line はその結果から直接描く（再分割しない）。
        let plot_width = frame.plot_right - frame.plot_left;
        let dec = crate::layout::decimate::resolve(&spec.decimation, plot_width, valid.len());
        let decimated = dec.is_some();
        let segments: Vec<Vec<(f64, f64, usize)>> = if let Some((algo, samples)) = dec {
            // samples はセグメント長で按分される（decimate_segments）。これにより gap で
            // 多数セグメントに割れた LTTB 系列でも合計が samples+3×セグメント数 以下に収まる
            // （min-max は samples を無視し占有ピクセル列数で自己制限）。
            crate::layout::decimate::decimate_segments(&segments, algo, samples)
        } else {
            segments
        };
        // area/marker/label 用に間引き後の点列へ差し替え（Chart.js dataset.data 差し替えモデル）。
        // cat は維持するため、ラベルの ser.values[cat] 参照は引き続き正しい。
        let valid: Vec<(f64, f64, usize)> = segments.iter().flatten().copied().collect();

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

        // 線: セグメントごとに描く(gap で線が途切れる)。間引き済みセグメントから直接描画する。
        for seg in &segments {
            if seg.len() < 2 {
                continue;
            }
            let xy: Vec<(f64, f64)> = seg.iter().map(|&(x, y, _)| (x, y)).collect();
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline {
                    points: xy,
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                let d = catmull_rom_path(&xy, ser.tension);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }

        // マーカー。threshold 超過で間引いた場合、線として描かれる(≥2点)セグメントの帯マーカーは
        // 既定で抑制する。ただし単点セグメント(gap で孤立し線にならない点)はマーカーが唯一の
        // 表現なので描画し、空チャート化を防ぐ。pointRadius 明示時は全点描画(エスケープハッチ)。
        // 非間引き時は従来どおり全点を MARKER_R で描画(バイト不変。segments を平坦化すると valid と
        // 同順・同内容)。
        for seg in &segments {
            let r = match (decimated, ser.point_radius) {
                (false, _) => Some(MARKER_R),
                (true, Some(r)) if r > 0.0 => Some(r),
                (true, Some(_)) => None,
                // 間引き既定: 線になる(≥2点)なら帯を抑制、単点(孤立点)は描画。
                (true, None) if seg.len() < 2 => Some(MARKER_R),
                (true, None) => None,
            };
            if let Some(r) = r {
                for &(cx, cy, _) in seg {
                    items.push(Prim::Circle {
                        cx,
                        cy,
                        r,
                        fill: ser.stroke_at(0),
                        stroke: ser.stroke_at(0),
                        stroke_width: 0.0,
                    });
                }
            }
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
    fn line_frame_stays_valid_when_edge_labels_exceed_width() {
        // 狭い幅 + 長い端ラベルでも edge 余白で描画領域が反転しない(plot_right >= plot_left)。
        let mut spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["VeryLongCategoryLabelLeft","VeryLongCategoryLabelRight"],
               "datasets":[{"data":[1,2]}]}}"#,
            false,
        )
        .unwrap();
        spec.width = 60.0; // edge ラベル半幅合計が利用可能幅を超える狭い幅。
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        assert!(
            frame.plot_right >= frame.plot_left,
            "plot area inverted: left={} right={}",
            frame.plot_left,
            frame.plot_right
        );
        // line_x は有限かつ先頭<=末尾(NaN や順序反転を生まない)。
        let n = spec.categories.len();
        let x0 = common::line_x(&frame, 0, n);
        let x_last = common::line_x(&frame, n - 1, n);
        assert!(x0.is_finite() && x_last.is_finite());
        assert!(x_last >= x0);
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

    #[test]
    fn line_points_x_is_band_centered_when_offset() {
        // chart.js offset:true: 点は category_center(band 中心)に並ぶ。
        // n=3 なら plot_left+0.5*band_w / +1.5*band_w / +2.5*band_w。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]},
               "options":{"scales":{"x":{"offset":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let ps = line_points(&spec, &frame);
        let s0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        let band_w = (frame.plot_right - frame.plot_left) / 3.0;
        for (i, p) in s0.iter().enumerate() {
            let expect = frame.plot_left + (i as f64 + 0.5) * band_w;
            assert!(
                (p.cx - expect).abs() < 1e-9,
                "offset:true の点は band 中心: i={i} cx={} expect={expect}",
                p.cx
            );
        }
        // edge-to-edge と区別: 先頭は plot_left より内側、末尾は plot_right より内側。
        assert!(s0[0].cx > frame.plot_left);
        assert!(s0[2].cx < frame.plot_right);
    }

    #[test]
    fn offset_line_skips_edge_padding() {
        // offset:true は bar 同様に端ラベル半幅の余白を取らない。
        // edge-to-edge(既定)では末尾ラベル半幅ぶん plot_right を内側化するため、
        // offset 版の plot_right はそれより外側(広い)になる。
        let parse = |opts: &str| {
            chartjs::parse(
                &format!(
                    r#"{{"type":"line","data":{{"labels":["Jan","Feb","Mar"],
                       "datasets":[{{"data":[10,20,30]}}]}}{opts}}}"#
                ),
                false,
            )
            .unwrap()
        };
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let edge = common::compute(&parse(""), &m);
        let off = common::compute(&parse(r#","options":{"scales":{"x":{"offset":true}}}"#), &m);
        assert!(
            off.plot_right > edge.plot_right,
            "offset:true は端余白を取らないため plot_right がより外側: off={} edge={}",
            off.plot_right,
            edge.plot_right
        );
    }

    #[test]
    fn offset_line_labels_align_to_band_centers() {
        // draw_frame の x ラベルも offset:true では band 中心(line_x ではなく category_center)。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]},
               "options":{"scales":{"x":{"offset":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);
        // title/legend なし → anchor=Middle の Text は x カテゴリラベルのみ。
        let label_xs: Vec<f64> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Text {
                    x,
                    anchor: Anchor::Middle,
                    ..
                } => Some(*x),
                _ => None,
            })
            .collect();
        assert_eq!(label_xs.len(), 3, "x ラベルは 3 個");
        let band_w = (frame.plot_right - frame.plot_left) / 3.0;
        for (i, &x) in label_xs.iter().enumerate() {
            let expect = frame.plot_left + (i as f64 + 0.5) * band_w;
            assert!(
                (x - expect).abs() < 1e-9,
                "offset ラベルは band 中心: i={i} x={x} expect={expect}"
            );
        }
    }
}
