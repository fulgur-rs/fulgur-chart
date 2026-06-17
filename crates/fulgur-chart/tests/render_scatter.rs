use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn scatter_draws_one_circle_per_point() {
    let svg = render(
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4},{"x":5,"y":1}]}]}}"#,
    );
    // フレームは円を一切描かない(凡例 swatch は Rect)。円数 == 点数。
    assert_eq!(svg.matches("<circle").count(), 3);
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn scatter_has_numeric_ticks_on_both_axes() {
    // x ドメインを 0..10、y を 0..20 にして、両軸の目盛りラベルが出ることを確認。
    let svg = render(
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":0,"y":0},{"x":10,"y":20}]}]}}"#,
    );
    // y 軸の代表目盛り(右寄せ End)と x 軸の代表目盛り(Middle)が出る。
    // text-anchor="end" は y 軸ラベル、"middle" は x 軸ラベル(+タイトル無し)。
    assert!(svg.contains("text-anchor=\"end\""));
    assert!(svg.contains("text-anchor=\"middle\""));
    // 数値ラベルそのもの。両軸の上限が描かれる。
    assert!(svg.contains(">10<"));
    assert!(svg.contains(">20<"));
}

#[test]
fn scatter_deterministic() {
    let j = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4}]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn scatter_snapshot() {
    let svg = render(
        r#"{"type":"scatter","data":{"datasets":[{"label":"観測","data":[{"x":1,"y":2},{"x":3,"y":5},{"x":4,"y":3}]}]},"options":{"plugins":{"title":{"display":true,"text":"散布図"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
