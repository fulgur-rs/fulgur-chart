use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png;
use fulgur_chart::render::render_chart;
use tiny_skia::Pixmap;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

fn render_png(json: &str) -> Vec<u8> {
    render_chart_to_png(&chartjs::parse(json, false).unwrap(), 1.0, DEFAULT_FONT).unwrap()
}

fn png_diff_pixels(first: &[u8], second: &[u8]) -> usize {
    let first = Pixmap::decode_png(first).expect("first PNG should decode");
    let second = Pixmap::decode_png(second).expect("second PNG should decode");
    assert_eq!(
        (first.width(), first.height()),
        (second.width(), second.height())
    );
    first
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.data().as_chunks::<4>().0.iter())
        .filter(|(left, right)| left != right)
        .count()
}

fn donut_radii(path: &str) -> (f64, f64) {
    let tokens: Vec<&str> = path.split_whitespace().collect();
    let outer = tokens[4].parse().expect("outer radius");
    let inner = tokens[15].parse().expect("inner radius");
    (outer, inner)
}

fn path_center(path: &str) -> (f64, f64) {
    let tokens: Vec<&str> = path.split_whitespace().collect();
    (
        tokens[1].parse().expect("center x"),
        tokens[2].parse().expect("center y"),
    )
}

fn pie_outer_radius(path: &str) -> f64 {
    let tokens: Vec<&str> = path.split_whitespace().collect();
    let radius_index = if tokens[3] == "A" { 4 } else { 7 };
    tokens[radius_index].parse().expect("pie outer radius")
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

#[test]
fn pie_numeric_cutout_uses_pixels() {
    let json = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]},"options":{"cutout":24}}"#;
    let svg = render(json);
    let (outer, inner) = donut_radii(&nth_path_d(&svg, 0));
    assert!((inner - 24.0).abs() < 0.01, "expected 24px, got {inner}");
    assert!(inner < outer);
    let plain = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#;
    assert!(png_diff_pixels(&render_png(plain), &render_png(json)) > 0);
}

#[test]
fn doughnut_percentage_cutout_scales_with_radius() {
    let json = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]},"options":{"cutout":"25%"}}"#;
    let svg = render(json);
    let (outer, inner) = donut_radii(&nth_path_d(&svg, 0));
    assert!(
        (inner / outer - 0.25).abs() < 0.001,
        "outer={outer}, inner={inner}"
    );
    let default = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#;
    assert!(png_diff_pixels(&render_png(default), &render_png(json)) > 0);
}

#[test]
fn pie_dataset_options_draw_concentric_rings() {
    let json = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]},{"data":[2,1]}]},"options":{"cutout":"40%"}}"#;
    let svg = render(json);
    assert_eq!(
        svg.matches("<path").count(),
        4,
        "each dataset should draw two arcs"
    );
    let (outer_ring_radius, _) = donut_radii(&nth_path_d(&svg, 0));
    let (inner_ring_radius, _) = donut_radii(&nth_path_d(&svg, 2));
    assert!(outer_ring_radius > inner_ring_radius);
    let one_dataset = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]},"options":{"cutout":"40%"}}"#;
    assert!(png_diff_pixels(&render_png(one_dataset), &render_png(json)) > 0);
}

#[test]
fn pie_spacing_separates_adjacent_arcs() {
    let spaced =
        r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"spacing":8} ]}}"#;
    let adjacent = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#;
    let svg = render(spaced);
    let first_end = nth_path_d(&svg, 0);
    let second_start = nth_path_d(&svg, 1);
    let first_tokens: Vec<&str> = first_end.split_whitespace().collect();
    let second_tokens: Vec<&str> = second_start.split_whitespace().collect();
    let end_x: f64 = first_tokens[12].parse().unwrap();
    let end_y: f64 = first_tokens[13].parse().unwrap();
    let start_x: f64 = second_tokens[1].parse().unwrap();
    let start_y: f64 = second_tokens[2].parse().unwrap();
    let gap = (end_x - start_x).hypot(end_y - start_y);
    assert!(
        (8.0..24.0).contains(&gap),
        "spacing 8 should make a bounded gap, got {gap}"
    );
    let adjacent_radius = pie_outer_radius(&nth_path_d(&render(adjacent), 0));
    let spaced_radius = pie_outer_radius(&nth_path_d(&svg, 0));
    assert!(
        (adjacent_radius - spaced_radius).abs() < 0.05,
        "Chart.js spacing reserves half its value, then adds that half back to the arc: {adjacent_radius} vs {spaced_radius}"
    );
    assert!(png_diff_pixels(&render_png(adjacent), &render_png(spaced)) > 0);
}

#[test]
fn pie_offset_moves_only_selected_arcs() {
    let json =
        r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"offset":[0,12]}]}}"#;
    let svg = render(json);
    let first = path_center(&nth_path_d(&svg, 0));
    let second = path_center(&nth_path_d(&svg, 1));
    let displacement = (first.0 - second.0).hypot(first.1 - second.1);
    assert!(
        (displacement - 3.0).abs() < 0.05,
        "Chart.js translates an offset arc by offset / 4, got {displacement}px"
    );
    let first_radius = pie_outer_radius(&nth_path_d(&svg, 0));
    let second_radius = pie_outer_radius(&nth_path_d(&svg, 1));
    assert!(
        ((second_radius - first_radius) - 3.0).abs() < 0.05,
        "Chart.js applies one quarter of offset 12 as radial correction for a half-circle: {first_radius} vs {second_radius}"
    );
    let default = r#"{"type":"pie","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#;
    let default_radius = pie_outer_radius(&nth_path_d(&render(default), 0));
    assert!(
        ((default_radius - first_radius) - 6.0).abs() < 0.05,
        "Chart.js reserves half the maximum offset from the chart radius: {default_radius} vs {first_radius}"
    );
    assert!(png_diff_pixels(&render_png(default), &render_png(json)) > 0);
}

#[test]
fn pie_unused_offsets_do_not_shrink_chart_radius() {
    let oversized =
        r#"{"type":"pie","data":{"labels":["A"],"datasets":[{"data":[1],"offset":[0,1000]}]}}"#;
    let default = r#"{"type":"pie","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#;
    let oversized_radius = pie_outer_radius(&nth_path_d(&render(oversized), 0));
    let default_radius = pie_outer_radius(&nth_path_d(&render(default), 0));
    assert!(
        (oversized_radius - default_radius).abs() < 0.05,
        "an offset value beyond the dataset length is unused: {default_radius} vs {oversized_radius}"
    );
}

#[test]
fn pie_border_radius_rounds_named_arc_corners() {
    let rounded = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1],"borderRadius":{"outerStart":12,"outerEnd":10,"innerStart":8,"innerEnd":6}}]}}"#;
    let plain = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[1,1]}]}}"#;
    let rounded_svg = render(rounded);
    let plain_svg = render(plain);
    assert!(
        nth_path_d(&rounded_svg, 0).matches(" A ").count()
            > nth_path_d(&plain_svg, 0).matches(" A ").count()
    );
    assert!(png_diff_pixels(&render_png(plain), &render_png(rounded)) > 0);
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
    assert!(
        l0y < cy,
        "start should be above center (top): l0y={l0y} cy={cy}"
    );
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
