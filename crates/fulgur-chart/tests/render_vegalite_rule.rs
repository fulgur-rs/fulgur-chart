use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::Color;
use fulgur_chart::layout;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;

fn lines(scene: &Scene) -> Vec<(f64, f64, f64, f64, Color, Vec<f64>)> {
    fn collect(items: &[Prim], output: &mut Vec<(f64, f64, f64, f64, Color, Vec<f64>)>) {
        for item in items {
            match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    dash,
                    ..
                } => output.push((*x1, *y1, *x2, *y2, *stroke, dash.clone())),
                Prim::Group { children, .. } => collect(children, output),
                _ => {}
            }
        }
    }

    let mut output = Vec::new();
    collect(&scene.items, &mut output);
    output
}

fn red() -> Color {
    Color {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    }
}

fn is_red(color: Color) -> bool {
    color.r == 255 && color.g == 0 && color.b == 0
}

#[test]
fn rule_with_only_x_spans_the_plot_height() {
    let spec = vegalite::parse(
        r##"{"width":320,"height":220,"mark":{"type":"rule","color":"red"},
        "data":{"values":[{"x":50}]},
        "encoding":{"x":{"field":"x","type":"quantitative"}}}"##,
        true,
    )
    .expect("rule mark should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("rule mark should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| line.4 == red())
        .collect::<Vec<_>>();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].0, rules[0].2);
    assert_ne!(rules[0].1, rules[0].3);
}

#[test]
fn rule_with_only_y_spans_the_plot_width() {
    let spec = vegalite::parse(
        r##"{"width":320,"height":220,"mark":{"type":"rule","color":"red"},
        "data":{"values":[{"y":"low"},{"y":"high"}]},
        "encoding":{"y":{"field":"y","type":"nominal"}}}"##,
        true,
    )
    .expect("rule mark should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("rule mark should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| line.4 == red())
        .collect::<Vec<_>>();

    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|line| line.0 != line.2));
    assert!(rules.iter().all(|line| line.1 == line.3));
    assert_ne!(rules[0].1, rules[1].1);
}

#[test]
fn rule_range_with_x2_preserves_category_y_and_styling() {
    let spec = vegalite::parse(
        r##"{"width":320,"height":220,
        "mark":{"type":"rule","color":"red","opacity":0.5,"strokeWidth":3,"strokeDash":[5,2],"clip":true},
        "data":{"values":[{"x0":10,"x1":30,"y":"low"}]},
        "encoding":{"x":{"field":"x0","type":"quantitative"},"x2":{"field":"x1"},"y":{"field":"y","type":"nominal"}}}"##,
        true,
    )
    .expect("ranged rule mark should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("ranged rule mark should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| is_red(line.4))
        .collect::<Vec<_>>();

    assert_eq!(rules.len(), 1);
    assert_ne!(rules[0].0, rules[0].2);
    assert_eq!(rules[0].1, rules[0].3);
    assert_eq!(rules[0].4.a, 0.5);
    assert_eq!(rules[0].5, vec![5.0, 2.0]);
    assert!(matches!(
        scene.items.last(),
        Some(Prim::Group { clip: Some(_), .. })
    ));
}

#[test]
fn rule_range_with_y2_maps_vertical_segment_and_temporal_x() {
    let spec = vegalite::parse(
        r##"{"width":320,"height":220,
        "mark":{"type":"rule","color":"red"},
        "data":{"values":[{"x":"2026-01-01T00:00:00Z","y0":2,"y1":8}]},
        "encoding":{"x":{"field":"x","type":"temporal"},"y":{"field":"y0","type":"quantitative"},"y2":{"field":"y1"}}}"##,
        true,
    )
    .expect("temporal ranged rule should parse");
    fulgur_chart::guard::validate_spec(&spec, &fulgur_chart::guard::InputLimits::default())
        .expect("temporal rule should pass input guards");
    let model = fulgur_chart::model::build_model(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("temporal ranged rule should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| line.4 == red())
        .collect::<Vec<_>>();

    assert_eq!(model.axes.expect("rule axes").x.kind, "temporal");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].0, rules[0].2);
    assert_ne!(rules[0].1, rules[0].3);
}

#[test]
fn rule_is_supported_as_a_layer_leaf_and_shared_category_scale() {
    let spec = vegalite::parse(
        r##"{"width":320,"height":220,"layer":[
          {"mark":"bar","data":{"values":[{"x":"A","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}},
          {"mark":{"type":"rule","color":"red","clip":true},"data":{"values":[{"x":"A"}]},"encoding":{"x":{"field":"x","type":"nominal"}}}
        ]}"##,
        true,
    )
    .expect("rule layer should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("rule layer should render");

    assert!(lines(&scene).iter().any(|line| line.4 == red()));
}

#[test]
fn rule_rejects_unsupported_encodings_and_accepts_optional_endpoints() {
    let unsupported = r##"{"mark":"rule","data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"size":{"value":2}}}"##;
    let error = vegalite::parse(unsupported, false).unwrap_err();
    assert!(
        error.contains("encoding.size") || error.contains("unsupported"),
        "{error}"
    );

    let missing_endpoint = r##"{"mark":{"type":"rule","color":"red"},"data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}}"##;
    let spec = vegalite::parse(missing_endpoint, true)
        .expect("x and y define a rule point when secondary endpoints are omitted");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("a rule with omitted secondary endpoints should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| is_red(line.4))
        .collect::<Vec<_>>();
    assert_eq!(rules.len(), 1);
    assert_eq!((rules[0].0, rules[0].1), (rules[0].2, rules[0].3));
}

#[test]
fn rule_stroke_dash_accepts_zero_entries() {
    let spec = vegalite::parse(
        r##"{"mark":{"type":"rule","color":"red","strokeDash":[0,4]},"data":{"values":[{"x":1}]},"encoding":{"x":{"field":"x","type":"quantitative"}}}"##,
        true,
    )
    .expect("Vega-Lite strokeDash permits non-negative entries");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("a zero-containing strokeDash should render");
    let rules = lines(&scene)
        .into_iter()
        .filter(|line| is_red(line.4))
        .collect::<Vec<_>>();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].5, vec![0.0, 4.0]);
}

#[test]
fn rule_schema_accepts_string_and_object_marks_and_rejects_unknown_styles() {
    let string_mark = r##"{"mark":"rule","data":{"values":[{"x":1}]},"encoding":{"x":{"field":"x","type":"quantitative"}}}"##;
    let object_mark = r##"{"mark":{"type":"rule","strokeDash":[2,1]},"data":{"values":[{"y":1}]},"encoding":{"y":{"field":"y","type":"quantitative"}}}"##;
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(string_mark).is_ok());
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(object_mark).is_ok());

    let unknown_style = object_mark.replace(
        r#""strokeDash":[2,1]"#,
        r#""strokeDash":[2,1],"futureOption":true"#,
    );
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&unknown_style).is_err());
}
