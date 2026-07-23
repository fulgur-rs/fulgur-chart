use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::guard::{InputLimits, validate_spec};
use fulgur_chart::layout::{common, line};
use fulgur_chart::num::fmt_num;
use fulgur_chart::palette::VEGALITE_PALETTE;
use fulgur_chart::raster_direct::render_chart_to_png;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::Prim;
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
