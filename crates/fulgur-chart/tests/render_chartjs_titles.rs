use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::build_scene;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;

fn render_scene(spec: &fulgur_chart::ir::ChartSpec) -> Scene {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    build_scene(spec, &measurer)
}

#[test]
fn chartjs_title_scene_preserves_original_scene_when_plugins_are_hidden() {
    let without_plugins = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
        false,
    )
    .unwrap();
    let hidden_plugins = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"options":{"plugins":{"title":{"display":false,"text":"Hidden"},"subtitle":{"display":false,"text":"Also hidden"}}}}"#,
        false,
    )
    .unwrap();

    assert_eq!(
        render_scene(&hidden_plugins),
        render_scene(&without_plugins)
    );
    assert_eq!(
        render_chart(&hidden_plugins),
        render_chart(&without_plugins)
    );
}

#[test]
fn legacy_chart_title_still_renders_without_chartjs_title_plugins() {
    let mut spec = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
        false,
    )
    .unwrap();
    spec.title = Some("Legacy title".into());
    assert!(spec.chartjs_title.is_none());
    assert!(spec.chartjs_subtitle.is_none());

    let scene = render_scene(&spec);
    assert!(
        !scene
            .items
            .iter()
            .any(|item| matches!(item, Prim::Group { .. }))
    );
    assert!(render_chart(&spec).contains("Legacy title"));
}
