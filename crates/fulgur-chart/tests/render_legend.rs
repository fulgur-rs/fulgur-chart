use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::build_scene;
use fulgur_chart::layout::common::{OUTER_PAD, legend_band_width_vertical};
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

fn assert_right_legend_entries(json: &str, labels: &[&str]) {
    let spec = chartjs::parse(json, false).unwrap();
    let svg = render_chart(&spec);
    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let scene = build_scene(&spec, &measurer);
    let legend_names: Vec<String> = labels.iter().map(|label| (*label).to_string()).collect();
    let plot_right = scene.width
        - OUTER_PAD
        - legend_band_width_vertical(&measurer, &legend_names, spec.theme.font_size);

    for label in labels {
        assert!(
            svg.contains(&format!(">{label}</text>")),
            "legend label {label:?} is missing from SVG"
        );
        let (label_index, label_x) = scene
            .items
            .iter()
            .enumerate()
            .find_map(|(index, item)| match item {
                Prim::Text { x, content, .. } if content == label => Some((index, *x)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("legend label {label:?} is missing from scene"));
        let Prim::Rect {
            x: swatch_x,
            w: swatch_w,
            ..
        } = scene
            .items
            .get(
                label_index
                    .checked_sub(1)
                    .expect("legend label has a swatch"),
            )
            .expect("legend label has a preceding swatch")
        else {
            panic!("legend label {label:?} is not preceded by a swatch");
        };
        assert!(
            *swatch_x >= plot_right,
            "legend swatch for {label:?} must start at or right of the plot edge"
        );
        assert!(
            label_x > plot_right,
            "legend label {label:?} must be right of the plot edge"
        );
        assert!(
            *swatch_x + *swatch_w <= label_x,
            "legend swatch for {label:?} must be left of its label"
        );
    }
}

#[test]
fn bar_legend_left_renders_series_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x","y"],
      "datasets":[{"label":"売上","data":[1,2]},{"label":"原価","data":[1,1]}]},
      "options":{"plugins":{"legend":{"position":"left"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">売上</text>"));
    assert!(svg.contains(">原価</text>"));
    // determinism
    assert_eq!(svg, render(json));
}

#[test]
fn bar_legend_right_renders_series_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x"],
      "datasets":[{"label":"売上","data":[1]},{"label":"原価","data":[2]}]},
      "options":{"plugins":{"legend":{"position":"right"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">売上</text>"));
    assert!(svg.contains(">原価</text>"));
}

#[test]
fn horizontal_bar_legend_right_renders_series_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x"],
      "datasets":[{"label":"売上","data":[1]},{"label":"原価","data":[2]}]},
      "options":{"indexAxis":"y","plugins":{"legend":{"position":"right"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">売上</text>"));
    assert!(svg.contains(">原価</text>"));
}

#[test]
fn legend_display_false_no_labels() {
    let json = r#"{"type":"bar","data":{"labels":["x"],
      "datasets":[{"label":"売上","data":[1]}]},
      "options":{"plugins":{"legend":{"display":false,"position":"left"}}}}"#;
    assert!(!render(json).contains(">売上</text>"));
}

#[test]
fn left_differs_from_top() {
    let base = r#"{"type":"bar","data":{"labels":["x"],"datasets":[{"label":"売上","data":[1]}]},"options":{"plugins":{"legend":{"position":"POS"}}}}"#;
    let left = render(&base.replace("POS", "left"));
    let top = render(&base.replace("POS", "top"));
    assert_ne!(left, top, "left 凡例は帯確保で top と出力が異なるはず");
}

#[test]
fn pie_legend_right_renders_category_labels() {
    let json = r#"{"type":"pie","data":{"labels":["りんご","みかん"],"datasets":[{"data":[3,1]}]},
      "options":{"plugins":{"legend":{"position":"right"}}}}"#;
    let svg = render(json);
    assert!(svg.contains(">りんご</text>"));
    assert!(svg.contains(">みかん</text>"));
}

#[test]
fn right_legends_render_for_scatter_radar_and_polar_area() {
    let specs = [
        (
            r#"{"type":"scatter","data":{"datasets":[{"label":"scatter-series","data":[{"x":1,"y":2}]}]},"options":{"plugins":{"legend":{"position":"right"}}}}"#,
            &["scatter-series"][..],
        ),
        (
            r#"{"type":"radar","data":{"labels":["a","b"],"datasets":[{"label":"radar-series","data":[1,2]}]},"options":{"plugins":{"legend":{"position":"right"}}}}"#,
            &["radar-series"][..],
        ),
        (
            r#"{"type":"polarArea","data":{"labels":["polar-a","polar-b"],"datasets":[{"data":[1,2]}]},"options":{"plugins":{"legend":{"position":"right"}}}}"#,
            &["polar-a", "polar-b"][..],
        ),
    ];
    for (spec, labels) in specs {
        assert_right_legend_entries(spec, labels);
    }
}
