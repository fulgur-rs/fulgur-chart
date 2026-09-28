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
