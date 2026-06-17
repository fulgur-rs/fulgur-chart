use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).unwrap();
    render_chart(&spec)
}

/// 縦積み上げ: 2 系列 [10,20] + [5,15] の y 軸はグループ最大値(20)ではなく
/// 積み上げ合計(35)を反映し、目盛り上限が 40 まで伸びる。
#[test]
fn vertical_stacked_axis_reflects_total() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"s1","data":[10,20]},{"label":"s2","data":[5,15]}]},"options":{"scales":{"y":{"stacked":true}}}}"#,
    );
    // 積み上げ合計 = max(15, 35) = 35 → nice_ticks(0,35,5) は step=10, 上限=40。
    // 非積み上げ(グループ)なら上限は 20 で、>40</text> は現れない。
    assert!(
        svg.contains(">40</text>"),
        "y軸上限が積み上げ合計を反映していない: {svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn vertical_stacked_snapshot() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["Q1","Q2","Q3"],"datasets":[{"label":"製品A","data":[10,20,15]},{"label":"製品B","data":[5,15,25]}]},"options":{"scales":{"y":{"stacked":true}},"plugins":{"title":{"display":true,"text":"積み上げ売上"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn horizontal_stacked_snapshot() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["東","西","南"],"datasets":[{"label":"製品A","data":[10,20,15]},{"label":"製品B","data":[5,15,25]}]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true}},"plugins":{"title":{"display":true,"text":"地域別積み上げ"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn stacked_deterministic() {
    let json = r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"s1","data":[10,20]},{"label":"s2","data":[5,15]}]},"options":{"scales":{"y":{"stacked":true}}}}"#;
    assert_eq!(render(json), render(json));
}
