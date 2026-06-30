use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::line;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
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
