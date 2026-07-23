use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::{chartjs, vegalite};
use fulgur_chart::ir::{SizeMode, XPositions};
use fulgur_chart::layout::common;
use fulgur_chart::model::build_model;
use fulgur_chart::temporal::parse_rfc3339_millis;
use fulgur_chart::text::TextMeasurer;

fn model_yaml(name: &str) -> fulgur_chart::model::ChartModel {
    let path = format!(
        "{}/../../examples/specs/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let json = std::fs::read_to_string(path).unwrap();
    let spec = chartjs::parse(&json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    build_model(&spec, &m)
}

#[test]
fn snapshot_bar_model() {
    insta::assert_yaml_snapshot!(model_yaml("bar"));
}

#[test]
fn snapshot_pie_model() {
    insta::assert_yaml_snapshot!(model_yaml("pie"));
}

#[test]
fn snapshot_line_model() {
    insta::assert_yaml_snapshot!(model_yaml("line"));
}

#[test]
fn snapshot_scatter_model() {
    insta::assert_yaml_snapshot!(model_yaml("scatter"));
}

#[test]
fn snapshot_bar_horizontal_model() {
    insta::assert_yaml_snapshot!(model_yaml("bar-horizontal"));
}

#[test]
fn plot_area_categorical_line_model_uses_scene_dimensions() {
    let json = r#"{
        "type":"line",
        "data":{"labels":["A","B","C"],"datasets":[{"data":[1,2,3]}]}
    }"#;
    let mut spec = chartjs::parse(json, false).unwrap();
    spec.size_mode = SizeMode::PlotArea;
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &m);
    let model = build_model(&spec, &m);
    let geometry = model.geometry.as_ref().unwrap();
    let plot_width = frame.plot_right - frame.plot_left;
    let plot_height = frame.plot_bottom - frame.plot_top;

    assert!(matches!(spec.x_positions, XPositions::Category));
    assert_eq!(
        (model.meta.width, model.meta.height),
        (frame.scene_width, frame.scene_height)
    );
    assert_eq!(geometry.plot_area.x, frame.plot_left / frame.scene_width);
    assert_eq!(geometry.plot_area.y, frame.plot_top / frame.scene_height);
    assert_eq!(geometry.plot_area.w, plot_width / frame.scene_width);
    assert_eq!(geometry.plot_area.h, plot_height / frame.scene_height);
}

#[test]
fn temporal_line_model_uses_scene_dimensions_and_temporal_axis() {
    let json = include_str!("fixtures/vegalite-temporal-line.json");
    let spec = vegalite::parse(json, true).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &m);
    let model = build_model(&spec, &m);
    let axes = model.axes.as_ref().unwrap();
    let geometry = model.geometry.as_ref().unwrap();
    let XPositions::Temporal { unix_millis } = &spec.x_positions else {
        panic!("fixture must use temporal positions");
    };

    assert_eq!(
        (model.meta.width, model.meta.height),
        (frame.scene_width, frame.scene_height)
    );
    assert_eq!(axes.x.kind, "temporal");
    assert_eq!(
        axes.x.labels.as_ref().unwrap(),
        &frame
            .temporal_ticks
            .iter()
            .map(|tick| tick.label.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        (axes.x.min, axes.x.max),
        (
            unix_millis.first().map(|value| *value as f64),
            unix_millis.last().map(|value| *value as f64)
        )
    );
    assert_eq!(
        axes.x.step,
        frame
            .temporal_ticks
            .windows(2)
            .next()
            .map(|window| (window[1].unix_millis - window[0].unix_millis) as f64)
    );
    assert_eq!(
        axes.x.ticks.as_ref().unwrap(),
        &frame
            .temporal_ticks
            .iter()
            .map(|tick| tick.unix_millis as f64)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        model.counts.x_ticks,
        frame.temporal_ticks.len(),
        "model tick count must describe rendered temporal ticks"
    );
    assert_eq!(geometry.plot_area.w, 720.0 / frame.scene_width);
    assert_eq!(geometry.plot_area.h, 320.0 / frame.scene_height);
    let x_positions = geometry
        .elements
        .iter()
        .take(3)
        .map(|element| element.nx)
        .collect::<Vec<_>>();
    assert_eq!((x_positions[0], x_positions[2]), (0.0, 1.0));
    assert!((x_positions[1] - 1.0 / 3.0).abs() < 1e-12);
    assert_eq!(
        unix_millis,
        &[
            parse_rfc3339_millis("timestamp", "2026-06-29T00:00:00Z").unwrap(),
            parse_rfc3339_millis("timestamp", "2026-07-01T00:00:00Z").unwrap(),
            parse_rfc3339_millis("timestamp", "2026-07-05T00:00:00Z").unwrap()
        ]
    );
}

#[test]
fn temporal_line_model_omits_explicitly_hidden_point_markers() {
    let json = include_str!("fixtures/vegalite-temporal-line.json")
        .replace("\"point\": true", "\"point\": false");
    let spec = vegalite::parse(&json, true).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let model = build_model(&spec, &m);

    assert!(
        model.geometry.unwrap().elements.is_empty(),
        "model geometry must not report markers omitted by the renderer"
    );
}
