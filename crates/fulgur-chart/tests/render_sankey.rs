//! sankey チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::guard::{InputLimits, MAX_SANKEY_NODES, validate_spec};
use fulgur_chart::ir::SankeyLink;
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

/// スタック安全テスト: ノード数上限ちょうどの線形連鎖を、guard が受理し、
/// レイアウトの再帰(process_to / get_all_keys_forward が連鎖長ぶん深くなる)が
/// スタックオーバーフローせず SVG を生成できることを検証する。
///
/// 既定のテストスレッドのスタックは RUST_MIN_STACK 等で増減し得るため、環境に
/// 依存しないよう明示的に 2 MB スタックのスレッドで実行する(本番メインスレッドの
/// 約 8 MB より厳しい、最悪ケースに近い条件)。MAX_SANKEY_NODES はこの 2 MB 条件で
/// オーバーフローする連鎖長(経験的に約 6,100 ノード)に対し約 3 倍のマージンを持つ。
#[test]
fn sankey_at_cap_linear_chain_renders_without_stack_overflow() {
    // N == MAX_SANKEY_NODES ノードの線形連鎖 n0→n1→…→n(N-1)(リンク N-1 本)。
    let n = MAX_SANKEY_NODES;
    let mut spec = chartjs::parse(
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"n0","to":"n1","flow":1}]}]}}"#,
        false,
    )
    .expect("parse error");
    let links: Vec<SankeyLink> = (0..n - 1)
        .map(|i| SankeyLink {
            from: format!("n{i}"),
            to: format!("n{}", i + 1),
            flow: 1.0,
        })
        .collect();
    spec.series[0].links = links;

    // ノード数ちょうど上限 → guard は受理する。
    assert!(
        validate_spec(&spec, &InputLimits::default()).is_ok(),
        "at-cap linear chain ({n} nodes) must pass guard"
    );

    // 2 MB スタックの専用スレッドで描画してオーバーフローしないことを確認する。
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || render_chart(&spec))
        .expect("spawn render thread");
    let svg = handle.join().expect("render thread must not overflow");
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}
