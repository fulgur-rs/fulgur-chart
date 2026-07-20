use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

const RADAR: &str = r#"{"type":"radar","data":{"labels":["速度","力","技"],"datasets":[
    {"label":"A","data":[60,80,40]},
    {"label":"B","data":[50,30,90]}]}}"#;

#[test]
fn radar_has_series_polygons() {
    let svg = render(RADAR);
    // 系列多角形は半透明塗り(fill-opacity="0.5")で識別する。グリッドは fill="none"。
    // (chart.js v4 互換: resolve_colors が設定した alpha=0.5 をそのまま使用)
    assert!(
        svg.matches(r#"fill-opacity="0.5""#).count() >= 2,
        "got: {svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn radar_shows_category_labels() {
    let svg = render(RADAR);
    assert!(svg.contains(">速度</text>"));
    assert!(svg.contains(">力</text>"));
    assert!(svg.contains(">技</text>"));
}

#[test]
fn radar_draws_grid() {
    let svg = render(RADAR);
    // 多角形グリッド/スポーク線はテーマのグリッド色 #e0e0e0。
    assert!(svg.contains("#e0e0e0"), "got: {svg}");
}

#[test]
fn radar_has_vertex_markers() {
    let svg = render(RADAR);
    // 系列ごとに n(=3) 頂点マーカー(circle r=3) を持つ。
    assert!(svg.matches(r#"<circle"#).count() >= 6, "got: {svg}");
    assert!(svg.contains(r#"r="3""#));
}

#[test]
fn radar_zero_data_does_not_panic() {
    let svg = render(
        r#"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[0,0,0]}]}}"#,
    );
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn radar_deterministic() {
    assert_eq!(render(RADAR), render(RADAR));
}

#[test]
fn radar_snapshot() {
    let svg = render(
        r#"{"type":"radar","data":{"labels":["速度","力","技"],"datasets":[
            {"label":"A","data":[60,80,40]},
            {"label":"B","data":[50,30,90]}]},
            "options":{"plugins":{"title":{"display":true,"text":"能力"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn radar_max_override_shifts_polygon() {
    // Same data, different scales.r.max → different SVG (polygon points scale differently).
    let default_svg = render(
        r#"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[80,80,80]}]}}"#,
    );
    let bounded_svg = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[80,80,80]}]},"options":{"scales":{"r":{"max":200}}}}"##,
    );
    assert_ne!(
        default_svg, bounded_svg,
        "scales.r.max=200 should shift polygon vs default nice(0..80)"
    );
}

#[test]
fn radar_min_override_does_not_panic() {
    let svg = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[0,50,100]}]},"options":{"scales":{"r":{"min":50}}}}"##,
    );
    assert!(!svg.contains("NaN"));
    assert!(svg.starts_with("<svg"));
}

#[test]
fn radar_snapshot_stable_without_scales() {
    // Cross-check: existing snapshot path is preserved (radial_axis == None).
    let default_svg = render(RADAR);
    let empty_scales_svg = render(
        r#"{"type":"radar","data":{"labels":["速度","力","技"],
        "datasets":[{"label":"A","data":[60,80,40]},{"label":"B","data":[50,30,90]}]}}"#,
    );
    assert_eq!(
        default_svg, empty_scales_svg,
        "identical input should yield identical SVG"
    );
}

#[test]
fn radar_snapshot_fixed_domain() {
    // r.min=0, r.max=100 で 2 系列を固定ドメインで描画。
    // データ最大 (80) が radius の 80% ちょうどになる。
    let svg = render(
        r##"{"type":"radar","data":{"labels":["速度","力","技","知","運"],
        "datasets":[
            {"label":"A","data":[60,80,40,55,20]},
            {"label":"B","data":[50,30,90,45,65]}]},
        "options":{"plugins":{"title":{"display":true,"text":"固定 0-100"}},
                   "scales":{"r":{"min":0,"max":100}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn radar_snapshot_begin_at_zero_with_suggested_range() {
    // suggestedMin/suggestedMax でドメインを広げつつ beginAtZero=true で下端を 0 に固定。
    // radar は負値未対応のため正値のみを使用。
    let svg = render(
        r##"{"type":"radar","data":{"labels":["a","b","c","d"],
        "datasets":[{"label":"delta","data":[20,30,10,5]}]},
        "options":{"scales":{"r":{"suggestedMin":15,"suggestedMax":50,"beginAtZero":true}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn radar_max_override_places_data_max_at_outer_edge() {
    // Codex Fix 5 のリグレッションテスト。
    // max: 95 で data: [95] のとき、頂点は radius 100% (outer edge) に置かれるべき。
    // nice_ticks で nice.max=100 に丸められると 95% の位置に落ちるバグを検出する。
    //
    // 検証: value 95 with max=95 と value 100 with max=100 は data/max 比率が同じ (=1.0)
    // なので頂点位置は同一のはず。path の最初の "M x y" 座標を抽出して一致を確認する。
    let svg_95 = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]},"options":{"scales":{"r":{"min":0,"max":95}}}}"##,
    );
    let svg_100 = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[100,100,100]}]},"options":{"scales":{"r":{"min":0,"max":100}}}}"##,
    );
    let extract_first_m = |s: &str| -> Option<(String, String)> {
        // 系列多角形の path から最初の "M x y" を拾う。
        // グリッド path は fill="none"、系列 path は fill="#XXXXXX" で判別する。
        // path 属性順は d → fill → stroke → stroke-width → fill-opacity (scene renderer 生成)。
        for chunk in s.split(r#"<path d=""#).skip(1) {
            let end = chunk.find('"')?;
            let d = &chunk[..end];
            let attrs = &chunk[end..];
            // fill が "none" 以外 (=系列 path) のみ対象。
            if attrs.contains(r#"fill="none""#) {
                continue;
            }
            let rest = d.strip_prefix("M ")?;
            let mut it = rest.split_whitespace();
            let x = it.next()?.to_string();
            let y = it.next()?.to_string();
            return Some((x, y));
        }
        None
    };
    let a = extract_first_m(&svg_95).expect("series path in max=95 svg");
    let b = extract_first_m(&svg_100).expect("series path in max=100 svg");
    assert_eq!(
        a, b,
        "value 95 with max=95 と value 100 with max=100 は outer edge 100% で同一座標のはず: a={a:?} b={b:?}"
    );
}
