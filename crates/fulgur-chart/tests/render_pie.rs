use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn pie_has_one_path_per_slice() {
    let svg = render(
        r#"{"type":"pie","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
    );
    assert!(svg.matches("<path").count() >= 3);
    assert!(svg.contains(" A ")); // 円弧コマンド
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn pie_uses_per_slice_colors() {
    let svg = render(
        r##"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"backgroundColor":["#ff0000","#0000ff"]}]}}"##,
    );
    assert!(svg.contains("#ff0000") && svg.contains("#0000ff"));
}

#[test]
fn doughnut_has_inner_arc() {
    // doughnut は内弧を含む（A が2回/パス、L で内外接続）
    let svg =
        render(r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#);
    assert!(svg.matches(" A ").count() >= 4); // 2スライス×2弧
}

#[test]
fn single_value_full_circle_does_not_panic() {
    let svg = render(r#"{"type":"pie","data":{"labels":["only"],"datasets":[{"data":[5]}]}}"#);
    assert!(svg.matches("<path").count() >= 2); // 全周は2分割
    assert!(!svg.contains("NaN"));
}

#[test]
fn zero_total_does_not_panic() {
    let svg = render(r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[0,0]}]}}"#);
    assert!(svg.starts_with("<svg")); // スライス無しでも有効SVG
    assert!(!svg.contains("NaN"));
}

#[test]
fn pie_legend_shows_categories() {
    let svg = render(
        r#"{"type":"pie","data":{"labels":["Apple","Banana"],"datasets":[{"data":[1,2]}]}}"#,
    );
    assert!(svg.contains(">Apple</text>") && svg.contains(">Banana</text>"));
}

#[test]
fn pie_deterministic() {
    let j = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
    assert_eq!(render(j), render(j));
}

/// SVG 中の n 番目(0始まり)の `<path d="...">` の d 属性を取り出す。
fn nth_path_d(svg: &str, n: usize) -> String {
    let mut rest = svg;
    for _ in 0..n {
        let p = rest.find("<path").expect("enough <path>");
        rest = &rest[p + 5..];
    }
    let p = rest.find("<path").expect("a <path>");
    let dstart = rest[p..].find("d=\"").expect("d attr") + p + 3;
    let dend = rest[dstart..].find('"').expect("close quote") + dstart;
    rest[dstart..dend].to_string()
}

// 開始角(12時)と回転方向(時計回り)が chart.js v4 デフォルトと一致することを固定する。
// 実測根拠: chart.js 4.5.1 を @napi-rs/canvas で描画し、データ [3,1] の pie で
//   arc[0] startAngle = -π/2 (真上), endAngle = π  (時計回りに角度増加)
//   arc[1] startAngle =  π,    endAngle = 1.5π
// を確認済み。pie レイアウトは a0 = -π/2 から +frac·2π で進むため同一の扇形配置になる。
#[test]
fn pie_starts_at_top_and_advances_clockwise() {
    // [3,1] → 先頭スライスは 270°(3/4 周)。対称データだと方向が判定不能なので非対称にする。
    let svg = render(r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[3,1]}]}}"#);

    // 先頭 <path> は先頭スライス(凡例は <rect>/<text> なので path にならない)。
    // pie パス: "M cx cy L o0x o0y A rx ry rot laf sweep o1x o1y Z"
    let d0 = nth_path_d(&svg, 0);
    let t: Vec<&str> = d0.split_whitespace().collect();
    assert_eq!(t[0], "M", "d0={d0}");
    let cx: f64 = t[1].parse().unwrap();
    let cy: f64 = t[2].parse().unwrap();
    assert_eq!(t[3], "L");
    let l0x: f64 = t[4].parse().unwrap();
    let l0y: f64 = t[5].parse().unwrap();
    // 開始点は中心の真上(12時方向): x は中心と一致、y は中心より上(SVG は y 下向き)。
    assert!(
        (l0x - cx).abs() < 0.05,
        "start should be directly above center (x): l0x={l0x} cx={cx}"
    );
    assert!(l0y < cy, "start should be above center (top): l0y={l0y} cy={cy}");
    // 円弧コマンドと sweep flag(=1: SVG y下向き座標で時計回り)、large-arc(270°なので1)。
    assert_eq!(t[6], "A");
    assert_eq!(t[10], "1", "large-arc-flag must be 1 for the 270° slice");
    assert_eq!(t[11], "1", "sweep flag must be 1 (clockwise)");

    // 2番目のスライスは先頭スライスの終点から続く(時計回りに前進している)。
    let o1 = (t[12].to_string(), t[13].to_string());
    let d1 = nth_path_d(&svg, 1);
    let t1: Vec<&str> = d1.split_whitespace().collect();
    assert_eq!(
        (t1[4], t1[5]),
        (o1.0.as_str(), o1.1.as_str()),
        "slice 2 must start where slice 1 ended (clockwise progression)"
    );
}

#[test]
fn pie_snapshot() {
    let svg = render(
        r#"{"type":"doughnut","data":{"labels":["A","B","C"],"datasets":[{"data":[30,50,20]}]},"options":{"plugins":{"title":{"display":true,"text":"内訳"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
