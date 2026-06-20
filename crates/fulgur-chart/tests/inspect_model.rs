use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::model::build_model;
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
