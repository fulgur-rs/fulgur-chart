//! Axis title / grid / border 機能の end-to-end fixture snapshot テスト。
//!
//! Chart.js JSON → Schema → IR → SVG まで一貫して軸装飾が反映されることを
//! `examples/specs/axis-*.json` の 3 fixture で検証する。plan `docs/plans/2026-07-20-axis-title-grid-border.md`
//! Task 13 の受入検証。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

#[test]
fn axis_title_basic_snapshot() {
    // bar: options.scales.{x,y}.title.text + Y title は color / font.size 付き。
    // Y タイトルは -90deg 回転して描画され、X タイトルは水平テキストで描画される。
    let json = include_str!("../../../examples/specs/axis-title-basic.json");
    let svg = render(json);
    // sanity: 回転Y title・水平X title 両方が Prim::Text として存在すること。
    assert!(
        svg.contains(">売上 (万円)</text>"),
        "Y title text should render; svg={svg}"
    );
    assert!(svg.contains(">月</text>"), "X title text should render");
    assert!(
        svg.contains("rotate(-90"),
        "Y title should be rotated -90deg; svg={svg}"
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn axis_grid_color_snapshot() {
    // line: options.scales.y.grid.{color,lineWidth} で水平グリッド線の色と太さが変わる。
    let json = include_str!("../../../examples/specs/axis-grid-color.json");
    let svg = render(json);
    assert!(
        svg.contains("stroke=\"#ffe4e4\""),
        "grid.color should reach SVG; svg={svg}"
    );
    // lineWidth=2 が水平グリッドの stroke-width に伝わっていること。既定は 1。
    // baseline/tick は既定太さ 1 のままなので、"stroke-width=\"2\"" は grid 由来のみ。
    assert!(
        svg.contains("stroke-width=\"2\""),
        "grid.lineWidth=2 should reach SVG; svg={svg}"
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn axis_border_dashed_snapshot() {
    // line: options.scales.x.border.{color,width,dash} で X 軸ベースラインが破線・
    // 指定色 (#666) / 太さ (2) で描画される。
    let json = include_str!("../../../examples/specs/axis-border-dashed.json");
    let svg = render(json);
    assert!(
        svg.contains("stroke-dasharray=\"4 4\""),
        "border.dash should render as stroke-dasharray; svg={svg}"
    );
    assert!(
        svg.contains("stroke=\"#666666\""),
        "border.color should reach SVG (#666 は #666666 に正規化); svg={svg}"
    );
    insta::assert_snapshot!(svg);
}
