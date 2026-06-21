use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn polar_area_basic_renders() {
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
    );
    assert!(svg.matches("<path").count() >= 3);
    assert!(svg.contains(" A "));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn polar_area_equal_angles() {
    // [10,10,10] → 3 slices each 120°, large-arc-flag must be 0
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,10,10]}]}}"#,
    );
    assert!(!svg.contains("NaN"));
    // Each 120° slice: large-arc-flag = 0 (< 180°)
    // Count "0 1" sweep patterns (laf=0, sweep=1) for clockwise arcs
    let clockwise_small_arcs = svg.matches("0 1 ").count();
    assert!(
        clockwise_small_arcs >= 3,
        "Expected at least 3 small-arc clockwise slices, got {clockwise_small_arcs}"
    );
}

#[test]
fn polar_area_radius_proportional_to_value() {
    // [100, 50] → second slice radius ~half of first
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[100,50]}]}}"#,
    );
    assert!(!svg.contains("NaN"));
    // SVG arc command: "A rx ry 0 laf sweep x y" — extract rx values to verify radius ratio.
    let radii: Vec<f64> = svg
        .split('A')
        .skip(1)
        .filter_map(|seg| seg.split_whitespace().next()?.parse::<f64>().ok())
        .collect();
    assert!(radii.len() >= 2, "arc半径を2つ以上抽出できませんでした");
    let ratio = radii[1] / radii[0];
    assert!(
        (ratio - 0.5).abs() < 0.1,
        "期待比率 0.5 に対して実測は {ratio}"
    );
}

#[test]
fn polar_area_zero_values_dont_panic() {
    let svg =
        render(r#"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[0,0]}]}}"#);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn polar_area_single_value_full_circle() {
    // single slice = 360° → split into 2
    let svg =
        render(r#"{"type":"polarArea","data":{"labels":["only"],"datasets":[{"data":[5]}]}}"#);
    assert!(svg.matches("<path").count() >= 2);
    assert!(!svg.contains("NaN"));
}

#[test]
fn polar_area_uses_per_slice_colors() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"backgroundColor":["#ff0000","#0000ff"]}]}}"##,
    );
    assert!(svg.contains("#ff0000") && svg.contains("#0000ff"));
}

#[test]
fn polar_area_legend_shows_categories() {
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["Apple","Banana"],"datasets":[{"data":[1,2]}]}}"#,
    );
    assert!(svg.contains(">Apple</text>") && svg.contains(">Banana</text>"));
}

#[test]
fn polar_area_deterministic() {
    let j =
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn polar_area_snapshot() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],"datasets":[{"data":[30,80,50,20],"backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},"options":{"plugins":{"title":{"display":true,"text":"季節別データ"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}
