use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::guard::{InputLimits, validate_spec, validate_spec_with_measurer};
use fulgur_chart::ir::ChartSpec;
use fulgur_chart::layout::{
    common::{self, Frame},
    line,
};
use fulgur_chart::num::fmt_num;
use fulgur_chart::palette::VEGALITE_PALETTE;
use fulgur_chart::raster_direct::{render_chart_to_png, render_chart_to_webp};
use fulgur_chart::render::{render_chart, render_chart_with_font};
use fulgur_chart::scene::{Anchor, Prim, Scene};
use fulgur_chart::text::TextMeasurer;

fn fixture() -> &'static str {
    include_str!("fixtures/vegalite-temporal-line.json")
}

fn parsed() -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(fixture(), true).unwrap()
}

fn measurer() -> TextMeasurer<'static> {
    TextMeasurer::new(DEFAULT_FONT).unwrap()
}

fn assert_plot_area_right_legend_contained(
    scene: &Scene,
    frame: &Frame,
    spec: &ChartSpec,
    m: &TextMeasurer,
) {
    let legend_swatch_bounds = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Rect { x, w, h, .. } if (*w, *h) == (12.0, 12.0) => Some((*x, *x + *w)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(legend_swatch_bounds.len(), spec.series.len());
    assert!(
        legend_swatch_bounds
            .iter()
            .all(|(x, right)| *x >= frame.plot_right && *right <= scene.width)
    );

    let legend_text_bounds = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Text {
                x,
                size,
                content,
                anchor: Anchor::Start,
                ..
            } if content == "metric"
                || spec.series.iter().any(|series| series.name == *content) =>
            {
                Some((*x, *x + m.width(content, *size as f32) as f64))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(legend_text_bounds.len(), spec.series.len() + 1);
    assert!(
        legend_text_bounds
            .iter()
            .all(|(x, right)| *x >= frame.plot_right && *right <= scene.width)
    );
}

#[test]
fn dogfood_fixture_renders_in_strict_and_non_strict_modes() {
    for strict in [false, true] {
        let spec = vegalite::parse(fixture(), strict).unwrap();
        validate_spec(&spec, &InputLimits::default()).unwrap();
        let svg = render_chart(&spec);
        assert!(svg.contains("qtest nightly trend"));
        assert!(svg.contains(">date</text>"));
        assert!(svg.contains(">subtests</text>"));
        assert!(svg.contains(">metric</text>"));
        assert!(svg.contains("stroke-opacity=\"0.15\""));
        assert!(svg.contains("<path"));
        assert!(svg.contains(" C "));
    }
}

#[test]
fn grid_opacity_does_not_fade_temporal_tick_marks() {
    let json = fixture().replace(
        r#""grid": true, "gridOpacity": 0.15"#,
        r#""grid": false, "gridOpacity": 0"#,
    );
    let spec = vegalite::parse(&json, true).unwrap();
    let frame = common::compute(&spec, &measurer());
    let scene = line::build(&spec, &measurer());
    let tick_strokes = scene.items.iter().filter_map(|item| match item {
        Prim::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            ..
        } if ((*x2 - frame.plot_left).abs() < 0.01
            && (*x1 - (frame.plot_left - 4.0)).abs() < 0.01
            && (*y1 - *y2).abs() < 0.01)
            || ((*y1 - frame.plot_bottom).abs() < 0.01
                && (*y2 - (frame.plot_bottom + 4.0)).abs() < 0.01
                && (*x1 - *x2).abs() < 0.01) =>
        {
            Some(*stroke)
        }
        _ => None,
    });
    let tick_strokes = tick_strokes.collect::<Vec<_>>();

    assert!(!tick_strokes.is_empty());
    assert!(
        tick_strokes.iter().all(|stroke| stroke.a == 1.0),
        "gridOpacity must affect grid lines only: {tick_strokes:?}"
    );
}

#[test]
fn plot_area_outer_scene_must_fit_dimension_limit() {
    let spec = parsed();
    let limits = InputLimits {
        max_dimension_px: 740.0,
        ..InputLimits::default()
    };

    let err = validate_spec(&spec, &limits).unwrap_err();
    assert!(err.contains("scene width"), "unexpected error: {err}");
}

#[test]
fn plot_area_outer_scene_height_must_fit_dimension_limit() {
    let mut spec = parsed();
    spec.width = 100.0;
    spec.height = 720.0;
    let limits = InputLimits {
        max_dimension_px: 740.0,
        ..InputLimits::default()
    };

    let err = validate_spec(&spec, &limits).unwrap_err();
    assert!(err.contains("scene height"), "unexpected error: {err}");
}

#[test]
fn plot_area_contains_long_rotated_y_axis_title() {
    const Y_TITLE: &str = "a very long quantitative y axis title";
    let json = fixture()
        .replace(r#""height": 320"#, r#""height": 24"#)
        .replace(
            r#""title": "subtests""#,
            &format!(r#""title": "{Y_TITLE}""#),
        );
    let spec = vegalite::parse(&json, true).unwrap();
    let m = measurer();
    let scene = line::build(&spec, &m);
    let (y, size) = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Text {
                y,
                size,
                content,
                rotate_deg: Some(-90.0),
                ..
            } if content == Y_TITLE => Some((*y, *size)),
            _ => None,
        })
        .expect("rotated y-axis title");
    let half_extent = m.width(Y_TITLE, size as f32) as f64 / 2.0;

    assert!(
        y - half_extent >= 0.0,
        "rotated title must fit above: y={y}, half_extent={half_extent}"
    );
    assert!(
        y + half_extent <= scene.height,
        "rotated title must fit below: y={y}, half_extent={half_extent}, scene={}",
        scene.height
    );
}

#[test]
fn plot_area_contains_long_centered_x_axis_title() {
    const X_TITLE: &str = "a very long centered temporal x axis title";
    let json = fixture()
        .replace(r#""width": 720"#, r#""width": 24"#)
        .replace(r#""title": "date""#, &format!(r#""title": "{X_TITLE}""#));
    let spec = vegalite::parse(&json, true).unwrap();
    let m = measurer();
    let frame = common::compute(&spec, &m);
    let scene = line::build(&spec, &m);
    let (x, size) = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Text {
                x,
                size,
                content,
                rotate_deg: None,
                anchor: Anchor::Middle,
                ..
            } if content == X_TITLE => Some((*x, *size)),
            _ => None,
        })
        .expect("centered x-axis title");
    let half_extent = m.width(X_TITLE, size as f32) as f64 / 2.0;

    assert!(x - half_extent >= 0.0);
    assert!(x + half_extent <= scene.width);
    assert_eq!(frame.plot_right - frame.plot_left, spec.width);
    assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);

    assert_plot_area_right_legend_contained(&scene, &frame, &spec, &m);
}

#[test]
fn plot_area_contains_long_centered_chart_title() {
    const CHART_TITLE: &str = "a very long unique centered top level chart title";
    let json = fixture()
        .replace(r#""width": 720"#, r#""width": 24"#)
        .replace(
            r#""title": "qtest nightly trend""#,
            &format!(r#""title": "{CHART_TITLE}""#),
        );
    let spec = vegalite::parse(&json, true).unwrap();
    let m = measurer();
    let frame = common::compute(&spec, &m);
    let scene = line::build(&spec, &m);
    let (x, size) = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Text {
                x,
                size,
                content,
                rotate_deg: None,
                anchor: Anchor::Middle,
                ..
            } if content == CHART_TITLE => Some((*x, *size)),
            _ => None,
        })
        .next()
        .expect("centered chart title");
    let half_extent = m.width(CHART_TITLE, size as f32) as f64 / 2.0;

    assert!(x - half_extent >= 0.0);
    assert!(x + half_extent <= scene.width);
    assert_eq!(frame.plot_right - frame.plot_left, spec.width);
    assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);

    assert_plot_area_right_legend_contained(&scene, &frame, &spec, &m);
}

#[test]
fn plot_area_outer_scene_can_be_validated_with_render_measurer() {
    let spec = parsed();
    let limits = InputLimits {
        max_dimension_px: 740.0,
        ..InputLimits::default()
    };
    let render_measurer = measurer();

    let err = validate_spec_with_measurer(&spec, &limits, &render_measurer).unwrap_err();
    assert!(err.contains("scene width"), "unexpected error: {err}");
}

#[test]
fn custom_font_render_paths_reject_oversized_plot_area_scene() {
    let mut spec = parsed();
    spec.width = InputLimits::default().max_dimension_px;

    for result in [
        render_chart_with_font(&spec, DEFAULT_FONT).map(|_| ()),
        render_chart_to_png(&spec, 1.0, DEFAULT_FONT).map(|_| ()),
        render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).map(|_| ()),
    ] {
        let err = result.unwrap_err();
        assert!(err.contains("scene width"), "unexpected error: {err}");
    }
}

#[test]
fn dogfood_fixture_preserves_series_values_and_tableau_order() {
    let spec = parsed();
    assert_eq!(
        spec.series
            .iter()
            .map(|series| series.name.as_str())
            .collect::<Vec<_>>(),
        ["allowlist", "candidates", "regressions"]
    );
    assert_eq!(
        spec.series
            .iter()
            .map(|series| series.values.as_slice())
            .collect::<Vec<_>>(),
        [&[0.0, 0.0, 5.0][..], &[2.0, 3.0, 6.0], &[1.0, 1.0, 4.0]]
    );
    assert_eq!(
        spec.series
            .iter()
            .map(|series| series.stroke[0])
            .collect::<Vec<_>>(),
        VEGALITE_PALETTE[..3]
    );
}

#[test]
fn empty_string_color_groups_keep_temporal_legend_and_model_items() {
    let all_empty_json = r#"{
        "mark":"line",
        "data":{"values":[
            {"timestamp":"2026-07-01T00:00:00Z","metric":"","value":1},
            {"timestamp":"2026-07-02T00:00:00Z","metric":"","value":2}
        ]},
        "encoding":{
            "x":{"field":"timestamp","type":"temporal"},
            "y":{"field":"value","type":"quantitative"},
            "color":{"field":"metric","type":"nominal"}
        }
    }"#;
    let all_empty = vegalite::parse(all_empty_json, true).unwrap();
    let all_empty_frame = common::compute(&all_empty, &measurer());
    let all_empty_scene = line::build(&all_empty, &measurer());
    let legend_swatches = all_empty_scene
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                Prim::Rect { x, .. } if *x > all_empty_frame.plot_right
            )
        })
        .count();
    let all_empty_model = fulgur_chart::model::build_model_core(&all_empty);

    assert_eq!(legend_swatches, 1);
    assert_eq!(all_empty_model.counts.legend_items, 1);

    let mixed_json = all_empty_json
        .replace(
            r#"{"timestamp":"2026-07-01T00:00:00Z","metric":"","value":1},"#,
            r#"{"timestamp":"2026-07-01T00:00:00Z","metric":"","value":1},
            {"timestamp":"2026-07-01T00:00:00Z","metric":"named","value":3},"#,
        )
        .replace(
            r#"{"timestamp":"2026-07-02T00:00:00Z","metric":"","value":2}"#,
            r#"{"timestamp":"2026-07-02T00:00:00Z","metric":"","value":2},
            {"timestamp":"2026-07-02T00:00:00Z","metric":"named","value":4}"#,
        );
    let mixed = vegalite::parse(&mixed_json, true).unwrap();
    let mixed_model = fulgur_chart::model::build_model_core(&mixed);

    assert_eq!(mixed.series.len(), 2);
    assert_eq!(mixed_model.counts.legend_items, 2);
}

#[test]
fn dogfood_fixture_uses_elapsed_time_for_line_geometry() {
    let spec = parsed();
    let frame = common::compute(&spec, &measurer());
    let points = line::line_points(&spec, &frame);
    let first_gap = points[1].cx - points[0].cx;
    let second_gap = points[2].cx - points[1].cx;

    assert_eq!(frame.plot_right - frame.plot_left, 720.0);
    assert_eq!(frame.plot_bottom - frame.plot_top, 320.0);
    assert!((second_gap / first_gap - 2.0).abs() < 1e-12);
}

#[test]
fn dogfood_fixture_expands_canvas_and_dispatches_monotone_paths() {
    let spec = parsed();
    let m = measurer();
    let frame = common::compute(&spec, &m);
    let scene = line::build(&spec, &m);
    let svg = render_chart(&spec);

    assert_eq!(
        (scene.width, scene.height),
        (frame.scene_width, frame.scene_height)
    );
    assert!(scene.width > 720.0);
    assert!(scene.height > 320.0);
    assert!(svg.starts_with(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"",
        fmt_num(frame.scene_width),
        fmt_num(frame.scene_height)
    )));
    assert_eq!(svg.matches("<circle").count(), 9);
    assert_eq!(
        scene
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Path { d, .. } if d.contains(" C ")))
            .count(),
        3
    );
}

#[test]
fn dogfood_fixture_is_deterministic_and_decodes_as_png() {
    let spec = parsed();
    let first = render_chart(&spec);
    let second = render_chart(&spec);
    assert_eq!(first, second);

    let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("dogfood PNG must decode");
    let frame = common::compute(&spec, &measurer());
    assert_eq!(
        (pixmap.width(), pixmap.height()),
        (
            frame.scene_width.ceil() as u32,
            frame.scene_height.ceil() as u32
        )
    );
}

#[test]
fn dogfood_fixture_svg_snapshot() {
    insta::assert_snapshot!(render_chart(&parsed()));
}
