//! sparkline チャート: 軸・ラベル・凡例なしのミニマル折れ線。

use super::common;
use crate::ir::ChartSpec;
use crate::num::fmt_num;
use crate::scale::LinearScale;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

const PAD: f64 = common::OUTER_PAD;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let (domain_min, domain_max) = common::value_domain(spec, &spec.y_axis);

    // y スケール（画面上下反転）
    let ys = LinearScale::new(domain_min, domain_max, spec.height - PAD, PAD);

    let plot_left = PAD;
    let plot_right = spec.width - PAD;

    // 全系列で共有する x スケール（異なる長さの系列も同一 index が同 x に並ぶ）。
    let max_count = spec
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(1)
        .max(1);

    let mut items: Vec<Prim> = Vec::new();

    // plot_width は間引き判定に使う（論理ピクセル空間、Frame は持たない）。
    let plot_width = plot_right - plot_left;

    for ser in &spec.series {
        let count = ser.values.len();
        if count == 0 {
            continue;
        }
        // (x, y, index)。x はエッジ対エッジ配置（最初の点は plot_left、最後は plot_right）。
        // 系列が 1 点の場合は中央に配置する。
        let pts: Vec<(f64, f64, usize)> = (0..count)
            .map(|i| {
                let x = if max_count == 1 {
                    (plot_left + plot_right) / 2.0
                } else {
                    plot_left + i as f64 * (plot_right - plot_left) / (max_count - 1) as f64
                };
                (x, ys.map(ser.values[i]), i)
            })
            .collect();

        // sparkline は gap 分割を持たないため系列全体を単一セグメントとして間引く。
        // 判定は系列全体の点数で（line と同じ Chart.js セマンティクス）。
        // no-fire 時は pts をそのまま使い、変更前とバイト不変を保つ。
        let pts: Vec<(f64, f64, usize)> =
            match crate::layout::decimate::resolve(&spec.decimation, plot_width, count) {
                Some((algo, samples)) => crate::layout::decimate::decimate_one(&pts, algo, samples),
                None => pts,
            };

        // area（背面）
        if ser.area && pts.len() >= 2 {
            let baseline_y = ys.map(0.0_f64.clamp(domain_min, domain_max));
            let mut d = String::new();
            for (k, &(x, y, _)) in pts.iter().enumerate() {
                let cmd = if k == 0 { 'M' } else { 'L' };
                write!(d, "{} {} {} ", cmd, fmt_num(x), fmt_num(y)).unwrap();
            }
            let (last_x, _, _) = pts[pts.len() - 1];
            let (first_x, _, _) = pts[0];
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

        // 折れ線
        if pts.len() >= 2 {
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline {
                    points: pts.iter().map(|&(x, y, _)| (x, y)).collect(),
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                let xy: Vec<(f64, f64)> = pts.iter().map(|&(x, y, _)| (x, y)).collect();
                let d = catmull_rom_path(&xy, ser.tension);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }
        // マーカーなし・データラベルなし
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

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
    use crate::scene::Prim;

    fn build_spec(json: &str) -> Scene {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        build(&spec, &m)
    }

    fn polyline_len(scene: &Scene) -> usize {
        scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Polyline { points, .. } => Some(points.len()),
                _ => None,
            })
            .expect("sparkline should have a polyline")
    }

    /// area の `Prim::Path` の座標コマンド数（"L " の出現回数）。
    /// area パスは M + 各点 L + baseline 2×L で構成されるため、
    /// 非間引き N 点なら (N-1)+2 = N+1 個の "L "。間引きで大きく減る。
    fn area_path_l_count(scene: &Scene) -> usize {
        scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Path { d, .. } => Some(d.matches("L ").count()),
                _ => None,
            })
            .expect("area sparkline should have a Path")
    }

    fn huge_sparkline_json(extra_opts: &str) -> String {
        let data: Vec<String> = (0..5000).map(|i| ((i * 7) % 13).to_string()).collect();
        format!(
            r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{}]}}]}}{}}}"#,
            data.join(","),
            extra_opts
        )
    }

    #[test]
    fn huge_sparkline_is_decimated_by_default() {
        let scene = build_spec(&huge_sparkline_json(""));
        assert!(
            polyline_len(&scene) < 5000,
            "auto-on decimation should reduce 5000 pts"
        );
    }

    #[test]
    fn huge_sparkline_passthrough_when_disabled() {
        let json =
            huge_sparkline_json(r#","options":{"plugins":{"decimation":{"enabled":false}}}"#);
        let scene = build_spec(&json);
        assert_eq!(
            polyline_len(&scene),
            5000,
            "enabled:false must keep all points (byte-identity path)"
        );
    }

    #[test]
    fn small_sparkline_below_threshold_keeps_all_points() {
        let scene =
            build_spec(r#"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4,1,5,9,2,6]}]}}"#);
        assert_eq!(polyline_len(&scene), 8, "below threshold: no decimation");
    }

    #[test]
    fn huge_sparkline_area_fire_path_is_decimated() {
        // fill:true → ser.area が真になり area の Prim::Path が生成される。
        // 間引きが発動すると area パスの頂点数も系列全体（5000）より大きく減る。
        // 非間引きなら "L " は 5001 個。間引きで 4000 未満に収まることを確認する。
        let data: Vec<String> = (0..5000).map(|i| ((i * 7) % 13).to_string()).collect();
        let scene = build_spec(&format!(
            r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{}],"fill":true}}]}}}}"#,
            data.join(",")
        ));
        // area パスと折れ線の両方が間引き後の点列から描かれている。
        assert!(
            polyline_len(&scene) < 5000,
            "area fire-path: polyline must be reduced"
        );
        assert!(
            area_path_l_count(&scene) < 4000,
            "area path vertices must be far below the undecimated 5001"
        );
    }

    #[test]
    fn sparkline_decimation_is_deterministic() {
        let json = huge_sparkline_json("");
        let a = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        let b = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        assert_eq!(a, b, "same input must yield identical SVG");
    }

    #[test]
    fn sparkline_lttb_reduces_to_samples_cap() {
        let json = huge_sparkline_json(
            r#","options":{"plugins":{"decimation":{"algorithm":"lttb","samples":200}}}"#,
        );
        let scene = build_spec(&json);
        assert!(polyline_len(&scene) <= 200, "lttb should hit samples cap");
    }

    #[test]
    fn huge_sparkline_with_tension_still_decimates_and_renders() {
        let data: Vec<String> = (0..5000).map(|i| ((i * 7) % 13).to_string()).collect();
        let json = format!(
            r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{}],"tension":0.4}}]}}}}"#,
            data.join(",")
        );
        let svg = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        assert!(svg.contains("<path"), "tension → Bezier path");
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        // 間引きが実際に起きたことを直接確認する（SVG サイズという弱い代理指標ではなく）。
        // 非間引きの 5000 点 Catmull-Rom は ~4999 個の "C " を出す。間引きで大きく減る。
        // 既定アルゴリズム（minMax）はこの系列を ~2500 点へ落とすため 4000 未満に収まる。
        let bezier_count = svg.matches("C ").count();
        assert!(
            bezier_count < 4000,
            "decimation must cut Bezier commands well below the undecimated ~4999: got {bezier_count}"
        );
    }
}
