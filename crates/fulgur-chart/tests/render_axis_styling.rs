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

fn numeric_texts(svg: &str) -> Vec<f64> {
    let mut values: Vec<f64> = svg
        .split("</text>")
        .filter_map(|part| part.rsplit_once('>').map(|(_, text)| text))
        .filter_map(|text| text.parse().ok())
        .collect();
    values.sort_by(f64::total_cmp);
    values
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

#[test]
fn linear_axis_step_size_generates_fixed_ticks() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"scales":{"y":{"min":0,"max":10,"ticks":{"stepSize":5}}}}}"#;
    let svg = render(json);

    assert_eq!(numeric_texts(&svg), vec![0.0, 5.0, 10.0]);
}

#[test]
fn linear_axis_step_size_without_max_ticks_limit_preserves_requested_spacing() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,100]}]},
      "options":{"scales":{"y":{"min":0,"max":100,"ticks":{"stepSize":1}}}}}"#;
    let ticks = numeric_texts(&render(json));

    assert_eq!(ticks.len(), 101);
    assert_eq!(ticks.first(), Some(&0.0));
    assert_eq!(ticks.last(), Some(&100.0));
    assert!(ticks.windows(2).all(|pair| pair[1] - pair[0] == 1.0));
}

#[test]
fn linear_axis_max_ticks_limit_caps_generated_tick_count() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,100]}]},
      "options":{"scales":{"y":{"min":0,"max":100,"ticks":{"stepSize":1,"maxTicksLimit":3}}}}}"#;
    let svg = render(json);

    assert_eq!(numeric_texts(&svg), vec![0.0, 50.0, 100.0]);
}

#[test]
fn linear_axis_count_sets_tick_count_and_precision_rounds_step() {
    let counted = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"scales":{"y":{"min":0,"max":10,"ticks":{"count":5}}}}}"#;
    assert_eq!(
        numeric_texts(&render(counted)),
        vec![0.0, 2.5, 5.0, 7.5, 10.0]
    );

    let precise = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,0.3]}]},
      "options":{"scales":{"y":{"min":0,"max":0.3,"ticks":{"precision":0}}}}}"#;
    assert_eq!(numeric_texts(&render(precise)), vec![0.0, 0.3]);
}

#[test]
fn linear_axis_step_size_takes_precedence_over_count_for_matching_hard_bounds() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"scales":{"y":{"min":0,"max":10,"ticks":{"stepSize":2,"count":3}}}}}"#;
    let svg = render(json);

    assert_eq!(numeric_texts(&svg), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
}

#[test]
fn linear_axis_count_one_generates_one_tick() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"scales":{"y":{"min":0,"max":10,"ticks":{"count":1}}}}}"#;
    let svg = render(json);

    assert_eq!(numeric_texts(&svg), vec![10.0]);
}

#[test]
fn linear_axis_count_one_keeps_valid_domain_for_conflicting_bounds() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"scales":{"y":{"min":10,"max":0,"ticks":{"count":1}}}}}"#;

    assert_eq!(numeric_texts(&render(json)), vec![10.5]);
}

#[test]
fn linear_axis_format_sets_fraction_digits_and_notation() {
    let fixed = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,2]}]},
      "options":{"scales":{"y":{"min":0,"max":2,"ticks":{"stepSize":2,
        "format":{"minimumFractionDigits":2,"maximumFractionDigits":2}}}}}}"#;
    let fixed_svg = render(fixed);
    assert!(fixed_svg.contains(">0.00</text>"), "svg={fixed_svg}");
    assert!(fixed_svg.contains(">2.00</text>"), "svg={fixed_svg}");

    let scientific = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10000]}]},
      "options":{"scales":{"y":{"min":0,"max":10000,"ticks":{"stepSize":10000,
        "format":{"notation":"scientific"}}}}}}"#;
    let scientific_svg = render(scientific);
    assert!(
        scientific_svg.contains(">1E4</text>"),
        "svg={scientific_svg}"
    );

    let engineering = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,10000]}]},
      "options":{"scales":{"y":{"min":0,"max":10000,"ticks":{"stepSize":10000,
        "format":{"notation":"engineering"}}}}}}"#;
    assert!(render(engineering).contains(">10E3</text>"));

    let compact = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,5000]}]},
      "options":{"scales":{"y":{"min":0,"max":5000,"ticks":{"stepSize":5000,
        "format":{"notation":"compact"}}}}}}"#;
    assert!(render(compact).contains(">5K</text>"));

    let compact_rounding = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[0,987654321]}]},
      "options":{"scales":{"y":{"min":0,"max":987654321,"ticks":{"stepSize":987654321,
        "format":{"notation":"compact"}}}}}}"#;
    assert!(render(compact_rounding).contains(">988M</text>"));
}

#[test]
fn linear_x_axis_tick_options_apply_to_scatter_and_horizontal_bar() {
    let scatter = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":0,"y":0},{"x":10,"y":10}]}]},
      "options":{"scales":{"x":{"min":0,"max":10,"ticks":{"stepSize":5}},
        "y":{"min":0,"max":10,"ticks":{"count":3}}}}}"#;
    assert_eq!(
        numeric_texts(&render(scatter)),
        vec![0.0, 0.0, 5.0, 5.0, 10.0, 10.0]
    );

    let horizontal_bar = r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[0,10]}]},
      "options":{"indexAxis":"y","scales":{"x":{"min":0,"max":10,"ticks":{"stepSize":5}}}}}"#;
    assert_eq!(numeric_texts(&render(horizontal_bar)), vec![0.0, 5.0, 10.0]);
}
