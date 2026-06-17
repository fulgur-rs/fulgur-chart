use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).unwrap();
    render_chart(&spec)
}

#[test]
fn bar_has_rects_and_valid_svg() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"label":"売上","data":[120,200,150]}]}}"#,
    );
    assert!(svg.matches("<rect").count() >= 3, "svg: {svg}");
    assert!(svg.starts_with("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    // 座標に NaN/inf が出ていないこと
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn bar_contains_labels_and_title() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},"options":{"plugins":{"title":{"display":true,"text":"TITLE_X"}}}}"#,
    );
    assert!(svg.contains(">A</text>"));
    assert!(svg.contains(">B</text>"));
    assert!(svg.contains(">TITLE_X</text>"));
    assert!(svg.contains(">0</text>"));
}

#[test]
fn bar_multi_series() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"s1","data":[10,20]},{"label":"s2","data":[15,25]}]}}"#,
    );
    assert!(svg.matches("<rect").count() >= 4);
    assert!(svg.contains(">s1</text>") && svg.contains(">s2</text>"));
}

#[test]
fn bar_deterministic() {
    let json = r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]}}"#;
    assert_eq!(render(json), render(json));
}

#[test]
fn bar_snapshot() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"plugins":{"title":{"display":true,"text":"四半期売上"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn horizontal_bar_differs_from_vertical_and_is_valid() {
    let vert =
        render(r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]}}"#);
    let horiz = render(
        r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},"options":{"indexAxis":"y"}}"#,
    );
    assert_ne!(vert, horiz, "横棒は縦棒と異なる出力になるべき");
    assert!(horiz.matches("<rect").count() >= 2);
    assert!(!horiz.contains("NaN") && !horiz.contains("inf"));
    assert!(horiz.contains(">A</text>") && horiz.contains(">B</text>"));
    assert!(horiz.starts_with("<svg") && horiz.trim_end().ends_with("</svg>"));
}

#[test]
fn horizontal_bar_snapshot() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["東","西","南"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"indexAxis":"y","plugins":{"title":{"display":true,"text":"地域別"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
