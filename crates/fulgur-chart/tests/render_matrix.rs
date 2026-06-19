use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn matrix_renders_correct_rect_count() {
    // 2 列 × 2 行 = 4 セル
    let svg = render(
        r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":2},
        {"x":"A","y":"Y","v":3},{"x":"B","y":"Y","v":4}
    ]}]}}"#,
    );
    let rect_count = svg.matches("<rect").count();
    assert_eq!(
        rect_count, 4,
        "2x2 matrix should have 4 rects, got: {rect_count}\n{svg}"
    );
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn matrix_nan_cell_uses_nan_color() {
    // (B, Y) が欠損 → NaN セルは #e0e0e0
    let svg = render(
        r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":2},
        {"x":"A","y":"Y","v":3}
    ]}]}}"#,
    );
    assert_eq!(
        svg.matches("<rect").count(),
        4,
        "4 rects including NaN cell: {svg}"
    );
    assert!(
        svg.contains("#e0e0e0"),
        "NaN cell should use #e0e0e0: {svg}"
    );
}

#[test]
fn matrix_min_cell_is_white() {
    // min 値(0.0)のセルは白(#ffffff)
    let svg = render(
        r##"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":0},{"x":"B","y":"X","v":10}
    ],"backgroundColor":"#0000ff"}]}}"##,
    );
    assert!(svg.contains("#ffffff"), "min cell should be white: {svg}");
}

#[test]
fn matrix_max_cell_matches_background_color() {
    // max 値のセルは backgroundColor (#0000ff)
    let svg = render(
        r##"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":0},{"x":"B","y":"X","v":10}
    ],"backgroundColor":"#0000ff"}]}}"##,
    );
    assert!(
        svg.contains("#0000ff"),
        "max cell should match backgroundColor: {svg}"
    );
}

#[test]
fn matrix_renders_axis_labels() {
    let svg = render(
        r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8}
    ]}]}}"#,
    );
    assert!(svg.contains(">Mon<"), "x label Mon missing: {svg}");
    assert!(svg.contains(">Tue<"), "x label Tue missing: {svg}");
    assert!(svg.contains(">Morning<"), "y label Morning missing: {svg}");
}

#[test]
fn matrix_renders_title() {
    let svg = render(
        r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1}
    ]}]},"options":{"plugins":{"title":{"display":true,"text":"Weekly Heatmap"}}}}"#,
    );
    assert!(svg.contains("Weekly Heatmap"), "title missing: {svg}");
}

#[test]
fn matrix_deterministic() {
    let j = r##"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8},
        {"x":"Mon","y":"Evening","v":3},{"x":"Tue","y":"Evening","v":9}
    ],"backgroundColor":"rgba(54,162,235,1.0)"}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn matrix_snapshot() {
    let svg = render(
        r##"{"type":"matrix","data":{"datasets":[{"label":"Sales","data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8},{"x":"Wed","y":"Morning","v":3},
        {"x":"Mon","y":"Evening","v":9},{"x":"Tue","y":"Evening","v":2},{"x":"Wed","y":"Evening","v":7}
    ],"backgroundColor":"rgba(54,162,235,1.0)"}]},"options":{"plugins":{"title":{"display":true,"text":"Weekly Heatmap"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}
