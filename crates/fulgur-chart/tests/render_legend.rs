use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn bar_legend_left_renders_series_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x","y"],
      "datasets":[{"label":"売上","data":[1,2]},{"label":"原価","data":[1,1]}]},
      "options":{"plugins":{"legend":{"position":"left"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">売上</text>"));
    assert!(svg.contains(">原価</text>"));
    // determinism
    assert_eq!(svg, render(json));
}

#[test]
fn bar_legend_right_renders_series_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x"],
      "datasets":[{"label":"売上","data":[1]},{"label":"原価","data":[2]}]},
      "options":{"plugins":{"legend":{"position":"right"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">売上</text>"));
    assert!(svg.contains(">原価</text>"));
}

#[test]
fn legend_display_false_no_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x"],
      "datasets":[{"label":"売上","data":[1]}]},
      "options":{"plugins":{"legend":{"display":false,"position":"left"}}}}"#;
    assert!(!render(json).contains(">売上</text>"));
}

#[test]
fn left_differs_from_top() {
    let base = r#"{"type":"bar","data":{"labels":["x"],"datasets":[{"label":"売上","data":[1]}]},"options":{"plugins":{"legend":{"position":"POS"}}}}"#;
    let left = render(&base.replace("POS", "left"));
    let top = render(&base.replace("POS", "top"));
    assert_ne!(left, top, "left 凡例は帯確保で top と出力が異なるはず");
}

#[test]
fn pie_legend_right_renders_category_labels() {
    let json = r#"{"type":"pie","data":{"labels":["りんご","みかん"],"datasets":[{"data":[3,1]}]},
      "options":{"plugins":{"legend":{"position":"right"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">りんご</text>"));
    assert!(svg.contains(">みかん</text>"));
}
