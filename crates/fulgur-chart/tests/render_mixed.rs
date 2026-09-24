use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::build_scene;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;

/// Parses and renders one Chart.js-compatible JSON spec as SVG.
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

/// 棒(bar) + 折れ線(line) の混合。3 カテゴリ、片方の dataset に type:"line"。
const MIXED_JSON: &str = r#"{
  "type": "bar",
  "data": {
    "labels": ["1月", "2月", "3月"],
    "datasets": [
      { "label": "売上", "data": [120, 200, 150] },
      { "type": "line", "label": "目標", "data": [140, 180, 170] }
    ]
  },
  "options": { "plugins": { "title": { "display": true, "text": "売上と目標" } } }
}"#;

#[test]
fn mixed_has_bars_line_and_markers() {
    let svg = render(MIXED_JSON);
    // 棒は <rect>。3 カテゴリの棒(+凡例 swatch 2 つ)があるので少なくとも 3 本。
    assert!(svg.matches("<rect").count() >= 3, "棒(rect)が足りない");
    // 折れ線は <polyline>(tension=0)または曲線 <path>。
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "折れ線(polyline/path)が無い"
    );
    // 折れ線のマーカー(circle)を各点に。
    assert!(
        svg.matches("<circle").count() >= 3,
        "マーカー(circle)が足りない"
    );
    // カテゴリラベル。
    assert!(
        svg.contains("1月") && svg.contains("3月"),
        "カテゴリラベルが無い"
    );
    // 凡例(系列名)。
    assert!(svg.contains("売上") && svg.contains("目標"), "凡例が無い");
    // 健全性。
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn mixed_line_cubic_interpolation_mode_uses_monotone_geometry() {
    let monotone_spec = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[
          {"type":"line","data":[0,2,7],"tension":0.8,"cubicInterpolationMode":"monotone","borderColor":"#0000ff"},
          {"data":[1,2,3]}
        ]}}"##,
        false,
    )
    .unwrap();
    let catmull_spec = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[
          {"type":"line","data":[0,2,7],"tension":0.8,"borderColor":"#0000ff"},
          {"data":[1,2,3]}
        ]}}"##,
        false,
    )
    .unwrap();
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let stroke_path = |scene: &fulgur_chart::scene::Scene| {
        scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(stroke),
                    ..
                } if (stroke.r, stroke.g, stroke.b) == (0, 0, 255) => Some(d.clone()),
                _ => None,
            })
            .expect("mixed line cubic interpolation must create a stroke path")
    };
    let monotone_path = stroke_path(&build_scene(&monotone_spec, &measurer));
    let catmull_path = stroke_path(&build_scene(&catmull_spec, &measurer));

    assert!(monotone_path.contains(" C "));
    assert_ne!(monotone_path, catmull_path);
}

#[test]
fn mixed_deterministic() {
    assert_eq!(render(MIXED_JSON), render(MIXED_JSON));
}

#[test]
fn mixed_snapshot() {
    let svg = render(MIXED_JSON);
    insta::assert_snapshot!(svg);
}

/// Higher order datasets paint behind lower order datasets across bar and line layers.
#[test]
fn mixed_dataset_order_controls_front_to_back_painting() {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let high_order_line_json = r##"{"type":"bar","data":{"labels":["x"],"datasets":[
      {"label":"bar","order":1,"data":[1],"backgroundColor":"#ff0000"},
      {"label":"line","type":"line","order":2.5,"data":[2],"borderColor":"#0000ff"}
    ]}}"##;
    let high_order_line = chartjs::parse(high_order_line_json, false).unwrap();
    let scene = build_scene(&high_order_line, &m);
    let line_index = scene
        .items
        .iter()
        .position(|prim| {
            matches!(prim, Prim::Circle { r: 3.0, fill, .. } if fill.r == 0 && fill.g == 0 && fill.b == 255)
        })
        .expect("line data marker");
    let bar_index = scene
        .items
        .iter()
        .position(|prim| {
            matches!(prim, Prim::Rect { w, h, fill, .. } if *w > 15.0 && *h > 0.0 && fill.r == 255 && fill.g == 0 && fill.b == 0)
        })
        .expect("bar data rectangle");
    assert!(
        line_index < bar_index,
        "higher order line should paint behind lower order bar"
    );

    let high_order_bar_json = r##"{"type":"bar","data":{"labels":["x"],"datasets":[
      {"label":"bar","order":2.5,"data":[1],"backgroundColor":"#ff0000"},
      {"label":"line","type":"line","order":1.5,"data":[2],"borderColor":"#0000ff"}
    ]}}"##;
    let high_order_bar = chartjs::parse(high_order_bar_json, false).unwrap();
    let scene = build_scene(&high_order_bar, &m);
    let line_index = scene
        .items
        .iter()
        .position(|prim| {
            matches!(prim, Prim::Circle { r: 3.0, fill, .. } if fill.r == 0 && fill.g == 0 && fill.b == 255)
        })
        .expect("line data marker");
    let bar_index = scene
        .items
        .iter()
        .position(|prim| {
            matches!(prim, Prim::Rect { w, h, fill, .. } if *w > 15.0 && *h > 0.0 && fill.r == 255 && fill.g == 0 && fill.b == 0)
        })
        .expect("bar data rectangle");
    assert!(
        bar_index < line_index,
        "higher order bar should paint behind lower order line"
    );
}

#[test]
fn mixed_fill_index_uses_original_dataset_order_after_order_sorting() {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let json = r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[
      {"type":"line","label":"target","order":5,"data":[1,3],"borderColor":"#0000ff","fill":false},
      {"type":"bar","label":"bar","order":0,"data":[2,2]},
      {"type":"line","label":"source","order":1,"data":[3,1],"borderColor":"#ff0000","fill":"-2"}
    ]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    let scene = build_scene(&spec, &m);
    let target = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Polyline { points, stroke, .. }
                if (stroke.r, stroke.g, stroke.b) == (0, 0, 255) =>
            {
                Some(points)
            }
            _ => None,
        })
        .expect("referenced mixed line");
    let area = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Path {
                d, fill: Some(_), ..
            } => Some(d),
            _ => None,
        })
        .expect("source area polygon");
    let target_edge = target
        .iter()
        .rev()
        .map(|(x, y)| {
            format!(
                "L {} {}",
                fulgur_chart::num::fmt_num(*x),
                fulgur_chart::num::fmt_num(*y)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        area.contains(&target_edge),
        "fill index was shifted by mixed order sorting: {area}"
    );
}

#[test]
fn mixed_stack_fill_targets_the_nearest_lower_order_line() {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let json = r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[
      {"type":"bar","order":0,"data":[1,1]},
      {"type":"line","order":1,"data":[2,4],"borderColor":"#0000ff","fill":false},
      {"type":"line","order":2,"data":[5,3],"borderColor":"#ff0000","fill":"stack"},
      {"type":"line","order":3,"data":[7,8],"borderColor":"#00aa00","fill":false}
    ]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    let scene = build_scene(&spec, &m);
    let points_for = |rgb| {
        scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Polyline { points, stroke, .. } if (stroke.r, stroke.g, stroke.b) == rgb => {
                    Some(points)
                }
                _ => None,
            })
            .expect("line dataset")
    };
    let area = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Path {
                d, fill: Some(_), ..
            } => Some(d),
            _ => None,
        })
        .expect("source area polygon");
    let target_edge = |points: &[(f64, f64)]| {
        points
            .iter()
            .rev()
            .map(|(x, y)| {
                format!(
                    "L {} {}",
                    fulgur_chart::num::fmt_num(*x),
                    fulgur_chart::num::fmt_num(*y)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    assert!(
        area.contains(&target_edge(points_for((0, 0, 255)))),
        "stack fill missed the nearest lower-order line: {area}; blue={:?}; red={:?}; green={:?}",
        points_for((0, 0, 255)),
        points_for((255, 0, 0)),
        points_for((0, 170, 0))
    );
    assert!(
        !area.contains(&target_edge(points_for((0, 170, 0)))),
        "stack fill incorrectly targets the higher-order line: {area}"
    );
}
