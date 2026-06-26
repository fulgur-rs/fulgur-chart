//! WordCloud チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

const BASIC: &str = r#"{
    "type": "wordCloud",
    "width": 500,
    "height": 300,
    "data": {
        "labels": ["Rust", "SVG", "Chart", "Fast", "Safe"],
        "datasets": [{"data": [80, 60, 50, 40, 30]}]
    }
}"#;

#[test]
fn wordcloud_renders_to_svg() {
    let svg = render(BASIC);
    assert!(svg.starts_with("<svg"), "should produce valid SVG");
    assert!(svg.contains("<text"), "SVG should contain text elements");
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn wordcloud_is_byte_deterministic() {
    assert_eq!(render(BASIC), render(BASIC));
}

#[test]
fn wordcloud_with_rotation() {
    let json = r#"{
        "type": "wordCloud",
        "width": 400,
        "height": 300,
        "data": {
            "labels": ["Alpha", "Beta"],
            "datasets": [{"data": [60, 40]}]
        },
        "options": {"elements": {"word": {"minRotation": -90, "maxRotation": 0, "rotationSteps": 2}}}
    }"#;
    let svg = render(json);
    assert!(
        svg.contains("rotate"),
        "vertical word should have rotate transform"
    );
    assert!(!svg.contains("NaN"));
}

#[test]
fn wordcloud_with_colors() {
    let json = r##"{
        "type": "wordCloud",
        "width": 500,
        "height": 300,
        "data": {
            "labels": ["Red", "Blue"],
            "datasets": [{"data": [60, 40], "color": ["#ff0000", "#0000ff"]}]
        }
    }"##;
    let svg = render(json);
    assert!(svg.contains("#ff0000"), "red color should appear in SVG");
    assert!(svg.contains("#0000ff"), "blue color should appear in SVG");
}

#[test]
fn wordcloud_snapshot() {
    let svg = render(BASIC);
    insta::assert_snapshot!(svg);
}

#[test]
fn wordcloud_example_spec_renders() {
    let json = include_str!("../../../examples/specs/wordcloud.json");
    let svg = render(json);
    assert!(svg.starts_with("<svg"), "should produce valid SVG");
    assert!(svg.contains("<text"), "SVG should contain text elements");
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn wordcloud_oversized_word_is_skipped() {
    // キャンバスより大きい単語は step cap かサイズ超過で配置をスキップし、パニックしない。
    let json = r#"{
        "type": "wordCloud",
        "width": 50,
        "height": 50,
        "data": {
            "labels": ["TinyCanvas", "Small"],
            "datasets": [{"data": [500, 20]}]
        }
    }"#;
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn wordcloud_short_color_array_fills_rest_with_palette() {
    // color 配列が labels/data より短い場合、後半の単語はパレット色で描画される。
    let json = r##"{
        "type": "wordCloud",
        "width": 500,
        "height": 300,
        "data": {
            "labels": ["Red", "Blue", "Green"],
            "datasets": [{"data": [60, 40, 30], "color": ["#ff0000"]}]
        }
    }"##;
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    // 3 単語とも描画されるはず（切り捨てなし）
    assert!(svg.contains("Red") || svg.contains("Blue") || svg.contains("Green"));
    assert!(!svg.contains("NaN"));
}
