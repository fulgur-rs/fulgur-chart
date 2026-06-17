use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn pie_has_one_path_per_slice() {
    let svg = render(
        r#"{"type":"pie","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
    );
    assert!(svg.matches("<path").count() >= 3);
    assert!(svg.contains(" A ")); // 円弧コマンド
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn pie_uses_per_slice_colors() {
    let svg = render(
        r##"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"backgroundColor":["#ff0000","#0000ff"]}]}}"##,
    );
    assert!(svg.contains("#ff0000") && svg.contains("#0000ff"));
}

#[test]
fn doughnut_has_inner_arc() {
    // doughnut は内弧を含む（A が2回/パス、L で内外接続）
    let svg =
        render(r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#);
    assert!(svg.matches(" A ").count() >= 4); // 2スライス×2弧
}

#[test]
fn single_value_full_circle_does_not_panic() {
    let svg = render(r#"{"type":"pie","data":{"labels":["only"],"datasets":[{"data":[5]}]}}"#);
    assert!(svg.matches("<path").count() >= 2); // 全周は2分割
    assert!(!svg.contains("NaN"));
}

#[test]
fn zero_total_does_not_panic() {
    let svg = render(r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[0,0]}]}}"#);
    assert!(svg.starts_with("<svg")); // スライス無しでも有効SVG
    assert!(!svg.contains("NaN"));
}

#[test]
fn pie_legend_shows_categories() {
    let svg = render(
        r#"{"type":"pie","data":{"labels":["Apple","Banana"],"datasets":[{"data":[1,2]}]}}"#,
    );
    assert!(svg.contains(">Apple</text>") && svg.contains(">Banana</text>"));
}

#[test]
fn pie_deterministic() {
    let j = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn pie_snapshot() {
    let svg = render(
        r#"{"type":"doughnut","data":{"labels":["A","B","C"],"datasets":[{"data":[30,50,20]}]},"options":{"plugins":{"title":{"display":true,"text":"内訳"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
