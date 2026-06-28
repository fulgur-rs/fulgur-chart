//! sankey チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

const ENERGY: &str = r##"{"type":"sankey","data":{"datasets":[{"data":[
  {"from":"Coal","to":"Electricity","flow":25},
  {"from":"Gas","to":"Electricity","flow":15},
  {"from":"Electricity","to":"Residential","flow":20},
  {"from":"Electricity","to":"Industrial","flow":20}
],"colorFrom":"#36a2eb","colorTo":"#ff6384"}]}}"##;

#[test]
fn sankey_renders_svg() {
    let svg = render(ENERGY);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<rect"), "nodes should be rects");
    assert!(svg.contains("<path"), "ribbons should be paths");
    assert!(svg.contains("<text"), "labels should be text");
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn sankey_is_byte_deterministic() {
    assert_eq!(render(ENERGY), render(ENERGY));
}

#[test]
fn sankey_gradient_default_emits_defs() {
    let svg = render(
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#,
    );
    assert!(
        svg.contains("<linearGradient"),
        "gradient mode default should emit defs"
    );
}

#[test]
fn sankey_color_mode_from_uses_solid_fill() {
    // colorMode='from' は単色塗り(グラデーション無し)。
    let svg = render(
        r#"{"type":"sankey","data":{"datasets":[{"colorMode":"from","data":[{"from":"A","to":"B","flow":1}]}]}}"#,
    );
    assert!(
        !svg.contains("<linearGradient"),
        "from mode emits no gradient"
    );
    assert!(svg.contains("<path"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_snapshot() {
    insta::assert_snapshot!(render(ENERGY));
}
