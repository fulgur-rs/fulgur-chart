use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).unwrap();
    render_chart(&spec)
}

/// 縦積み上げ: 2 系列 [10,20] + [5,15] の y 軸はグループ最大値(20)ではなく
/// 積み上げ合計(35)を反映し、目盛り上限が 35 まで伸びる。
#[test]
fn vertical_stacked_axis_reflects_total() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"s1","data":[10,20]},{"label":"s2","data":[5,15]}]},"options":{"scales":{"y":{"stacked":true}}}}"#,
    );
    // 積み上げ合計 = max(15, 35) = 35 → nice_ticks(0,35,10) は step=5, 上限=35。
    // 非積み上げ(グループ)なら上限は 20 で、>35</text> は現れない。
    assert!(
        svg.contains(">35</text>"),
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

/// 縦積み上げ + データラベル: セグメント中央(box 中心)へのラベル y 座標を
/// フル SVG スナップショットで固定する。各カテゴリで 2 系列が積み上がるため、
/// 同一カテゴリ内の上下セグメントは異なる中点 y を持つ(box 中心式の回帰防止)。
/// 値は 123/87 等の非丸値で nice_ticks 目盛りと衝突させない。
#[test]
fn vertical_stacked_with_datalabels_snapshot() {
    let svg = render(
        r#"{"type":"bar","data":{"labels":["Q1","Q2","Q3"],"datasets":[{"label":"製品A","data":[123,87,64]},{"label":"製品B","data":[41,93,72]}]},"options":{"scales":{"y":{"stacked":true}},"plugins":{"datalabels":{"display":true}}}}"#,
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

/// 積み上げでも per-data-point の backgroundColor 配列はカテゴリ index で参照される
/// (系列 index ではない)。通常棒と同じ fill_at(i) の挙動。
#[test]
fn stacked_uses_per_category_color() {
    let svg = render(
        r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"s1","data":[10,20],"backgroundColor":["#112233","#445566"]}]},"options":{"scales":{"y":{"stacked":true}}}}"##,
    );
    // カテゴリ A は #112233、カテゴリ B は #445566 で塗られる(系列 index 0 の色固定ではない)。
    assert!(svg.contains("#112233"), "カテゴリ0の色: {svg}");
    assert!(svg.contains("#445566"), "カテゴリ1の色: {svg}");
}
