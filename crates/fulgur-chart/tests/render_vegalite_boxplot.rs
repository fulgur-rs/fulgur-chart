use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::Color;
use fulgur_chart::layout;
use fulgur_chart::model::build_model;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;

fn parse(json: &str) -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(json, true).unwrap()
}

fn measurer() -> TextMeasurer<'static> {
    TextMeasurer::new(DEFAULT_FONT).unwrap()
}

fn scene(json: &str) -> Scene {
    layout::build_scene_checked(&parse(json), &measurer()).unwrap()
}

fn box_rects(scene: &Scene) -> Vec<(f64, f64, f64, f64, Color)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Rect { x, y, w, h, fill }
                if *w < scene.width * 0.9 && *h < scene.height * 0.9 =>
            {
                Some((*x, *y, *w, *h, *fill))
            }
            _ => None,
        })
        .collect()
}

fn line_strokes(scene: &Scene) -> Vec<(Color, f64, f64, f64, f64, f64)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Line {
                x1,
                y1,
                x2,
                y2,
                stroke,
                stroke_width,
                ..
            } => Some((*stroke, *stroke_width, *x1, *y1, *x2, *y2)),
            _ => None,
        })
        .collect()
}

fn finite_scene(scene: &Scene) -> bool {
    let mut finite = true;
    fn visit(items: &[Prim], finite: &mut bool) {
        for item in items {
            match item {
                Prim::Rect { x, y, w, h, .. } => {
                    *finite &= [*x, *y, *w, *h].into_iter().all(f64::is_finite);
                }
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke_width,
                    ..
                } => {
                    *finite &= [*x1, *y1, *x2, *y2, *stroke_width]
                        .into_iter()
                        .all(f64::is_finite);
                }
                Prim::Circle {
                    cx,
                    cy,
                    r,
                    stroke_width,
                    ..
                } => {
                    *finite &= [*cx, *cy, *r, *stroke_width]
                        .into_iter()
                        .all(f64::is_finite);
                }
                Prim::Group { children, .. } => visit(children, finite),
                _ => {}
            }
        }
    }
    visit(&scene.items, &mut finite);
    finite
}

#[test]
fn boxplot_vertical_horizontal_and_1d_geometry() {
    let vertical = scene(
        r##"{"width":320,"height":220,"mark":"boxplot","data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let horizontal = scene(
        r##"{"width":320,"height":220,"mark":"boxplot","data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4}]},"encoding":{"x":{"field":"value","type":"quantitative"},"y":{"field":"group","type":"nominal"}}}"##,
    );
    let one_dimensional = scene(
        r##"{"width":320,"height":220,"mark":"boxplot","data":{"values":[{"value":1},{"value":2},{"value":3},{"value":4}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let vertical_box = box_rects(&vertical)[0];
    let horizontal_box = box_rects(&horizontal)[0];
    let one_dimensional_box = box_rects(&one_dimensional)[0];

    assert!(
        vertical_box.3 > vertical_box.2,
        "vertical box: {vertical_box:?}"
    );
    assert!(
        horizontal_box.2 > horizontal_box.3,
        "horizontal box: {horizontal_box:?}"
    );
    let one_d_center = one_dimensional_box.0 + one_dimensional_box.2 / 2.0;
    assert!((one_dimensional.width * 0.3..=one_dimensional.width * 0.7).contains(&one_d_center));
}

#[test]
fn boxplot_groups_are_side_by_side_deterministically() {
    let json = r##"{"width":320,"height":220,"mark":"boxplot","data":{"values":[{"group":"A","detail":"first","value":1},{"group":"A","detail":"second","value":2}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"detail":{"field":"detail","type":"nominal"}}}"##;
    let first = scene(json);
    let second = scene(json);
    let mut centers = box_rects(&first)
        .into_iter()
        .map(|rect| rect.0 + rect.2 / 2.0)
        .collect::<Vec<_>>();
    centers.sort_by(f64::total_cmp);
    assert_eq!(centers.len(), 2);
    assert!(centers[0] < centers[1]);
    assert_eq!(first, second);
}

#[test]
fn boxplot_styles_apply_mark_encoding_and_part_precedence() {
    let rendered = scene(
        r##"{"width":320,"height":220,"mark":{"type":"boxplot","color":"red","box":{"fill":"blue","stroke":"green","strokeWidth":4},"median":{"color":"orange","strokeWidth":5},"outliers":false},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":100}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"color":{"value":"purple"}}}"##,
    );
    let rects = box_rects(&rendered);
    assert!(
        rects
            .iter()
            .any(|rect| (rect.4.r, rect.4.g, rect.4.b) == (0, 0, 255))
    );
    let lines = line_strokes(&rendered);
    assert!(
        lines
            .iter()
            .any(|line| { (line.0.r, line.0.g, line.0.b, line.1) == (0, 128, 0, 4.0) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { (line.0.r, line.0.g, line.0.b, line.1) == (255, 165, 0, 5.0) })
    );
    assert!(
        !rendered
            .items
            .iter()
            .any(|item| matches!(item, Prim::Circle { .. }))
    );
}

#[test]
fn boxplot_size_and_opacity_encodings_reach_component_geometry() {
    let rendered = scene(
        r##"{"mark":"boxplot","data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4} ]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"size":{"value":22},"opacity":{"value":0.5}}}"##,
    );
    let boxes = box_rects(&rendered);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].2, 22.0);
    assert_eq!(boxes[0].4.a, 0.5);

    let field_encoded = scene(
        r##"{"mark":"boxplot","data":{"values":[{"group":"A","value":1,"width":24,"alpha":0.25},{"group":"A","value":2,"width":24,"alpha":0.25},{"group":"A","value":3,"width":24,"alpha":0.25},{"group":"A","value":4,"width":24,"alpha":0.25}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"size":{"field":"width","type":"quantitative"},"opacity":{"field":"alpha","type":"quantitative"}}}"##,
    );
    let field_box = box_rects(&field_encoded)[0];
    assert_eq!(field_box.2, 24.0);
    assert_eq!(field_box.4.a, 0.25);
}

#[test]
fn boxplot_opacity_precedence_is_component_then_encoding_then_mark() {
    let mark_only = scene(
        r##"{"mark":{"type":"boxplot","clip":false,"opacity":0.2},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}}"##,
    );
    assert!((box_rects(&mark_only)[0].4.a - 0.2).abs() < 1e-6);

    let encoding_over_mark = scene(
        r##"{"mark":{"type":"boxplot","clip":false,"opacity":0.2},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"opacity":{"value":0.5}}}"##,
    );
    assert!((box_rects(&encoding_over_mark)[0].4.a - 0.5).abs() < 1e-6);

    let component_over_encoding = scene(
        r##"{"mark":{"type":"boxplot","clip":false,"opacity":0.2,"box":{"fill":"red","opacity":0.75}},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"opacity":{"value":0.5}}}"##,
    );
    assert!((box_rects(&component_over_encoding)[0].4.a - 0.75).abs() < 1e-6);
}

#[test]
fn boxplot_component_visibility_and_style_objects_are_applied() {
    let rendered = scene(
        r##"{"mark":{"type":"boxplot","extent":"min-max","box":false,"median":false,"outliers":false,"rule":false,"ticks":{"color":"blue","size":18}},"data":{"values":[{"value":1},{"value":2},{"value":3}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"##,
    );
    assert!(box_rects(&rendered).is_empty());
    assert!(
        !rendered
            .items
            .iter()
            .any(|item| matches!(item, Prim::Circle { .. }))
    );
    let blue = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 1.0,
    };
    let blue_ticks = line_strokes(&rendered)
        .into_iter()
        .filter(|line| line.0 == blue)
        .collect::<Vec<_>>();
    assert_eq!(blue_ticks.len(), 2);
    assert!(blue_ticks.iter().all(|line| line.1 == 1.0));
}

#[test]
fn boxplot_defaults_match_vegalite_composite_mark_defaults() {
    let rendered = scene(
        r##"{"width":320,"height":220,"mark":"boxplot","data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4},{"group":"A","value":100}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let boxes = box_rects(&rendered);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].2, 14.0, "default box size is 14px");
    let lines = line_strokes(&rendered);
    let black = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };
    let white = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 1.0,
    };
    assert_eq!(
        lines.iter().filter(|line| line.0 == black).count(),
        1,
        "default end ticks are hidden"
    );
    assert!(
        lines.iter().any(|line| line.0 == white && line.1 == 1.0),
        "default median tick is white"
    );
    let outlier_area = rendered
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Circle { r, .. } => Some(std::f64::consts::PI * r * r),
            _ => None,
        })
        .expect("default outlier point");
    assert!((outlier_area - 30.0).abs() < 0.01);
    let (fill, _, stroke_width) = rendered
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Circle {
                fill,
                stroke,
                stroke_width,
                ..
            } => Some((*fill, *stroke, *stroke_width)),
            _ => None,
        })
        .expect("default outlier point style");
    assert_eq!(fill.a, 0.0, "default point marks are hollow");
    assert_eq!(stroke_width, 1.0);
}

#[test]
fn boxplot_outlier_size_uses_point_area_semantics() {
    let rendered = scene(
        r##"{"mark":{"type":"boxplot","outliers":{"size":18,"color":"red"}},"data":{"values":[{"value":1},{"value":2},{"value":3},{"value":4},{"value":100}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let (area, fill, stroke) = rendered
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Circle {
                r, fill, stroke, ..
            } => Some((std::f64::consts::PI * r * r, *fill, *stroke)),
            _ => None,
        })
        .expect("outlier point");
    assert!((area - 18.0).abs() < 0.01);
    assert_eq!(fill.a, 0.0, "color alone keeps default points hollow");
    assert_eq!((stroke.r, stroke.g, stroke.b), (255, 0, 0));
}

#[test]
fn vegalite_boxplot_handles_singleton_and_constant_groups() {
    let rendered = scene(
        r##"{"mark":"boxplot","data":{"values":[{"group":"single","value":7},{"group":"constant","value":4},{"group":"constant","value":4},{"group":"constant","value":4}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}}"##,
    );
    assert!(finite_scene(&rendered));
}

#[test]
fn boxplot_extent_zero_omits_missing_whisker_primitives() {
    let rendered = scene(
        r##"{"mark":{"type":"boxplot","extent":0,"rule":{"color":"red"},"ticks":{"color":"blue"}},"data":{"values":[{"value":1},{"value":2}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    };
    let blue = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 1.0,
    };
    assert!(
        !line_strokes(&rendered)
            .iter()
            .any(|line| line.0 == red || line.0 == blue)
    );
    assert_eq!(
        rendered
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Circle { .. }))
            .count(),
        2
    );
}

#[test]
fn boxplot_axis_domain_includes_outliers_and_clips_to_hard_bounds() {
    let spec = parse(
        r##"{"width":320,"height":220,"mark":{"type":"boxplot","clip":true},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4},{"group":"A","value":100}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative","scale":{"domain":[0,10]}}}}"##,
    );
    let model = build_model(&spec, &measurer());
    let y = &model.axes.as_ref().unwrap().y;
    assert_eq!((y.min, y.max), (Some(0.0), Some(10.0)));
    let rendered = layout::build_scene_checked(&spec, &measurer()).unwrap();
    assert!(
        rendered
            .items
            .iter()
            .any(|item| matches!(item, Prim::Group { clip: Some(_), .. }))
    );
    let unbounded = build_model(
        &parse(
            r#"{"mark":"boxplot","data":{"values":[{"value":1},{"value":2},{"value":3},{"value":4},{"value":100}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"#,
        ),
        &measurer(),
    );
    assert!(unbounded.axes.unwrap().y.max.unwrap() >= 100.0);
}

#[test]
fn vegalite_boxplot_model_reports_type_axes_and_groups() {
    let spec = parse(
        r##"{"mark":"boxplot","data":{"values":[{"group":"A","color":"red","value":1},{"group":"B","color":"blue","value":2}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"color":{"field":"color","type":"nominal"}}}"##,
    );
    let model = build_model(&spec, &measurer());
    assert_eq!(model.meta.r#type, "vegaBoxPlot");
    assert_eq!(model.counts.datasets, 2);
    assert_eq!(model.axes.as_ref().unwrap().x.kind, "category");
    assert_eq!(model.axes.as_ref().unwrap().y.kind, "linear");
}

#[test]
fn vega_boxplot_guard_enforces_point_category_and_primitive_limits() {
    let spec = parse(
        r##"{"mark":"boxplot","data":{"values":[{"group":"A","detail":"first","value":1},{"group":"A","detail":"second","value":2},{"group":"B","detail":"third","value":3}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"},"detail":{"field":"detail","type":"nominal"}}}"##,
    );
    let point_limited = fulgur_chart::guard::InputLimits {
        max_total_data_points: 2,
        ..fulgur_chart::guard::InputLimits::default()
    };
    let category_limited = fulgur_chart::guard::InputLimits {
        max_categories: 1,
        ..fulgur_chart::guard::InputLimits::default()
    };
    let group_limited = fulgur_chart::guard::InputLimits {
        max_series: 2,
        ..fulgur_chart::guard::InputLimits::default()
    };
    let primitive_limited = fulgur_chart::guard::InputLimits {
        max_categorical_primitives: 1,
        ..fulgur_chart::guard::InputLimits::default()
    };
    assert!(fulgur_chart::guard::validate_spec(&spec, &point_limited).is_err());
    assert!(fulgur_chart::guard::validate_spec(&spec, &category_limited).is_err());
    assert!(fulgur_chart::guard::validate_spec(&spec, &group_limited).is_err());
    assert!(fulgur_chart::guard::validate_spec(&spec, &primitive_limited).is_err());

    let outlined = parse(
        r##"{"mark":{"type":"boxplot","extent":"min-max","box":{"stroke":"black"},"ticks":false},"data":{"values":[{"value":1},{"value":2},{"value":3}]},"encoding":{"y":{"field":"value","type":"quantitative"}}}"##,
    );
    let underestimated_primitive_limit = fulgur_chart::guard::InputLimits {
        max_categorical_primitives: 3,
        ..fulgur_chart::guard::InputLimits::default()
    };
    assert!(
        fulgur_chart::guard::validate_spec(&outlined, &underestimated_primitive_limit).is_err(),
        "box outline edges must count toward the primitive budget"
    );
}

#[test]
fn boxplot_whiskers_do_not_cross_the_box_in_either_orientation() {
    for (json, vertical) in [
        (
            r##"{"width":320,"height":220,"mark":{"type":"boxplot","clip":false},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4},{"group":"A","value":5}]},"encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}}"##,
            true,
        ),
        (
            r##"{"width":320,"height":220,"mark":{"type":"boxplot","clip":false},"data":{"values":[{"group":"A","value":1},{"group":"A","value":2},{"group":"A","value":3},{"group":"A","value":4},{"group":"A","value":5}]},"encoding":{"x":{"field":"value","type":"quantitative"},"y":{"field":"group","type":"nominal"}}}"##,
            false,
        ),
    ] {
        let rendered = scene(json);
        let rect = box_rects(&rendered)[0];
        let center_x = rect.0 + rect.2 / 2.0;
        let center_y = rect.1 + rect.3 / 2.0;
        let rules = line_strokes(&rendered)
            .into_iter()
            .filter(|line| {
                line.0.r == 0
                    && line.0.g == 0
                    && line.0.b == 0
                    && if vertical {
                        (line.2 - center_x).abs() < 1e-6 && (line.4 - center_x).abs() < 1e-6
                    } else {
                        (line.3 - center_y).abs() < 1e-6 && (line.5 - center_y).abs() < 1e-6
                    }
            })
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 2, "whiskers should stop at the box edges");
        for line in rules {
            let (start, end, box_start, box_end) = if vertical {
                (
                    line.3.min(line.5),
                    line.3.max(line.5),
                    rect.1,
                    rect.1 + rect.3,
                )
            } else {
                (
                    line.2.min(line.4),
                    line.2.max(line.4),
                    rect.0,
                    rect.0 + rect.2,
                )
            };
            assert!(
                end <= box_start + 1e-6 || start >= box_end - 1e-6,
                "whisker {start}..{end} crosses box {box_start}..{box_end}"
            );
        }
    }
}

#[test]
fn boxplot_long_category_label_keeps_measurement_plot_area() {
    let category = "a".repeat(50);
    let json = format!(
        r#"{{"width":320,"height":220,"mark":{{"type":"boxplot","clip":false}},"data":{{"values":[{{"group":"{category}","value":1}},{{"group":"{category}","value":2}},{{"group":"{category}","value":3}},{{"group":"{category}","value":4}},{{"group":"{category}","value":5}}]}},"encoding":{{"x":{{"field":"group","type":"nominal"}},"y":{{"field":"value","type":"quantitative"}}}}}}"#
    );
    let rendered = scene(&json);
    let box_rect = box_rects(&rendered)[0];
    assert!(
        box_rect.3 > 20.0,
        "long horizontal category labels must not collapse the value-axis plot area: {box_rect:?}"
    );
}

#[test]
fn boxplot_render_honors_relaxed_caller_series_limit() {
    let values = (0..1001)
        .map(|index| format!(r#"{{"group":"A","detail":"{index}","value":{index}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{"mark":"boxplot","data":{{"values":[{values}]}},"encoding":{{"x":{{"field":"group","type":"nominal"}},"y":{{"field":"value","type":"quantitative"}},"detail":{{"field":"detail","type":"nominal"}}}}}}"#
    );
    let limits = fulgur_chart::guard::InputLimits {
        max_series: 1100,
        ..fulgur_chart::guard::InputLimits::default()
    };
    let spec = vegalite::parse_with_limits(&json, true, &limits).unwrap();
    fulgur_chart::guard::validate_spec(&spec, &limits).unwrap();
    layout::build_scene_checked_with_limits(&spec, &measurer(), &limits)
        .expect("layout should keep the caller's relaxed max_series limit");
}

#[test]
fn boxplot_rejects_nonfinite_mapped_outlier_geometry() {
    let spec = parse(
        r##"{"width":320,"height":220,"mark":{"type":"boxplot","clip":false},"data":{"values":[{"value":1},{"value":2},{"value":3},{"value":4},{"value":1e308}]},"encoding":{"y":{"field":"value","type":"quantitative","scale":{"domain":[0,1]}}}}"##,
    );
    let error = layout::build_scene_checked(&spec, &measurer()).unwrap_err();
    assert!(error.contains("finite"), "unexpected error: {error}");
}
