use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn vertical_bar_datalabels_render_values() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["1月","2月","3月"],"datasets":[{"data":[123,87,151]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    // 123/87 は奇数・非5倍数。nice_ticks の目盛り(丸い値)にもカテゴリ名にも
    // 一致しないため、この部分文字列はデータラベル由来とのみ判定できる。
    assert!(
        svg.contains(">123</text>"),
        "datalabel 123 が描画されること"
    );
    assert!(svg.contains(">87</text>"));
}

#[test]
fn vertical_bar_datalabels_render_negative_value() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["a"],"datasets":[{"data":[-87]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    // 負値分岐(棒下端の下にラベル)の検証。-87 は nice_ticks 目盛りにならない番兵。
    assert!(render(json).contains(">-87</text>"));
}

#[test]
fn vertical_bar_without_datalabels_has_no_value_text() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["1月"],"datasets":[{"data":[123]}]}
    }"#;
    // 123 はどの nice_ticks 目盛りにも出ない値なので、無効時は SVG に現れない。
    assert!(
        !render(json).contains(">123</text>"),
        "無効時は値ラベルを描かない"
    );
}

#[test]
fn horizontal_bar_datalabels_render_values() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["a","b"],"datasets":[{"data":[123,87]}]},
      "options":{"indexAxis":"y","plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">123</text>"));
    assert!(svg.contains(">87</text>"));
}

#[test]
fn line_datalabels_render_values() {
    let json = r#"{
      "type":"line",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[123,87,151]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">123</text>"));
    assert!(svg.contains(">87</text>"));
}

#[test]
fn pie_datalabels_render_values() {
    let json = r#"{
      "type":"pie",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[30,50,20]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">30</text>"));
    assert!(svg.contains(">50</text>"));
    assert!(svg.contains(">20</text>"));
}
