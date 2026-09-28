use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::guard::{InputLimits, validate_spec};
use fulgur_chart::ir::{ChartKind, Color};
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

fn red() -> Color {
    Color {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    }
}

fn red_lines(scene: &Scene) -> Vec<(f64, f64, f64, f64, f64)> {
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
            } if *stroke == red() => Some((*x1, *y1, *x2, *y2, *stroke_width)),
            _ => None,
        })
        .collect()
}

#[test]
fn errorbar_vertical_and_horizontal_ranges_map_to_axis_endpoints() {
    let vertical = parse(
        r##"{"width":320,"height":220,"mark":{"type":"errorbar","color":"red"},
        "data":{"values":[{"x":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},
        "y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let vertical_scene = layout::build_scene_checked(&vertical, &measurer()).unwrap();
    let vertical_rules = red_lines(&vertical_scene);
    assert_eq!(vertical_rules.len(), 1);
    assert_eq!(vertical_rules[0].0, vertical_rules[0].2);
    assert_ne!(vertical_rules[0].1, vertical_rules[0].3);

    let horizontal = parse(
        r##"{"width":320,"height":220,"mark":{"type":"errorbar","orient":"horizontal","color":"red"},
        "data":{"values":[{"y":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"low","type":"quantitative"},"x2":{"field":"high"},
        "y":{"field":"y","type":"nominal"}}}"##,
    );
    let horizontal_scene = layout::build_scene_checked(&horizontal, &measurer()).unwrap();
    let horizontal_rules = red_lines(&horizontal_scene);
    assert_eq!(horizontal_rules.len(), 1);
    assert_ne!(horizontal_rules[0].0, horizontal_rules[0].2);
    assert_eq!(horizontal_rules[0].1, horizontal_rules[0].3);
}

#[test]
fn errorbar_1d_uses_plot_center_on_the_other_axis() {
    let spec = parse(
        r##"{"width":320,"height":220,"mark":{"type":"errorbar","orient":"vertical","color":"red"},
        "data":{"values":[{"low":2,"high":8}]},
        "encoding":{"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
    let lines = red_lines(&scene);
    assert_eq!(lines.len(), 1);
    assert!((scene.width * 0.3..=scene.width * 0.7).contains(&lines[0].0));
}

#[test]
fn errorbar_ticks_are_optional_and_use_part_styles() {
    let spec = parse(
        r##"{"width":320,"height":220,"mark":{"type":"errorbar","color":"red",
        "rule":{"color":"blue","strokeWidth":3},"ticks":{"color":"green","size":12}},
        "data":{"values":[{"x":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},
        "y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
    let lines = scene
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
            } if *stroke
                == Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 1.0,
                }
                || *stroke
                    == Color {
                        r: 0,
                        g: 128,
                        b: 0,
                        a: 1.0,
                    } =>
            {
                Some((*x1, *y1, *x2, *y2, *stroke_width, *stroke))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.5
                == Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 1.0
                })
            .count(),
        1
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.5
                == Color {
                    r: 0,
                    g: 128,
                    b: 0,
                    a: 1.0
                })
            .count(),
        2
    );
    assert!(lines.iter().any(|line| line.4 == 3.0));
    assert!(
        lines
            .iter()
            .filter(|line| line.5
                == Color {
                    r: 0,
                    g: 128,
                    b: 0,
                    a: 1.0
                })
            .all(|line| (line.0 - line.2).abs() > 0.0)
    );
}

#[test]
fn error_mark_guard_rejects_misaligned_and_nonfinite_ranges() {
    let mut spec = parse(
        r##"{"mark":{"type":"errorbar","ticks":true},"data":{"values":[{"x":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    if let ChartKind::ErrorMark(data) = &mut spec.kind {
        data.ranges[0].series_index = 5;
    } else {
        panic!("error-mark kind expected");
    }
    assert!(validate_spec(&spec, &InputLimits::default()).is_err());

    if let ChartKind::ErrorMark(data) = &mut spec.kind {
        data.ranges[0].series_index = 0;
        data.ranges[0].lower = f64::NAN;
    }
    assert!(validate_spec(&spec, &InputLimits::default()).is_err());
}

#[test]
fn error_mark_guard_bounds_generated_primitives() {
    let spec = parse(
        r##"{"mark":{"type":"errorbar","ticks":true},"data":{"values":[{"x":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let limits = InputLimits {
        max_categorical_primitives: 2,
        ..InputLimits::default()
    };
    assert!(validate_spec(&spec, &limits).is_err());
}

#[test]
fn error_mark_model_reports_chart_type_and_axes() {
    let spec = parse(
        r##"{"mark":"errorbar","data":{"values":[{"x":"A","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let model = build_model(&spec, &measurer());
    assert_eq!(model.meta.r#type, "errorbar");
    let axes = model.axes.expect("error marks expose their axes");
    assert_eq!(axes.x.kind, "category");
    assert_eq!(axes.y.kind, "linear");

    let horizontal = parse(
        r##"{"mark":{"type":"errorbar","orient":"horizontal"},"data":{"values":[{"y":"A","lo":2,"hi":8}]},
        "encoding":{"x":{"field":"lo","type":"quantitative"},"x2":{"field":"hi"},"y":{"field":"y","type":"nominal"}}}"##,
    );
    let model = build_model(&horizontal, &measurer());
    let axes = model
        .axes
        .expect("horizontal error marks expose their axes");
    assert_eq!(axes.x.kind, "linear");
    assert_eq!(axes.y.kind, "category");
}

fn filled_paths(scene: &Scene) -> Vec<(&str, Color)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::ClippedPath {
                d,
                fill: Some(fill),
                ..
            }
            | Prim::Path {
                d,
                fill: Some(fill),
                ..
            } => Some((d.as_str(), *fill)),
            _ => None,
        })
        .collect()
}

fn path_coordinates(path: &str) -> Vec<(f64, f64)> {
    let tokens = path.split_whitespace().collect::<Vec<_>>();
    let mut coordinates = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let command = tokens[index];
        index += 1;
        if matches!(command, "M" | "L") {
            let x = tokens[index].parse().unwrap();
            let y = tokens[index + 1].parse().unwrap();
            coordinates.push((x, y));
            index += 2;
        }
    }
    coordinates
}

#[test]
fn errorband_vertical_horizontal_and_1d_geometry() {
    let vertical = parse(
        r##"{"mark":"errorband","data":{"values":[{"x":"a","low":1,"high":4},{"x":"b","low":2,"high":4},{"x":"c","low":3,"high":4}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let vertical_scene = layout::build_scene_checked(&vertical, &measurer()).unwrap();
    let vertical_fill = filled_paths(&vertical_scene);
    assert_eq!(vertical_fill.len(), 1);
    let vertical_points = path_coordinates(vertical_fill[0].0);
    assert!(vertical_points[1].0 > vertical_points[0].0);
    assert!((vertical_points[1].1 - vertical_points[0].1).abs() < 0.01);

    let horizontal = parse(
        r##"{"mark":{"type":"errorband","orient":"horizontal"},"data":{"values":[{"y":"a","low":1,"high":4},{"y":"b","low":2,"high":4}]},
        "encoding":{"x":{"field":"low","type":"quantitative"},"x2":{"field":"high"},"y":{"field":"y","type":"nominal"}}}"##,
    );
    let horizontal_scene = layout::build_scene_checked(&horizontal, &measurer()).unwrap();
    let horizontal_fill = filled_paths(&horizontal_scene);
    assert_eq!(horizontal_fill.len(), 1);
    let horizontal_points = path_coordinates(horizontal_fill[0].0);
    assert!((horizontal_points[1].0 - horizontal_points[0].0).abs() < 0.01);
    assert!(horizontal_points[1].1 > horizontal_points[0].1);

    let one_dimensional = parse(
        r##"{"mark":{"type":"errorband","orient":"vertical"},"data":{"values":[{"low":2,"high":8}]},
        "encoding":{"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let one_dimensional_scene = layout::build_scene_checked(&one_dimensional, &measurer()).unwrap();
    let one_dimensional_fills = filled_paths(&one_dimensional_scene);
    assert_eq!(one_dimensional_fills.len(), 1);
    let clip = one_dimensional_scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::ClippedPath {
                fill: Some(_),
                clip,
                ..
            } => Some(**clip),
            _ => None,
        })
        .expect("one-dimensional errorband fill is clipped to the plot");
    let coordinates = path_coordinates(one_dimensional_fills[0].0);
    let x_coordinates = coordinates.iter().map(|point| point.0);
    let min_x = x_coordinates.clone().fold(f64::INFINITY, f64::min);
    let max_x = x_coordinates.fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (min_x - clip.x).abs() < 0.01,
        "minimum path x {min_x} != clip left {}; path: {}",
        clip.x,
        one_dimensional_fills[0].0
    );
    assert!(
        (max_x - (clip.x + clip.w)).abs() < 0.01,
        "maximum path x {max_x} != clip right {}; path: {}",
        clip.x + clip.w,
        one_dimensional_fills[0].0
    );
}

#[test]
fn errorband_single_point_group_emits_no_fill() {
    let spec = parse(
        r##"{"mark":"errorband","data":{"values":[{"x":"only","low":2,"high":8}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
    assert!(filled_paths(&scene).is_empty());
}

#[test]
fn errorband_styles_control_fill_and_boundaries() {
    let spec = parse(
        r##"{"mark":{"type":"errorband","band":{"fill":"blue","opacity":0.25},"borders":{"stroke":"red","strokeWidth":2}},
        "data":{"values":[{"x":"a","low":1,"high":4},{"x":"b","low":2,"high":6}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"}}}"##,
    );
    let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
    let fills = filled_paths(&scene);
    assert_eq!(fills.len(), 1);
    assert_eq!(
        fills[0].1,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 0.25
        }
    );
    let borders = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::ClippedPath {
                stroke: Some(stroke),
                stroke_width,
                fill: None,
                ..
            } if *stroke == red() => Some(*stroke_width),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(borders, vec![2.0, 2.0]);
}

#[test]
fn errorband_all_interpolations_emit_finite_paths() {
    for interpolation in [
        "linear",
        "linear-closed",
        "step",
        "step-before",
        "step-after",
        "basis",
        "basis-open",
        "basis-closed",
        "cardinal",
        "cardinal-open",
        "cardinal-closed",
        "bundle",
        "monotone",
    ] {
        let json = format!(
            r##"{{"mark":{{"type":"errorband","interpolate":"{interpolation}","tension":0.4}},"data":{{"values":[{{"x":"a","low":1,"high":4}},{{"x":"b","low":3,"high":7}},{{"x":"c","low":2,"high":8}}]}},"encoding":{{"x":{{"field":"x","type":"nominal"}},"y":{{"field":"low","type":"quantitative"}},"y2":{{"field":"high"}}}}}}"##
        );
        let spec = parse(&json);
        let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
        let fills = filled_paths(&scene);
        assert_eq!(fills.len(), 1, "{interpolation}");
        let path = fills[0].0.to_ascii_lowercase();
        assert!(
            !path.contains("nan") && !path.contains("inf"),
            "{interpolation}: {path}"
        );
    }
}

#[test]
fn errorband_groups_keep_detail_paths_separate() {
    let spec = parse(
        r##"{"mark":"errorband","data":{"values":[
        {"x":"a","low":1,"high":4,"detail":"one"},{"x":"b","low":2,"high":5,"detail":"one"},
        {"x":"a","low":3,"high":6,"detail":"two"},{"x":"b","low":4,"high":7,"detail":"two"}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"low","type":"quantitative"},"y2":{"field":"high"},"detail":{"field":"detail","type":"nominal"}}}"##,
    );
    let scene = layout::build_scene_checked(&spec, &measurer()).unwrap();
    assert_eq!(filled_paths(&scene).len(), 2);
}
