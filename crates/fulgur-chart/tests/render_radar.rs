use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

const RADAR: &str = r#"{"type":"radar","data":{"labels":["速度","力","技"],"datasets":[
    {"label":"A","data":[60,80,40]},
    {"label":"B","data":[50,30,90]}]}}"#;

#[test]
fn radar_has_series_polygons() {
    let svg = render(RADAR);
    // 系列多角形は半透明塗り(fill-opacity="0.2")で識別する。グリッドは fill="none"。
    assert!(
        svg.matches(r#"fill-opacity="0.2""#).count() >= 2,
        "got: {svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn radar_shows_category_labels() {
    let svg = render(RADAR);
    assert!(svg.contains(">速度</text>"));
    assert!(svg.contains(">力</text>"));
    assert!(svg.contains(">技</text>"));
}

#[test]
fn radar_draws_grid() {
    let svg = render(RADAR);
    // 多角形グリッド/スポーク線はテーマのグリッド色 #e0e0e0。
    assert!(svg.contains("#e0e0e0"), "got: {svg}");
}

#[test]
fn radar_has_vertex_markers() {
    let svg = render(RADAR);
    // 系列ごとに n(=3) 頂点マーカー(circle r=3) を持つ。
    assert!(svg.matches(r#"<circle"#).count() >= 6, "got: {svg}");
    assert!(svg.contains(r#"r="3""#));
}

#[test]
fn radar_zero_data_does_not_panic() {
    let svg = render(
        r#"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[0,0,0]}]}}"#,
    );
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn radar_deterministic() {
    assert_eq!(render(RADAR), render(RADAR));
}

#[test]
fn radar_snapshot() {
    let svg = render(
        r#"{"type":"radar","data":{"labels":["速度","力","技"],"datasets":[
            {"label":"A","data":[60,80,40]},
            {"label":"B","data":[50,30,90]}]},
            "options":{"plugins":{"title":{"display":true,"text":"能力"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
