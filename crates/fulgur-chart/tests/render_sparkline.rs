use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn sparkline_has_polyline() {
    let svg = render(r#"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4,1,5,9,2,6]}]}}"#);
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "折れ線要素がない"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn sparkline_has_no_text_elements() {
    let svg = render(r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3]}]}}"#);
    assert!(
        !svg.contains("<text"),
        "軸ラベルや凡例の <text> が存在してはならない"
    );
}

#[test]
fn sparkline_has_no_markers() {
    let svg = render(r#"{"type":"sparkline","data":{"datasets":[{"data":[1,2,3,4,5]}]}}"#);
    assert!(
        !svg.contains("<circle"),
        "マーカー <circle> が存在してはならない"
    );
}

#[test]
fn sparkline_area_fill() {
    let svg = render(r#"{"type":"sparkline","data":{"datasets":[{"data":[1,3,2],"fill":true}]}}"#);
    assert!(svg.contains("<path"), "fill:true で area パスがない");
    assert!(svg.contains("Z\""), "area パスが閉じていない");
}

#[test]
fn sparkline_tension_uses_bezier() {
    let svg =
        render(r#"{"type":"sparkline","data":{"datasets":[{"data":[1,3,2],"tension":0.4}]}}"#);
    assert!(svg.contains("<path"), "tension で Bezier パスがない");
    assert!(
        svg.contains(" C "),
        "Catmull-Rom コントロールポイントがない"
    );
}

#[test]
fn sparkline_deterministic() {
    let j = r#"{"type":"sparkline","data":{"datasets":[{"data":[5,3,8,2,7]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn sparkline_snapshot() {
    let svg = render(
        r##"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4,1,5,9,2,6],"borderColor":"#4c8cff"}]}}"##,
    );
    insta::assert_snapshot!(svg);
}
