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
fn sankey_color_mode_to_uses_solid_fill() {
    // colorMode='to' は単色塗り(グラデーション無し)。
    let svg = render(
        r#"{"type":"sankey","data":{"datasets":[{"colorMode":"to","data":[{"from":"A","to":"B","flow":1}]}]}}"#,
    );
    assert!(
        !svg.contains("<linearGradient"),
        "to mode emits no gradient"
    );
    assert!(svg.contains("<path"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_min_size_single_input_overlap_no_nan() {
    // size='min' で B は in=10>out=5 → size=5<in、from は単一エッジ。
    // upstream の (idx*(size-flow))/(len-1) が len==1 で 0/0=NaN を作るが、
    // fmt_num が "0" に潰すため SVG に literal "NaN" は出ない(fmt_num の安全網を固定)。
    let svg = render(
        r#"{"type":"sankey","data":{"datasets":[{"size":"min","data":[
            {"from":"A","to":"B","flow":10},
            {"from":"B","to":"C","flow":5}
        ]}]}}"#,
    );
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"), "fmt_num must collapse NaN add_y to 0");
}

#[test]
fn sankey_self_loop_renders_without_panic() {
    // 自己ループ A→A は退化ケースだがパニックせず SVG を出す。
    let svg = render(
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"A","flow":1}]}]}}"#,
    );
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_empty_links_renders_without_panic() {
    // リンク 0 件でもパニックせず空の SVG を出す。
    let svg = render(r#"{"type":"sankey","data":{"datasets":[{"data":[]}]}}"#);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_snapshot() {
    insta::assert_snapshot!(render(ENERGY));
}
