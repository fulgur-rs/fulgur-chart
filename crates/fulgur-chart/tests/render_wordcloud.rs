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
