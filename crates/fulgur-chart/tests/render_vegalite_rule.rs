use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::{ChartKind, Color};
use fulgur_chart::layout;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;

type RuleLine = (f64, f64, f64, f64, Color, Vec<f64>);

fn lines(scene: &Scene) -> Vec<RuleLine> {
    fn collect(items: &[Prim], output: &mut Vec<RuleLine>) {
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
fn rule_range_with_x_and_x2_without_y_uses_the_y_axis_center() {
    let json = r##"{"width":320,"height":220,"mark":{"type":"rule","color":"red"},
    "data":{"values":[{"x0":10,"x1":30}]},
    "encoding":{"x":{"field":"x0","type":"quantitative"},"x2":{"field":"x1"}}}"##;

    for strict in [false, true] {
        let spec = vegalite::parse(json, strict)
            .expect("x/x2 ranged rules should not require an orthogonal y encoding");
        let ChartKind::VegaRule(data) = &spec.kind else {
            panic!("VegaRule expected, got {:?}", spec.kind);
        };
        assert!(matches!(
            (data.segments[0].x1, data.segments[0].x2),
            (
                fulgur_chart::ir::VegaRulePosition::Quantitative(10.0),
                fulgur_chart::ir::VegaRulePosition::Quantitative(30.0)
            )
        ));
        assert_eq!(data.segments[0].y1, data.segments[0].y2);

        let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
            .expect("x/x2 ranged rule should render without y");
        let rules = lines(&scene)
            .into_iter()
            .filter(|line| is_red(line.4))
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 1);
        assert_ne!(rules[0].0, rules[0].2);
        assert_eq!(rules[0].1, rules[0].3);
    }
}

#[test]
fn rule_range_with_y_and_y2_without_x_uses_the_x_axis_center() {
    let json = r##"{"width":320,"height":220,"mark":{"type":"rule","color":"red"},
    "data":{"values":[{"y0":10,"y1":30}]},
    "encoding":{"y":{"field":"y0","type":"quantitative"},"y2":{"field":"y1"}}}"##;

    for strict in [false, true] {
        let spec = vegalite::parse(json, strict)
            .expect("y/y2 ranged rules should not require an orthogonal x encoding");
        let ChartKind::VegaRule(data) = &spec.kind else {
            panic!("VegaRule expected, got {:?}", spec.kind);
        };
        assert!(matches!(
            (data.segments[0].y1, data.segments[0].y2),
            (
                fulgur_chart::ir::VegaRulePosition::Quantitative(10.0),
                fulgur_chart::ir::VegaRulePosition::Quantitative(30.0)
            )
        ));
        assert_eq!(data.segments[0].x1, data.segments[0].x2);

        let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
            .expect("y/y2 ranged rule should render without x");
        let rules = lines(&scene)
            .into_iter()
            .filter(|line| is_red(line.4))
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].0, rules[0].2);
        assert_ne!(rules[0].1, rules[0].3);
    }
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
fn rule_secondary_endpoints_still_require_their_primary_channels() {
    let specs = [
        r##"{"mark":"rule","data":{"values":[{"x2":2,"y":1}]},"encoding":{"x2":{"field":"x2","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}}"##,
        r##"{"mark":"rule","data":{"values":[{"x":1,"y2":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y2":{"field":"y2","type":"quantitative"}}}"##,
    ];

    for json in specs {
        for strict in [false, true] {
            assert!(
                vegalite::parse(json, strict).is_err(),
                "secondary endpoints require their primary channels: {json}"
            );
        }
    }
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
fn rule_rejects_oversized_stroke_dash_before_converting_entries() {
    let dash = vec!["1"; 65].join(",");
    let json = format!(
        r##"{{"mark":{{"type":"rule","strokeDash":[{dash}]}},"data":{{"values":[{{"x":1}}]}},"encoding":{{"x":{{"field":"x","type":"quantitative"}}}}}}"##
    );

    for strict in [false, true] {
        let error = vegalite::parse(&json, strict).unwrap_err();

        assert!(error.contains("64"), "strict={strict}: {error}");
    }
}

#[test]
fn rule_rejects_excessive_total_dash_expansion_before_building_scene() {
    let dash = vec!["1"; 64].join(",");
    let json = format!(
        r##"{{"mark":{{"type":"rule","strokeDash":[{dash}]}},"data":{{"values":[{{"x":1}}]}},"encoding":{{"x":{{"field":"x","type":"quantitative"}}}}}}"##
    );
    let mut spec = vegalite::parse(&json, true).expect("bounded rule should parse");
    let ChartKind::VegaRule(data) = &mut spec.kind else {
        panic!("expected Vega-Lite rule data");
    };
    let segment = data.segments[0].clone();
    data.segments = vec![segment; 15_626];

    let error = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect_err("the expanded dash payload must be bounded before layout");

    assert!(error.contains("strokeDash expansion"), "{error}");
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

#[test]
fn rule_rejects_category_labels_that_collide_across_json_value_types() {
    let specs = [
        r##"{"mark":"rule","data":{"values":[{"x":1},{"x":"1"}]},"encoding":{"x":{"field":"x","type":"nominal"}}}"##,
        r##"{"mark":"rule","data":{"values":[{"x":1,"end":"1","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"x2":{"field":"end"},"y":{"field":"y","type":"quantitative"}}}"##,
        r##"{"mark":"rule","data":{"values":[{"x":1,"y":1,"end":"1"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"nominal"},"y2":{"field":"end"}}}"##,
        r##"{"mark":"rule","data":{"values":[{"x":1,"group":1},{"x":2,"group":"1"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"color":{"field":"group","type":"nominal"}}}"##,
    ];

    for json in specs {
        for strict in [false, true] {
            let result = vegalite::parse(json, strict);

            assert!(
                result
                    .as_ref()
                    .is_err_and(|error| error.contains("JSON value types")),
                "strict={strict}: ambiguous category labels should be rejected, got {result:?}"
            );
        }
    }
}
