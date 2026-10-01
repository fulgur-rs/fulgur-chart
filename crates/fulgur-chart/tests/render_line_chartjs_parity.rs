use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::line;
use fulgur_chart::model::build_model;
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;
use serde_json::{Value, json};

fn dense_line(dataset: Value, options: Value) -> fulgur_chart::ir::ChartSpec {
    let mut dataset = dataset;
    dataset["data"] = json!((0..5_000).map(|i| i % 73).collect::<Vec<_>>());
    chartjs::parse(
        &json!({
            "type": "line", "width": 300, "height": 220,
            "data": {"labels": vec![""; 5_000], "datasets": [dataset]},
            "options": options
        })
        .to_string(),
        true,
    )
    .unwrap()
}

#[test]
fn chartjs_dense_line_retains_all_markers_and_inspect_elements() {
    let spec = dense_line(
        json!({}),
        json!({"plugins": {"legend": {"display": false}}}),
    );
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &measurer);
    let markers = scene
        .items
        .iter()
        .filter(|item| matches!(item, Prim::Circle { r: 3.0, .. }))
        .count();
    assert_eq!(
        markers, 5_000,
        "Chart.js does not suppress dense-line markers"
    );
    let geometry = build_model(&spec, &measurer).geometry.unwrap();
    assert_eq!(geometry.elements.len(), 5_000);
    assert_eq!(geometry.elements.first().unwrap().index, 0);
    assert_eq!(geometry.elements.last().unwrap().index, 4_999);
    let drawn_points: usize = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Polyline { points, .. } => Some(points.len()),
            _ => None,
        })
        .sum();
    assert!(
        drawn_points < 2_000,
        "only the line path should be simplified: {drawn_points}"
    );
}

#[test]
fn chartjs_category_decimation_is_noop_even_when_explicitly_enabled() {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let baseline = dense_line(json!({}), json!({}));
    let baseline_scene =
        fulgur_chart::svg::render_svg(&line::build(&baseline, &measurer), "sans-serif");
    for decimation in [
        json!({}),
        json!({"threshold": 1}),
        json!({"enabled": true, "threshold": 1}),
        json!({"enabled": true, "algorithm": "lttb", "samples": 3, "threshold": 1}),
        json!({"enabled": false}),
    ] {
        let spec = dense_line(json!({}), json!({"plugins": {"decimation": decimation}}));
        assert!(
            !spec.decimation.enabled,
            "category data is ineligible: {decimation}"
        );
        assert_eq!(
            fulgur_chart::svg::render_svg(&line::build(&spec, &measurer), "sans-serif"),
            baseline_scene
        );
        assert_eq!(
            build_model(&spec, &measurer)
                .geometry
                .unwrap()
                .elements
                .len(),
            5_000
        );
    }
}

#[test]
fn chartjs_point_radius_zero_hides_markers_but_preserves_inspect_elements() {
    let spec = dense_line(
        json!({"pointRadius": 0}),
        json!({"plugins": {"legend": {"display": false}}}),
    );
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &measurer);
    assert!(
        !scene
            .items
            .iter()
            .any(|item| matches!(item, Prim::Circle { .. }))
    );
    assert_eq!(
        build_model(&spec, &measurer)
            .geometry
            .unwrap()
            .elements
            .len(),
        5_000
    );
}

#[test]
fn chartjs_dash_and_step_paths_keep_all_source_points() {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    for dataset in [json!({"borderDash": [2, 2]}), json!({"stepped": true})] {
        let spec = dense_line(dataset.clone(), json!({}));
        let scene = line::build(&spec, &measurer);
        let drawn_points: usize = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Polyline { points, .. } | Prim::StyledPolyline { points, .. } => {
                    Some(points.len())
                }
                _ => None,
            })
            .sum();
        assert!(
            drawn_points >= 5_000,
            "fast path must not apply to {dataset}"
        );
        assert_eq!(
            build_model(&spec, &measurer)
                .geometry
                .unwrap()
                .elements
                .len(),
            5_000
        );
    }
}

#[test]
fn chartjs_default_and_explicit_point_radius_match_scene_marker_positions() {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    for dataset in [
        json!({}),
        json!({"pointRadius": 2}),
        json!({"showLine": false}),
    ] {
        let spec = dense_line(dataset, json!({"plugins": {"legend": {"display": false}}}));
        let scene = line::build(&spec, &measurer);
        let model = build_model(&spec, &measurer);
        let geometry = model.geometry.unwrap();
        let plot = geometry.plot_area;
        let markers: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
                _ => None,
            })
            .collect();
        assert_eq!(markers.len(), 5_000);
        for (element, (x, y)) in geometry.elements.iter().zip(markers) {
            assert!((element.nx - (x / model.meta.width - plot.x) / plot.w).abs() < 1e-12);
            assert!((element.ny - (y / model.meta.height - plot.y) / plot.h).abs() < 1e-12);
        }
    }
}

#[test]
fn chartjs_dense_area_upper_edge_matches_the_line_drawing_path() {
    let spec = dense_line(
        json!({"fill": true, "pointRadius": 0}),
        json!({"plugins": {"legend": {"display": false}}}),
    );
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &measurer);
    let points = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Polyline { points, .. } => Some(points),
            _ => None,
        })
        .unwrap();
    let prefix: String = points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            format!(
                "{} {} {} ",
                if i == 0 { "M" } else { "L" },
                fulgur_chart::num::fmt_num(*x),
                fulgur_chart::num::fmt_num(*y)
            )
        })
        .collect();
    let area = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Path {
                d,
                fill: Some(_),
                stroke: None,
                ..
            } => Some(d),
            _ => None,
        })
        .unwrap();
    assert!(
        area.starts_with(&prefix),
        "the area boundary must use the same drawing fast path"
    );
    assert_eq!(
        build_model(&spec, &measurer)
            .geometry
            .unwrap()
            .elements
            .len(),
        5_000
    );
}

#[test]
fn chartjs_mixed_line_simplifies_drawing_but_retains_every_marker() {
    let spec = chartjs::parse(
        &json!({
            "type": "bar", "width": 300, "height": 220,
            "data": {"labels": vec![""; 5_000], "datasets": [
                {"type": "bar", "data": [3]},
                {"type": "line", "data": (0..5_000).map(|i| i % 73).collect::<Vec<_>>()}
            ]}, "options": {"plugins": {"legend": {"display": false}}}
        })
        .to_string(),
        true,
    )
    .unwrap();
    let scene =
        fulgur_chart::layout::mixed::build(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let drawn_points: usize = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Polyline { points, .. } => Some(points.len()),
            _ => None,
        })
        .sum();
    assert!(
        drawn_points < 2_000,
        "mixed LineElements need the same fast path: {drawn_points}"
    );
    assert_eq!(
        scene
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Circle { .. }))
            .count(),
        5_000
    );
}
