use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn polar_area_basic_renders() {
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
    );
    assert!(svg.matches("<path").count() >= 3);
    assert!(svg.contains(" A "));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn polar_area_equal_angles() {
    // [10,10,10] → 3 slices each 120°, large-arc-flag must be 0
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,10,10]}]}}"#,
    );
    assert!(!svg.contains("NaN"));
    // Each 120° slice: large-arc-flag = 0 (< 180°)
    // Count "0 1" sweep patterns (laf=0, sweep=1) for clockwise arcs
    let clockwise_small_arcs = svg.matches("0 1 ").count();
    assert!(
        clockwise_small_arcs >= 3,
        "Expected at least 3 small-arc clockwise slices, got {clockwise_small_arcs}"
    );
}

#[test]
fn polar_area_radius_proportional_to_value() {
    // [100, 50] → second slice radius ~half of first
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[100,50]}]}}"#,
    );
    assert!(!svg.contains("NaN"));
    // SVG arc command: "A rx ry 0 laf sweep x y" — extract rx values to verify radius ratio.
    let radii: Vec<f64> = svg
        .split('A')
        .skip(1)
        .filter_map(|seg| seg.split_whitespace().next()?.parse::<f64>().ok())
        .collect();
    assert!(radii.len() >= 2, "arc半径を2つ以上抽出できませんでした");
    let ratio = radii[1] / radii[0];
    assert!(
        (ratio - 0.5).abs() < 0.1,
        "期待比率 0.5 に対して実測は {ratio}"
    );
}

#[test]
fn polar_area_zero_values_dont_panic() {
    let svg =
        render(r#"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[0,0]}]}}"#);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn polar_area_single_value_full_circle() {
    // single slice = 360° → split into 2
    let svg =
        render(r#"{"type":"polarArea","data":{"labels":["only"],"datasets":[{"data":[5]}]}}"#);
    assert!(svg.matches("<path").count() >= 2);
    assert!(!svg.contains("NaN"));
}

#[test]
fn polar_area_uses_per_slice_colors() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"backgroundColor":["#ff0000","#0000ff"]}]}}"##,
    );
    assert!(svg.contains("#ff0000") && svg.contains("#0000ff"));
}

#[test]
fn polar_area_legend_shows_categories() {
    let svg = render(
        r#"{"type":"polarArea","data":{"labels":["Apple","Banana"],"datasets":[{"data":[1,2]}]}}"#,
    );
    assert!(svg.contains(">Apple</text>") && svg.contains(">Banana</text>"));
}

#[test]
fn polar_area_deterministic() {
    let j =
        r#"{"type":"polarArea","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn polar_area_snapshot() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],"datasets":[{"data":[30,80,50,20],"backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},"options":{"plugins":{"title":{"display":true,"text":"季節別データ"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn polar_area_max_override_does_not_panic() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],
        "datasets":[{"data":[100,50]}]},"options":{"scales":{"r":{"max":200}}}}"##,
    );
    assert!(!svg.contains("NaN"));
    assert!(svg.starts_with("<svg"));
    // Two data points ⇒ at least 2 arc paths.
    let radii: Vec<f64> = svg
        .split('A')
        .skip(1)
        .filter_map(|seg| seg.split_whitespace().next()?.parse::<f64>().ok())
        .collect();
    assert!(radii.len() >= 2, "expected ≥2 arcs, got {}", radii.len());
    let ratio = radii[1] / radii[0];
    assert!((ratio - 0.5).abs() < 0.1, "ratio={ratio}");
}

#[test]
fn polar_area_min_override_clamps_below() {
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,50,100]}]},"options":{"scales":{"r":{"min":50,"max":100}}}}"##,
    );
    assert!(!svg.contains("NaN"));
    assert!(svg.starts_with("<svg"));
}

#[test]
fn polar_area_snapshot_suggested_max_expands_domain() {
    // suggestedMax=200 でデータ最大 (80) より広いドメインに拡張。
    // 各 slice 半径は v/200 (default v/80 に対して 40% 縮小)。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],
        "datasets":[{"data":[30,80,50,20],
                     "backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},
        "options":{"plugins":{"title":{"display":true,"text":"suggestedMax=200"}},
                   "scales":{"r":{"suggestedMax":200}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn polar_area_snapshot_stable_without_scales() {
    // Second render of the exact snapshot fixture; must equal the first render.
    let a = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],"datasets":[{"data":[30,80,50,20],"backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},"options":{"plugins":{"title":{"display":true,"text":"季節別データ"}}}}"##,
    );
    let b = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],"datasets":[{"data":[30,80,50,20],"backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},"options":{"plugins":{"title":{"display":true,"text":"季節別データ"}}}}"##,
    );
    assert_eq!(a, b);
}

#[test]
fn polar_area_min_equal_to_data_max_does_not_produce_nan() {
    // 縮退ケース: min == data の最大 → hi = lo + 1.0 で救済され、有効な SVG が返る。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[100,100,100]}]},"options":{"scales":{"r":{"min":100}}}}"##,
    );
    assert!(
        !svg.contains("NaN"),
        "degenerate min == data_max should not produce NaN"
    );
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn polar_area_all_negative_data_still_renders_slices() {
    // Codex Fix 13 のリグレッションテスト。
    // 全データが負で beginAtZero が既定 (true) のとき、下端だけ 0 方向へ寄せると
    // ドメインが [0, 0] → 縮退救済で [0, 1] に潰れ、全スライスが消えていた。
    // beginAtZero は「ドメインに 0 を含める」意味なので上端にも効く必要がある
    // (→ [-50, 0])。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],
        "datasets":[{"data":[-50,-10]}]},"options":{"scales":{"r":{"suggestedMin":-60}}}}"##,
    );
    assert!(
        svg.matches("<path").count() >= 2,
        "全負データでもスライスが描画されるべき: {svg}"
    );
    assert!(svg.contains(" A "), "円弧コマンドが必要: {svg}");
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn polar_area_begin_at_zero_includes_zero_with_hard_negative_min() {
    // Codex Fix 13: hard な負の `min` が指定されていても、`max` は自動計算側なので
    // beginAtZero によって上端が 0 まで引き上げられ、ドメインが 0 を含むべき。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],
        "datasets":[{"data":[-50,-10]}]},"options":{"scales":{"r":{"min":-100}}}}"##,
    );
    // ドメイン [-100, 0] なら値 -50 は半径の中央 (ratio 0.5) にマップされる。
    // [-100, -10] (beginAtZero 無効時) との差を見るため、明示 max との一致を確認する。
    let explicit = render(
        r##"{"type":"polarArea","data":{"labels":["A","B"],
        "datasets":[{"data":[-50,-10]}]},"options":{"scales":{"r":{"min":-100,"max":0}}}}"##,
    );
    assert_eq!(
        svg, explicit,
        "beginAtZero(既定 true) は自動側の上端を 0 まで引き上げるべき"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn polar_area_empty_scales_r_is_a_no_op() {
    // Codex Fix 9: `scales.r: {}` は radial_axis を populate せず、
    // scales 未指定時と完全に同じ SVG になるべき。
    let with_empty = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{}}}}"##,
    );
    let without = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]}}"##,
    );
    assert_eq!(with_empty, without, "空の scales.r は no-op であるべき");
}

#[test]
fn polar_area_hard_min_wins_over_suggested_min() {
    // Codex Fix 12: hard な `min` は、より広い `suggestedMin` に負けてはならない。
    let both = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":0,"suggestedMin":-50}}}}"##,
    );
    let hard_only = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":0}}}}"##,
    );
    assert_eq!(both, hard_only);
}

#[test]
fn polar_area_constant_data_visible_without_begin_at_zero() {
    // Codex Fix 15 のリグレッションテスト (polar 側)。
    // `beginAtZero: false` のみ指定 + 全値が同一だと自動 lo == hi となり、
    // 縮退救済 [v, v+1] で全スライスの半径が 0 になり `r > 0` チェックで
    // 一枚も描かれなくなっていた。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[30,30,30]}]},"options":{"scales":{"r":{"beginAtZero":false}}}}"##,
    );
    assert!(
        svg.matches("<path").count() >= 3,
        "定数データでもスライスが描画されるべき: {svg}"
    );
    assert!(svg.contains(" A "), "円弧コマンドが必要");
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn polar_area_hard_max_survives_inverted_domain() {
    // Codex Fix 14 のリグレッションテスト (polar 側)。
    // hard な `max` がデータ範囲より下でも、上限を書き換えずに自動側 (下端) を
    // 動かして解消するべき。値は外周にクランプされる。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[50,50,50]}]},"options":{"scales":{"r":{"max":40,"beginAtZero":false}}}}"##,
    );
    assert!(
        svg.matches("<path").count() >= 3,
        "max を超える値も外周スライスとして描かれるべき: {svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn polar_area_conflicting_hard_bounds_render_deterministically() {
    // 両側 hard で min > max (指定ミス) の場合も NaN を出さず決定的に描画すること。
    let a = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":100,"max":50}}}}"##,
    );
    let b = render(
        r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,20,30]}]},"options":{"scales":{"r":{"min":100,"max":50}}}}"##,
    );
    assert_eq!(a, b, "矛盾指定でも決定的であるべき");
    assert!(!a.contains("NaN") && !a.contains("inf"));
    assert!(a.starts_with("<svg") && a.trim_end().ends_with("</svg>"));
}
