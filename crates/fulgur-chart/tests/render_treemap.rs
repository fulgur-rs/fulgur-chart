//! treemap チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

const NESTED: &str = r#"{
    "type": "treemap",
    "options": { "plugins": { "title": { "display": true, "text": "Sales" } } },
    "data": { "datasets": [{
        "key": "value",
        "groups": ["region", "product"],
        "tree": [
            {"region": "EMEA", "product": "A", "value": 12},
            {"region": "EMEA", "product": "B", "value": 7},
            {"region": "APAC", "product": "A", "value": 9},
            {"region": "APAC", "product": "C", "value": 5},
            {"region": "AMER", "product": "A", "value": 14}
        ]
    }] }
}"#;

#[test]
fn treemap_renders_to_svg() {
    let svg = render(NESTED);
    assert!(svg.starts_with("<svg"), "should produce valid SVG");
    assert!(svg.contains("<rect"), "SVG should contain rect elements");
    assert!(
        svg.contains("<text"),
        "SVG should contain text labels/captions"
    );
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn treemap_numeric_tree_renders() {
    let svg = render(r#"{"type":"treemap","data":{"datasets":[{"tree":[6,4,3,2,1]}]}}"#);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<rect"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn treemap_is_byte_deterministic() {
    assert_eq!(render(NESTED), render(NESTED));
}

#[test]
fn treemap_snapshot() {
    let svg = render(NESTED);
    insta::assert_snapshot!(svg);
}
