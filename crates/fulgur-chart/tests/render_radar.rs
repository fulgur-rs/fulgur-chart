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

/// 系列多角形 (fill != "none") の path から最初の "M x y" 座標を取り出す。
/// グリッド path は fill="none" なので除外される。
fn first_series_vertex(s: &str) -> Option<(String, String)> {
    for chunk in s.split(r#"<path d=""#).skip(1) {
        let end = chunk.find('"')?;
        let d = &chunk[..end];
        let attrs = &chunk[end..];
        if attrs.contains(r#"fill="none""#) {
            continue;
        }
        let m = d.strip_prefix('M')?.trim_start();
        let mut it = m.split_whitespace();
        let x = it.next()?.to_string();
        let y = it.next()?.to_string();
        return Some((x, y));
    }
    None
}

#[test]
fn radar_auto_upper_bound_stays_nice_with_hard_min_only() {
    // Codex Fix 11 のリグレッションテスト。
    // `min` だけを hard 指定した場合、上端は自動計算なので従来通り nice 境界
    // (data 95 → nice.max = 100) を使うべき。raw の data max (95) を上端にすると
    // 頂点が外周まで伸びてしまい、既定 (scales 未指定) の見た目から不必要にズレる。
    let hard_min_only = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]},"options":{"scales":{"r":{"min":0}}}}"##,
    );
    let explicit_nice = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]},"options":{"scales":{"r":{"min":0,"max":100}}}}"##,
    );
    assert_eq!(
        first_series_vertex(&hard_min_only),
        first_series_vertex(&explicit_nice),
        "min のみ指定時、上端は nice.max=100 のままであるべき"
    );

    // 逆に max を hard 指定した場合は raw の 95 が使われ、頂点は外周に届く → 別位置。
    let hard_max = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]},"options":{"scales":{"r":{"min":0,"max":95}}}}"##,
    );
    assert_ne!(
        first_series_vertex(&hard_min_only),
        first_series_vertex(&hard_max),
        "max: 95 (hard) は nice.max=100 とは異なる位置になるべき"
    );
}

#[test]
fn radar_empty_scales_r_is_a_no_op() {
    // Codex Fix 9 のリグレッションテスト。
    // `scales.r: {}` はドメインキーを 1 つも含まないので radial_axis を populate せず、
    // scales 未指定時と完全に同じ SVG になるべき。
    let with_empty = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]},"options":{"scales":{"r":{}}}}"##,
    );
    let without = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[95,95,95]}]}}"##,
    );
    assert_eq!(with_empty, without, "空の scales.r は no-op であるべき");
}

#[test]
fn radar_hard_min_wins_over_suggested_min() {
    // Codex Fix 12 のリグレッションテスト。
    // `min` は hard bound なので、より広い `suggestedMin` があっても負けてはならない。
    let both = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":0,"suggestedMin":-50}}}}"##,
    );
    let hard_only = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":0}}}}"##,
    );
    assert_eq!(
        both, hard_only,
        "hard な min:0 は suggestedMin:-50 に上書きされてはならない"
    );
}

#[test]
fn radar_hard_max_wins_over_suggested_max() {
    // Codex Fix 12 の対称ケース。`max` が hard なら suggestedMax は無視される。
    let both = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"max":50,"suggestedMax":500}}}}"##,
    );
    let hard_only = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"max":50}}}}"##,
    );
    assert_eq!(
        both, hard_only,
        "hard な max:50 は suggestedMax:500 に上書きされてはならない"
    );
}

#[test]
fn radar_hard_max_survives_inverted_domain() {
    // Codex Fix 14 のリグレッションテスト。
    // `beginAtZero: false` + hard な `max` がデータ範囲より下 (data 50, max 40) のとき、
    // 縮退救済が hard な上限 40 を 51 に書き換えてしまい、値が外周ではなく中心へ
    // 落ちていた。hard bound は保持し、自動側 (下端) を動かして解消するべき。
    let clamped = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[50,50,50]}]},"options":{"scales":{"r":{"max":40,"beginAtZero":false}}}}"##,
    );
    // 値 50 は max 40 を超えるので外周 (ratio 1.0) にクランプされるべき。
    // ratio 1.0 になる既知の構成と頂点位置が一致することで検証する。
    let outer_edge = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[100,100,100]}]},"options":{"scales":{"r":{"min":0,"max":100}}}}"##,
    );
    assert_eq!(
        first_series_vertex(&clamped),
        first_series_vertex(&outer_edge),
        "hard な max を超える値は外周にクランプされるべき (中心ではなく)"
    );
    assert!(!clamped.contains("NaN"));
}

#[test]
fn radar_constant_data_visible_without_begin_at_zero() {
    // Codex Fix 15 のリグレッションテスト。
    // `beginAtZero: false` のみ指定 + 全値が同一のとき、自動 lo == hi となり
    // 縮退救済が [v, v+1] を作るため、全頂点が半径 0 (中心) に潰れていた。
    // chart.js と同じく値の周囲へ広げ、データが見えるようにする。
    let svg = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[30,30,30]}]},"options":{"scales":{"r":{"beginAtZero":false}}}}"##,
    );
    let collapsed = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[0,0,0]}]},"options":{"scales":{"r":{"min":0}}}}"##,
    );
    assert_ne!(
        first_series_vertex(&svg),
        first_series_vertex(&collapsed),
        "定数データが中心へ潰れてはならない"
    );
    assert!(!svg.contains("NaN"));
}

#[test]
fn radar_negative_domain_draws_inner_grid_rings() {
    // Codex Fix 16 のリグレッションテスト。
    // ドメインが負を含む (min: -10, max: 10) とき、負および 0 の tick も
    // 正の半径へ写るのでグリッドリングとして描かれるべき。従来は `t <= 0.0` で
    // 一律にスキップされ、内側半分のリングとゼロ境界が丸ごと欠けていた。
    let with_negative = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[5,5,5]}]},"options":{"scales":{"r":{"min":-10,"max":10}}}}"##,
    );
    // グリッドリングは fill="none" の path。
    // nice_ticks(-10, 10) は step 2 で -10..10 の 11 tick を返す。うち rr() が
    // 正の半径へ写すのは -8..10 の 10 本。従来は `t <= 0.0` スキップにより
    // 2..10 の 5 本しか描かれなかったので、この閾値が回帰を検出する。
    let rings = with_negative.matches(r#"fill="none""#).count();
    assert!(
        rings >= 8,
        "負ドメインでは内側リングとゼロ境界も描かれるべき (旧実装は 5 本): rings={rings}"
    );
    assert!(!with_negative.contains("NaN"));
}

#[test]
fn radar_conflicting_hard_bounds_render_deterministically() {
    // 両側 hard で min > max (指定ミス) の場合、動かせる自動側が無い。
    // 決定的に hard な min を優先して上端を開き、NaN を出さず有効な SVG を返すこと。
    let a = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":100,"max":50}}}}"##,
    );
    let b = render(
        r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":100,"max":50}}}}"##,
    );
    assert_eq!(a, b, "矛盾指定でも決定的であるべき");
    assert!(!a.contains("NaN") && !a.contains("inf"));
    assert!(a.starts_with("<svg") && a.trim_end().ends_with("</svg>"));
}
