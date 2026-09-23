use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::line;
use fulgur_chart::raster_direct::{render_chart_to_png, render_chart_to_webp};
use fulgur_chart::render::{render_chart, render_chart_with_font};
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

/// PNG バイト列（scale 1.0）。決定性・SVG↔PNG 一致テスト用。
fn render_png(json: &str) -> Vec<u8> {
    render_chart_to_png(&chartjs::parse(json, false).unwrap(), 1.0, DEFAULT_FONT).unwrap()
}

fn line_scene(json: &str) -> fulgur_chart::scene::Scene {
    let spec = chartjs::parse(json, false).unwrap();
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    line::build(&spec, &measurer)
}

fn area_paths(scene: &fulgur_chart::scene::Scene) -> Vec<(&str, fulgur_chart::ir::Color)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Path {
                d,
                fill: Some(fill),
                ..
            } => Some((d.as_str(), *fill)),
            _ => None,
        })
        .collect()
}

fn line_points_by_color(scene: &fulgur_chart::scene::Scene, rgb: (u8, u8, u8)) -> Vec<(f64, f64)> {
    scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Polyline { points, stroke, .. } if (stroke.r, stroke.g, stroke.b) == rgb => {
                Some(points.clone())
            }
            _ => None,
        })
        .expect("line polyline")
}

fn assert_area_tracks_target(area: &str, target: &[(f64, f64)]) {
    let target_edge = target
        .iter()
        .rev()
        .map(|(x, y)| {
            format!(
                "L {} {}",
                fulgur_chart::num::fmt_num(*x),
                fulgur_chart::num::fmt_num(*y)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        area.contains(&target_edge),
        "area path does not follow target: {area}"
    );
}

#[test]
fn line_has_polyline_and_markers() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C"],"datasets":[{"label":"s","data":[1,3,2]}]}}"#,
    );
    assert!(svg.contains("<polyline"));
    assert!(svg.matches("<circle").count() >= 3); // 各点にマーカー
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn area_emits_filled_path_with_opacity() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[1,2],"fill":true}]}}"#,
    );
    assert!(svg.contains("<path"));
    assert!(svg.contains("fill-opacity=")); // 半透明 area
    assert!(svg.contains("Z\"")); // 閉じたパス
}

#[test]
fn relative_area_fill_closes_to_the_referenced_dataset() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#0000ff","fill":false},
      {"data":[3,1],"borderColor":"#ff0000","fill":"-1"}
    ]}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;
    assert_area_tracks_target(area, &target);
}

#[test]
fn absolute_area_fill_index_closes_to_that_dataset() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#0000ff","fill":false},
      {"data":[2,2],"borderColor":"#00aa00","fill":false},
      {"data":[3,1],"borderColor":"#ff0000","fill":0}
    ]}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;

    assert_area_tracks_target(area, &target);
}

#[test]
fn positive_relative_area_fill_closes_to_the_next_dataset() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[3,1],"borderColor":"#ff0000","fill":"+1"},
      {"data":[1,3],"borderColor":"#0000ff","fill":false}
    ]}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;

    assert_area_tracks_target(area, &target);
}

#[test]
fn stack_area_fill_closes_to_the_line_below() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#0000ff","fill":false},
      {"data":[3,1],"borderColor":"#ff0000","fill":"stack"}
    ]}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;

    assert_area_tracks_target(area, &target);
}

#[test]
fn value_area_fill_closes_at_the_requested_axis_value() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#ff0000","fill":{"value":2}}
    ]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = fulgur_chart::layout::common::compute(&spec, &measurer);
    let scene = line::build(&spec, &measurer);
    let source = line_points_by_color(&scene, (255, 0, 0));
    let area = area_paths(&scene).first().expect("area polygon").0;
    let target_y = fulgur_chart::num::fmt_num(frame.ys.map(2.0));
    let first_x = fulgur_chart::num::fmt_num(source[0].0);
    let last_x = fulgur_chart::num::fmt_num(source[source.len() - 1].0);
    let target_edge = format!("L {last_x} {target_y} L {first_x} {target_y} Z");

    assert!(area.contains(&target_edge), "value target not used: {area}");
}

#[test]
fn start_and_end_area_targets_follow_plot_bounds() {
    for (mode, use_plot_bottom) in [("start", true), ("end", false)] {
        let json = format!(
            r##"{{"type":"line","data":{{"labels":["A","B"],"datasets":[
              {{"data":[1,3],"borderColor":"#ff0000","fill":"{mode}"}}
            ]}}}}"##
        );
        let spec = chartjs::parse(&json, false).unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = fulgur_chart::layout::common::compute(&spec, &measurer);
        let scene = line::build(&spec, &measurer);
        let source = line_points_by_color(&scene, (255, 0, 0));
        let area = area_paths(&scene).first().expect("area polygon").0;
        let target_y = if use_plot_bottom {
            frame.plot_bottom
        } else {
            frame.plot_top
        };
        let target_y = fulgur_chart::num::fmt_num(target_y);
        let first_x = fulgur_chart::num::fmt_num(source[0].0);
        let last_x = fulgur_chart::num::fmt_num(source[source.len() - 1].0);
        let target_edge = format!("L {last_x} {target_y} L {first_x} {target_y} Z");

        assert!(
            area.contains(&target_edge),
            "{mode} target not used: {area}"
        );
    }
}

#[test]
fn stacked_stack_fill_closes_to_the_current_series_stack_base() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#0000ff","fill":false},
      {"data":[3,1],"borderColor":"#ff0000","fill":"stack"}
    ]},"options":{"scales":{"y":{"stacked":true}}}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;

    assert_area_tracks_target(area, &target);
}

#[test]
fn invalid_area_target_disables_the_fill() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"fill":"+1"}
    ]}}"##;
    let scene = line_scene(json);

    assert!(
        area_paths(&scene).is_empty(),
        "invalid target produced a fill"
    );
}

#[test]
fn above_and_below_colors_split_a_crossing_fill() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[3,1],"borderColor":"#ff0000","fill":false},
      {"data":[1,3],"borderColor":"#00aa00","fill":{
        "target":0,"above":"#ff0000","below":"#0000ff"
      }}
    ]}}"##;
    let scene = line_scene(json);
    let colors: Vec<_> = area_paths(&scene)
        .iter()
        .map(|(_, color)| (color.r, color.g, color.b))
        .collect();

    assert!(
        colors.contains(&(255, 0, 0)),
        "missing above color: {colors:?}"
    );
    assert!(
        colors.contains(&(0, 0, 255)),
        "missing below color: {colors:?}"
    );
}

#[test]
fn colored_fill_accepts_a_value_object_as_its_target() {
    let json = r##"{"type":"line","data":{"labels":["A","B"],"datasets":[
      {"data":[1,3],"borderColor":"#00aa00","fill":{
        "target":{"value":2},"above":"#ff0000","below":"#0000ff"
      }}
    ]}}"##;
    let scene = line_scene(json);
    let colors: Vec<_> = area_paths(&scene)
        .iter()
        .map(|(_, color)| (color.r, color.g, color.b))
        .collect();

    assert!(
        colors.contains(&(255, 0, 0)),
        "missing above color: {colors:?}"
    );
    assert!(
        colors.contains(&(0, 0, 255)),
        "missing below color: {colors:?}"
    );
}

#[test]
fn fill_to_dataset_does_not_bridge_a_target_gap() {
    let json = r##"{"type":"line","data":{"labels":["A","B","C","D"],"datasets":[
      {"data":[0,1,null,0],"borderColor":"#0000ff","fill":false},
      {"data":[1,null,3,2],"borderColor":"#ff0000","fill":0}
    ]}}"##;
    let scene = line_scene(json);

    assert!(area_paths(&scene).is_empty(), "fill crossed a target gap");
}

#[test]
fn fill_to_span_gaps_target_interpolates_through_missing_values() {
    let json = r##"{"type":"line","data":{"labels":["A","B","C"],"datasets":[
      {"data":[1,null,3],"spanGaps":true,"borderColor":"#0000ff","fill":false},
      {"data":[3,2,1],"borderColor":"#ff0000","fill":0}
    ]}}"##;
    let scene = line_scene(json);
    let target = line_points_by_color(&scene, (0, 0, 255));
    let area = area_paths(&scene).first().expect("area polygon").0;
    let middle = (
        (target[0].0 + target[1].0) / 2.0,
        (target[0].1 + target[1].1) / 2.0,
    );
    let expected_target_point = format!(
        "L {} {}",
        fulgur_chart::num::fmt_num(middle.0),
        fulgur_chart::num::fmt_num(middle.1)
    );

    assert!(
        area.contains(&expected_target_point),
        "spanGaps target was not interpolated: {area}"
    );
}

#[test]
fn tension_uses_bezier_path() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C"],"datasets":[{"data":[1,3,2],"tension":0.4}]}}"#,
    );
    assert!(svg.contains("<path")); // 曲線はpath
    assert!(svg.contains(" C ")); // ベジエコマンド
}

#[test]
fn line_deterministic() {
    let j = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn line_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"plugins":{"title":{"display":true,"text":"推移"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn area_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["Q1","Q2","Q3"],"datasets":[{"label":"累計","data":[30,75,130],"fill":true}]}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn tension_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C","D"],"datasets":[{"label":"曲線","data":[1,3,2,4],"tension":0.4}]}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn offset_snapshot() {
    // chart.js options.scales.x.offset:true: 点・ラベルを band 中心へ寄せ、bar と同じ
    // chartArea(端余白なし)で描く。既定の edge-to-edge とは別の出力。
    let svg = render(
        r#"{"type":"line","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"scales":{"x":{"offset":true}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

// --- デシメーション配線（Task 7/8）---

/// scene 内の全 Polyline の点数合計。間引きで線が消えていないこと/間引かれたことの検証に使う。
fn polyline_pts(json: &str) -> usize {
    let spec = chartjs::parse(json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &m);
    scene
        .items
        .iter()
        .filter_map(|p| match p {
            Prim::Polyline { points, .. } => Some(points.len()),
            _ => None,
        })
        .sum()
}

#[test]
fn large_line_is_decimated_vs_disabled() {
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let body = format!(
        "\"labels\":[{}],\"datasets\":[{{\"data\":[{}]}}]",
        labels.join(","),
        data.join(",")
    );
    let on = format!(r#"{{"type":"line","data":{{{body}}}}}"#);
    let off = format!(
        r#"{{"type":"line","data":{{{body}}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#
    );
    let on_pts = polyline_pts(&on);
    let off_pts = polyline_pts(&off);
    assert_eq!(off_pts, n, "disabled must keep all points (single segment)");
    assert!(
        on_pts > 0 && on_pts < off_pts,
        "default must decimate: {on_pts} vs {off_pts}"
    );
}

#[test]
fn small_line_polyline_unchanged() {
    // 3点 line → 間引きされず 3点のまま（既存 golden と整合）。
    let pts = polyline_pts(
        r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#,
    );
    assert_eq!(pts, 3);
}

/// scene 内の Circle（マーカー）数。
fn circle_count(json: &str) -> usize {
    let spec = chartjs::parse(json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &m);
    scene
        .items
        .iter()
        .filter(|p| matches!(p, Prim::Circle { .. }))
        .count()
}

#[test]
fn large_line_suppresses_markers_by_default() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert_eq!(
        circle_count(&json),
        0,
        "large line should suppress markers by default"
    );
}

#[test]
fn large_line_keeps_markers_when_pointradius_set() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}],"pointRadius":2}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert!(
        circle_count(&json) > 0,
        "explicit pointRadius should keep markers"
    );
}

#[test]
fn small_line_markers_unchanged() {
    // 3点 → markers drawn as before (radius 3, not suppressed)。
    assert_eq!(
        circle_count(
            r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#
        ),
        3
    );
}

#[test]
fn explicit_point_radius_zero_suppresses_markers() {
    assert_eq!(
        circle_count(
            r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3],"pointRadius":0}]}}"#
        ),
        0
    );
}

#[test]
fn explicit_point_radius_none_retains_markers() {
    assert_eq!(
        circle_count(
            r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#
        ),
        3
    );
}

#[test]
fn unsupported_point_radius_returns_same_error_for_fallible_svg_png_webp_apis() {
    const ERROR: &str = "pointRadius must be finite and no greater than 32768";
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"pointRadius":1e40}]}}"#,
        false,
    )
    .unwrap();

    let errors = [
        render_chart_with_font(&spec, DEFAULT_FONT).unwrap_err(),
        render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap_err(),
        render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).unwrap_err(),
    ];
    assert_eq!(errors, [ERROR, ERROR, ERROR]);
}

#[test]
fn oversized_point_radius_reviewer_repros_return_clean_png_errors() {
    const ERROR: &str = "pointRadius must be finite and no greater than 32768";
    for (radius, scale) in [(2e9, 1.0), (2e8, 10.0)] {
        let json = format!(
            r#"{{"type":"line","data":{{"labels":["a"],"datasets":[{{"data":[1],"pointRadius":{radius}}}]}}}}"#
        );
        let spec = chartjs::parse(&json, false).unwrap();
        assert_eq!(
            render_chart_to_png(&spec, scale, DEFAULT_FONT),
            Err(ERROR.to_string()),
            "radius={radius}, scale={scale}"
        );
    }
}

#[test]
fn point_radius_at_marker_limit_renders_png() {
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"pointRadius":32768}]}}"#,
        false,
    )
    .unwrap();
    let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
    assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
}

/// scene 内の各 Polyline の点列を順に返す（セグメント数・各セグメント点数・座標有限性の検証用）。
fn polylines(spec: &fulgur_chart::ir::ChartSpec) -> Vec<Vec<(f64, f64)>> {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(spec, &m);
    scene
        .items
        .iter()
        .filter_map(|p| match p {
            Prim::Polyline { points, .. } => Some(points.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn gapped_large_line_keeps_segments_and_decimates() {
    // segment-first 設計の回帰テスト: gap のある巨大系列が、間引き後も
    //   (a) セグメント融合せず ≥2 本の Polyline を保ち、
    //   (b) 間引きが効いて総点数が大幅に減り、
    //   (c) どのセグメントも崩壊・消失しない
    // ことを保証する。素朴な「間引き後に cat で再分割」実装ではここで全点が gap 扱いになり
    // 各点が長さ1セグメントへ割れて Polyline が 0 本になる（(a) が FAIL する）。
    //
    // JSON の data は untagged `Nums(Vec<f64>)` で非有限値を表現できないため（null も 1e400 も
    // parse エラー）、parse 後に中央へ NaN を注入して gap を作る。これは line の gap 分割が
    // 認識する表現そのもの（`valid` フィルタが非有限値を落とし cat 不連続を生む）。
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    let mut spec = chartjs::parse(&json, false).unwrap();
    spec.series[0].values[n / 2] = f64::NAN; // 中央に gap を作る

    let polys = polylines(&spec);
    let counts: Vec<usize> = polys.iter().map(|p| p.len()).collect();

    // (a) gap が保たれ 2 セグメント以上（gap をまたいで融合していない／線が消えていない）。
    assert!(
        counts.len() >= 2,
        "gap must yield >=2 polylines, got {}: {counts:?}",
        counts.len()
    );
    // (c) どのセグメントも 2 点以上（崩壊・消失していない）。
    assert!(
        counts.iter().all(|&c| c >= 2),
        "every polyline must have >=2 points: {counts:?}"
    );
    // (b) 間引きが効いて総点数が大幅に減る（gap で 1 点落ちた n-1 ではなく、明確に半分未満）。
    let total: usize = counts.iter().sum();
    assert!(
        total < n / 2,
        "decimation must substantially reduce total points: {total} vs n={n}"
    );
    // 念のため: y スケールが NaN 汚染されておらず、出力座標がすべて有限。
    assert!(
        polys
            .iter()
            .flatten()
            .all(|&(x, y)| x.is_finite() && y.is_finite()),
        "all emitted polyline points must be finite"
    );
}

#[test]
fn gapped_large_line_lttb_prorates_segment_budget() {
    // 回帰: LTTB × gap 多数セグメント時、per-segment に full samples を与えると
    // 合計 samples×セグメント数 点に膨れる（fulgur-chart-vzd）。セグメント長で
    // 予算を按分し、合計を samples+3×セグメント数 以下に上限化することを検証する。
    // JSON は非有限値を表現できないため parse 後に NaN を注入して gap を作る。
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    // samples/threshold を明示し、確実に LTTB 間引きを発動させる。
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}},"options":{{"plugins":{{"decimation":{{"enabled":true,"algorithm":"lttb","samples":100,"threshold":500}}}}}}}}"#,
        labels.join(","),
        data.join(",")
    );
    let mut spec = chartjs::parse(&json, false).unwrap();
    // 3 箇所に孤立 NaN を注入 → 3 gap → 4 セグメント。
    for p in [n / 4, n / 2, 3 * n / 4] {
        spec.series[0].values[p] = f64::NAN;
    }

    let polys = polylines(&spec);
    let counts: Vec<usize> = polys.iter().map(|p| p.len()).collect();
    let num_seg = counts.len();
    let total: usize = counts.iter().sum();

    // 複数セグメントに割れている（gap が保たれている）。
    assert!(
        num_seg >= 2,
        "gaps must yield >=2 polylines, got {counts:?}"
    );
    // 各セグメントは崩壊していない。
    assert!(
        counts.iter().all(|&c| c >= 2),
        "no segment collapse: {counts:?}"
    );
    // 証明済み上限: samples(=100) + 3×num_segments。素朴実装なら 100×num_seg に膨れる。
    assert!(
        total <= 100 + 3 * num_seg,
        "budget must be prorated across segments: total={total}, num_seg={num_seg}"
    );
    // 素朴 per-segment 予算（samples×num_seg）を明確に下回る。
    assert!(
        total < 100 * num_seg,
        "must beat naive per-segment budget: total={total}"
    );
}

/// spec から Circle マーカー数を数える（NaN 注入など parse 後に変異させた spec 用）。
fn circle_count_spec(spec: &fulgur_chart::ir::ChartSpec) -> usize {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(spec, &m);
    scene
        .items
        .iter()
        .filter(|p| matches!(p, Prim::Circle { .. }))
        .count()
}

#[test]
fn all_singleton_gaps_keep_markers_when_decimated() {
    // 回帰: 非有限値で全ての有限点が「孤立点(単点セグメント)」になる巨大系列では、
    // resolve は有限点数(>threshold)で間引きを有効化するが、単点セグメントは Polyline を
    // 出さない。マーカーまで一律抑制すると線もマーカーも無い「空チャート」になる(以前は
    // 点として見えていた回帰)。単点セグメントの孤立点はマーカーを保持すること。
    // JSON は非有限値を表現できないため parse 後に奇数 index を NaN 化して孤立点を作る。
    let n = 12000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    let mut spec = chartjs::parse(&json, false).unwrap();
    // 奇数 index を NaN 化 → 偶数 index の有限点はすべて cat 不連続の単点セグメント。
    for i in (1..n).step_by(2) {
        spec.series[0].values[i] = f64::NAN;
    }
    // 有限点(=n/2=6000)は threshold を超え間引きが有効化されるが、全て単点 → Polyline は 0 本。
    assert!(polylines(&spec).is_empty(), "all singletons → no polyline");
    // マーカーは孤立点の唯一の表現なので保持される（空チャートにしない）。
    assert!(
        circle_count_spec(&spec) > 0,
        "singleton (undrawable-as-line) points must keep markers when decimated"
    );
}

// --- 決定性・no-op サニティ・SVG↔PNG 一致（Task 9）---

#[test]
fn disabled_decimation_keeps_all_points_sanity() {
    // サニティ: enabled:false の巨大 line は単一セグメント全点を保持し、間引きされない。
    // （pre-feature バイト不変の真の保証は threshold 未満で緑のままの既存小 golden。
    //   これは passthrough = 非間引きと同形であることの確認のみ。）
    let n = 3000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 13) % 50)).collect();
    let off = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert_eq!(polyline_pts(&off), n);
}

/// 5000 点（threshold 超過確実）の自動間引き line spec を組み立てる。
fn big_decimated_line_json() -> String {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 29) % 83)).collect();
    format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    )
}

#[test]
fn decimated_line_is_deterministic() {
    // 同一入力 → 同一バイト列（SVG・PNG 双方）。間引き経路の決定性を担保。
    let json = big_decimated_line_json();
    assert_eq!(render(&json), render(&json), "SVG must be byte-identical");
    assert_eq!(
        render_png(&json),
        render_png(&json),
        "PNG must be byte-identical"
    );
}

#[test]
fn decimated_line_renders_svg_and_png_consistently() {
    // SVG/PNG は build() の同一 Scene を消費するため、間引き-on の line が
    // 両出力でエラー無く・決定的にレンダされることを確認（geometry 共有の担保）。
    let json = big_decimated_line_json();
    let svg = render(&json);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.contains("<polyline"));

    let png = render_png(&json);
    assert!(png.len() > 8, "PNG must be non-empty");
    // 妥当な PNG であること（デコードできる）を tiny-skia で確認。
    let pix = tiny_skia::Pixmap::decode_png(&png).expect("decimated PNG must decode");
    assert!(pix.width() > 0 && pix.height() > 0);
}
