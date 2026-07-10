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

/// `examples/specs/sankey.json`(README/CLI スモークで使う「Energy flow」サンプル)が
/// ライブラリ経路でも問題なく SVG を生成することを検証する。
#[test]
fn sankey_example_spec_renders() {
    let json = include_str!("../../../examples/specs/sankey.json");
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
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
            color_from: None,
            color_to: None,
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

#[test]
fn sankey_accepts_hover_color_and_renders_identically() {
    // hoverColorFrom / hoverColorTo は静的レンダラでは描画されないため、
    // 指定した spec と指定しない spec の SVG が byte-identical になる。
    let with_hover = r##"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "hoverColorFrom":"#000000","hoverColorTo":"#ffffff",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"##;
    let without_hover = r##"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"##;
    assert_eq!(render(with_hover), render(without_hover));
}

#[test]
fn sankey_rejects_invalid_hover_color() {
    let bad = r##"{"type":"sankey","data":{"datasets":[{
        "hoverColorFrom":"not-a-color",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"##;
    let err = chartjs::parse(bad, false).unwrap_err();
    assert!(
        err.contains("hoverColorFrom"),
        "error must mention field: {err}"
    );
}

#[test]
fn sankey_hover_color_accepted_by_strict_parser() {
    // The strict allowlist for `check_unknown_keys_sankey` must include
    // `hoverColorFrom` / `hoverColorTo`, otherwise strict mode would reject
    // chartjs-compatible JSON that carries them.
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "hoverColorFrom":"#000000","hoverColorTo":"#ffffff",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"##;
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict parser must accept hoverColorFrom/hoverColorTo"
    );
}

#[test]
fn sankey_per_link_color_short_form_overrides_both_stops() {
    // per-link `color` は from/to 両 stop の shorthand (ribbon 塗りの上書き)。
    // ノード矩形は dataset 色のまま — chartjs-chart-sankey 挙動 (design fulgur-chart-40h).
    let with_override = r##"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "data":[{"from":"A","to":"B","flow":1,"color":"#00ff00"}]
    }]}}"##;
    let svg = render(with_override);
    let start = svg.find("<linearGradient").expect("gradient present");
    let end = svg[start..]
        .find("</linearGradient>")
        .expect("gradient closed")
        + start;
    let grad = &svg[start..end];
    assert!(
        grad.contains("00ff00"),
        "per-link color must fill gradient stops: {grad}"
    );
    assert!(
        !grad.contains("36a2eb") && !grad.contains("ff6384"),
        "dataset ribbon colors must not appear in gradient stops: {grad}"
    );
}

#[test]
fn sankey_per_link_color_from_overrides_only_from_stop() {
    // per-link `colorFrom` は from stop のみ上書き。to stop は dataset colorTo のまま。
    // ノード矩形は dataset 色のまま — gradient 内だけを検査する。
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#111111","colorTo":"#222222",
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"#abcdef"}]
    }]}}"##;
    let svg = render(json);
    let start = svg.find("<linearGradient").expect("gradient present");
    let end = svg[start..]
        .find("</linearGradient>")
        .expect("gradient closed")
        + start;
    let grad = &svg[start..end];
    assert!(grad.contains("abcdef"), "from override in gradient: {grad}");
    assert!(
        grad.contains("222222"),
        "to keeps dataset value in gradient: {grad}"
    );
    assert!(
        !grad.contains("111111"),
        "from dataset value replaced in gradient: {grad}"
    );
}

#[test]
fn sankey_per_link_color_from_wins_over_color_shorthand() {
    // color と colorFrom を併用: colorFrom が勝つ (from 側)。to 側は color の値。
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"color":"#aa0000","colorFrom":"#00aa00"}]
    }]}}"##;
    let svg = render(json);
    assert!(svg.contains("00aa00"), "from uses explicit colorFrom");
    assert!(svg.contains("aa0000"), "to uses shorthand color");
}

#[test]
fn sankey_per_link_color_deterministic() {
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"color":"#123456"}]
    }]}}"##;
    assert_eq!(render(json), render(json));
}

#[test]
fn sankey_per_link_color_from_invalid_rejected() {
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"not-a-color"}]
    }]}}"##;
    let err = chartjs::parse(json, false).unwrap_err();
    assert!(
        err.contains("colorFrom"),
        "error must mention colorFrom: {err}"
    );
}

#[test]
fn sankey_per_link_color_works_with_from_mode() {
    // colorMode=from + per-link color: リンクごとの effective_from が単色塗りに使われる。
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "colorMode":"from",
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"#abcdef"}]
    }]}}"##;
    let svg = render(json);
    assert!(!svg.contains("<linearGradient"), "from mode -> solid");
    assert!(svg.contains("abcdef"), "per-link colorFrom fills path");
}
