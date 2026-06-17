use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;
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
