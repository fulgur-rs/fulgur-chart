use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::{Decimation, DecimationAlgorithm, XPositions};
use fulgur_chart::layout::{self, common};
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;

fn example_spec(name: &str) -> fulgur_chart::ir::ChartSpec {
    let path = format!(
        "{}/../../examples/specs/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let json = std::fs::read_to_string(&path).expect("trail example should be readable");
    vegalite::parse(&json, true).expect("trail example should parse")
}

fn clipped_trails(
    scene: &fulgur_chart::scene::Scene,
) -> Vec<(&str, fulgur_chart::scene::ClipRect)> {
    scene
        .items
        .iter()
        .filter_map(|prim| match prim {
            Prim::ClippedPath {
                d,
                fill: Some(_),
                stroke: None,
                clip,
                ..
            } => Some((d.as_str(), **clip)),
            _ => None,
        })
        .collect()
}

#[test]
fn trail_scene_splits_gaps_and_clips_to_plot() {
    let mut spec = example_spec("vegalite-trail-temporal");
    assert!(matches!(spec.x_positions, XPositions::Temporal { .. }));
    for series in &mut spec.series {
        series.values[2] = f64::NAN;
    }

    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &measurer);
    let scene = layout::build_scene(&spec, &measurer);
    let trails = clipped_trails(&scene);

    assert_eq!(
        trails.len(),
        4,
        "each color group has two segments around its gap"
    );
    for (_, clip) in trails {
        assert_eq!(
            (clip.x, clip.y, clip.w, clip.h),
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right - frame.plot_left,
                frame.plot_bottom - frame.plot_top,
            )
        );
    }
    assert!(!scene.items.iter().any(|prim| matches!(
        prim,
        Prim::Polyline { .. } | Prim::StyledPolyline { .. } | Prim::Circle { .. }
    )));
}

#[test]
fn trail_decimation_keeps_widths_aligned() {
    let values = (0..1200)
        .map(|index| format!(r#"{{"x":"{index}","y":{},"size":{index}}}"#, index % 2))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{"mark":"trail","width":120,"height":120,"data":{{"values":[{values}]}},"encoding":{{"x":{{"field":"x"}},"y":{{"field":"y","type":"quantitative"}},"size":{{"field":"size"}}}}}}"#
    );
    let mut spec = vegalite::parse(&json, true).expect("large trail should parse");
    spec.decimation = Decimation {
        enabled: true,
        algorithm: DecimationAlgorithm::MinMax,
        samples: Some(3.0),
        threshold: Some(3.0),
    };
    assert_eq!(
        spec.series[0].trail_widths_slice().len(),
        spec.categories.len()
    );

    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let decimated_scene = layout::build_scene(&spec, &measurer);
    let decimated_path = clipped_trails(&decimated_scene)
        .into_iter()
        .next()
        .expect("decimated trail should render")
        .0
        .to_owned();
    let mut full_spec = spec.clone();
    full_spec.decimation.enabled = false;
    let full_scene = layout::build_scene(&full_spec, &measurer);
    let full_path = clipped_trails(&full_scene)
        .into_iter()
        .next()
        .expect("full trail should render")
        .0
        .to_owned();

    assert!(decimated_path.len() < full_path.len());
    assert!(!decimated_path.contains("NaN") && !decimated_path.contains("inf"));
}
