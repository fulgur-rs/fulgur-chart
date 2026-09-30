use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::Color;
use fulgur_chart::layout;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;

fn red_ticks(scene: &Scene) -> Vec<(f64, f64, f64, f64)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Rect { x, y, w, h, fill }
                if *fill
                    == (Color {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 1.0,
                    }) =>
            {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect()
}

fn rects(scene: &Scene) -> Vec<(f64, f64, f64, f64, Color)> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Rect { x, y, w, h, fill }
                if (*w - scene.width).abs() > f64::EPSILON
                    || (*h - scene.height).abs() > f64::EPSILON =>
            {
                Some((*x, *y, *w, *h, *fill))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn horizontal_tick_strip_plot_renders_one_sized_mark_per_record() {
    let spec = vegalite::parse(
        r##"{
          "width":240,"height":180,
          "mark":{"type":"tick","orient":"horizontal","size":12,"color":"red"},
          "data":{"values":[{"value":2,"group":"A"},{"value":8,"group":"B"}]},
          "encoding":{
            "x":{"field":"value","type":"quantitative"},
            "y":{"field":"group","type":"nominal"}
          }
        }"##,
        true,
    )
    .expect("tick strip plot should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("tick strip plot should render");
    let ticks = red_ticks(&scene);

    assert_eq!(ticks.len(), 2);
    assert!(ticks.iter().all(|tick| tick.2 == 12.0 && tick.3 == 1.0));
    assert_ne!(
        ticks[0].0, ticks[1].0,
        "quantitative positions should map to x"
    );
    assert_ne!(ticks[0].1, ticks[1].1, "categorical groups should map to y");
}

#[test]
fn vertical_ticks_use_y_for_length_and_x_for_thickness() {
    let spec = vegalite::parse(
        r##"{
          "width":240,"height":180,
          "config":{"tick":{"thickness":2}},
          "mark":{"type":"tick","orient":"vertical","size":10,"color":"red"},
          "data":{"values":[{"value":2,"group":"A"},{"value":8,"group":"B"}]},
          "encoding":{
            "x":{"field":"group","type":"nominal"},
            "y":{"field":"value","type":"quantitative"}
          }
        }"##,
        true,
    )
    .expect("vertical tick strip plot should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("vertical tick strip plot should render");
    let ticks = red_ticks(&scene);

    assert_eq!(ticks.len(), 2);
    assert!(ticks.iter().all(|tick| tick.2 == 2.0 && tick.3 == 10.0));
    assert_ne!(
        ticks[0].0, ticks[1].0,
        "categorical positions should map to x"
    );
    assert_ne!(
        ticks[0].1, ticks[1].1,
        "quantitative positions should map to y"
    );
}

#[test]
fn omitted_orthogonal_position_centers_x_only_and_y_only_ticks() {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    for (json, use_x_axis) in [
        (
            r##"{"width":240,"height":180,"mark":{"type":"tick","size":6,"color":"red"},"data":{"values":[{"x":2},{"x":8}]},"encoding":{"x":{"field":"x","type":"quantitative"}}}"##,
            true,
        ),
        (
            r##"{"width":240,"height":180,"mark":{"type":"tick","orient":"vertical","size":6,"color":"red"},"data":{"values":[{"y":2},{"y":8}]},"encoding":{"y":{"field":"y","type":"quantitative"}}}"##,
            false,
        ),
    ] {
        let spec = vegalite::parse(json, true).expect("one-axis tick should parse");
        let scene = layout::build_scene_checked(&spec, &measurer).expect("one-axis tick renders");
        let ticks = red_ticks(&scene);
        assert_eq!(ticks.len(), 2);
        if use_x_axis {
            let plot_mid_y = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (x1 - x2).abs() < f64::EPSILON && (y2 - y1).abs() > 1.0 =>
                    {
                        Some((y1 + y2) / 2.0)
                    }
                    _ => None,
                })
                .expect("x axis grid line defines the plot's vertical midpoint");
            assert!(
                ticks
                    .iter()
                    .all(|tick| (tick.1 + tick.3 / 2.0 - plot_mid_y).abs() < 1e-6)
            );
        } else {
            let plot_mid_x = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (y1 - y2).abs() < f64::EPSILON && (x2 - x1).abs() > 1.0 =>
                    {
                        Some((x1 + x2) / 2.0)
                    }
                    _ => None,
                })
                .expect("y axis grid line defines the plot's horizontal midpoint");
            assert!(
                ticks
                    .iter()
                    .all(|tick| (tick.0 + tick.2 / 2.0 - plot_mid_x).abs() < 1e-6)
            );
        }
    }
}

#[test]
fn tick_config_controls_default_band_size_and_thickness() {
    let spec = vegalite::parse(
        r##"{
          "width":240,"height":180,
          "config":{"tick":{"bandSize":8,"thickness":2}},
          "mark":{"type":"tick","color":"red"},
          "data":{"values":[{"x":2,"group":"A"}]},
          "encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"group","type":"nominal"}}
        }"##,
        true,
    )
    .expect("configured tick should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("configured tick should render");
    let ticks = red_ticks(&scene);

    assert_eq!(ticks.len(), 1);
    assert_eq!((ticks[0].2, ticks[0].3), (8.0, 2.0));
}

#[test]
fn default_band_size_uses_three_quarters_of_a_multi_category_step() {
    let spec = vegalite::parse(
        r##"{
          "width":240,"height":180,
          "mark":{"type":"tick","orient":"horizontal","color":"red"},
          "data":{"values":[{"group":"A","value":2},{"group":"B","value":8}]},
          "encoding":{"x":{"field":"group","type":"nominal"},"y":{"field":"value","type":"quantitative"}}
        }"##,
        true,
    )
    .expect("categorical tick positions should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("categorical tick positions should render");
    let ticks = red_ticks(&scene);
    let plot_width = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Line { x1, x2, y1, y2, .. }
                if (y1 - y2).abs() < f64::EPSILON && (x2 - x1).abs() > 1.0 =>
            {
                Some((x2 - x1).abs())
            }
            _ => None,
        })
        .fold(0.0, f64::max);

    assert_eq!(ticks.len(), 2);
    let expected_width = 0.75 * plot_width / 2.0;
    assert!(
        ticks
            .iter()
            .all(|tick| (tick.2 - expected_width).abs() < 1e-6)
    );
    assert!(ticks.iter().all(|tick| tick.3 == 1.0));
}

#[test]
fn field_size_color_and_opacity_are_resolved_per_tick() {
    let spec = vegalite::parse(
        r##"{
          "width":240,"height":180,
          "mark":{"type":"tick","orient":"horizontal"},
          "data":{"values":[
            {"x":1,"group":"A","kind":"small","amount":2,"opacity":0},
            {"x":2,"group":"A","kind":"large","amount":10,"opacity":1}
          ]},
          "encoding":{
            "x":{"field":"x","type":"quantitative"},
            "y":{"field":"group","type":"nominal"},
            "color":{"field":"kind","type":"nominal"},
            "size":{"field":"amount","type":"quantitative"},
            "opacity":{"field":"opacity","type":"quantitative"}
          }
        }"##,
        true,
    )
    .expect("field-driven tick styles should parse");
    let scene = layout::build_scene_checked(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
        .expect("field-driven tick styles should render");
    let ticks = rects(&scene)
        .into_iter()
        .filter(|tick| tick.3 == 1.0)
        .collect::<Vec<_>>();

    assert_eq!(ticks.len(), 2);
    assert!(
        ticks[0].2 < ticks[1].2,
        "size encoding should affect tick length"
    );
    assert!((ticks[0].2 - 5.4).abs() < 1e-6);
    assert_eq!(ticks[1].2, 19.0);
    assert_ne!(
        ticks[0].4, ticks[1].4,
        "color field should select separate colors"
    );
    assert_ne!(
        ticks[0].4.a, ticks[1].4.a,
        "opacity field should affect alpha"
    );
    assert!((ticks[0].4.a - 0.3).abs() < f32::EPSILON);
    assert!((ticks[1].4.a - 0.8).abs() < f32::EPSILON);
}
