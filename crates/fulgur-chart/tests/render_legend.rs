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
fn legend_visual_options_change_alignment_markers_and_text_style() {
    let json = r##"{"type":"line","data":{"labels":["x"],"datasets":[
      {"label":"First","data":[1]},{"label":"Second","data":[2]}]},
      "options":{"plugins":{"legend":{"align":"end","reverse":true,
        "labels":{"color":"#123456","font":{"size":15,"family":"Fira Sans","weight":600,"style":"italic"},
          "padding":8,"boxWidth":22,"boxHeight":14,"usePointStyle":true,"pointStyle":"triangle"},
        "title":{"display":true,"text":"Keys","color":"#abcdef",
          "font":{"size":18,"family":"Fira Mono","weight":"bold"},"padding":{"top":2,"bottom":4}}
      }}}}"##;
    let spec = chartjs::parse(json, true).unwrap();
    let svg = render_chart(&spec);
    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let scene = build_scene(&spec, &measurer);

    assert!(svg.contains(
        "font-family=\"Fira Sans\" font-size=\"15\" font-weight=\"600\" font-style=\"italic\""
    ));
    assert!(svg.contains("font-family=\"Fira Mono\" font-size=\"18\" font-weight=\"bold\""));
    assert!(svg.contains("fill=\"#123456\""));
    assert!(svg.contains("fill=\"#abcdef\""));

    let labels: Vec<(usize, f64, fulgur_chart::ir::Color)> = scene
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            Prim::StyledText {
                x, fill, content, ..
            } if content == "First" || content == "Second" => Some((index, *x, *fill)),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), 2);
    assert_eq!(
        labels[0].2,
        fulgur_chart::color::parse_color("#123456").unwrap()
    );
    assert!(
        labels[0].1 < labels[1].1,
        "reverse keeps dataset order reversed"
    );
    assert!(matches!(scene.items[labels[0].0 - 1], Prim::Path { .. }));
    assert!(scene.items.iter().any(|item| matches!(item, Prim::StyledText {
        content, fill, size, ..
    } if content == "Keys" && *fill == fulgur_chart::color::parse_color("#abcdef").unwrap() && *size == 18.0)));
}

#[test]
fn vertical_legend_options_apply_order_row_spacing_and_title() {
    let json = r##"{"type":"radar","data":{"labels":["x","y"],"datasets":[
      {"label":"First","data":[1,2]},{"label":"Second","data":[2,1]}]},
      "options":{"plugins":{"legend":{"position":"right","align":"end","reverse":true,
        "labels":{"color":"#223344","font":{"size":15},"padding":8,
          "boxWidth":22,"boxHeight":14,"usePointStyle":true,"pointStyle":"rectRot"},
        "title":{"display":true,"text":"Series"}
      }}}}"##;
    let spec = chartjs::parse(json, true).unwrap();
    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let scene = build_scene(&spec, &measurer);

    let labels: Vec<(usize, f64, f64)> = scene
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            Prim::Text { y, content, .. } if content == "First" || content == "Second" => {
                Some((index, 0.0, *y))
            }
            Prim::StyledText { y, content, .. } if content == "First" || content == "Second" => {
                Some((index, 0.0, *y))
            }
            Prim::Text { x, y, content, .. } if content == "Series" => Some((index, *x, *y)),
            Prim::StyledText { x, y, content, .. } if content == "Series" => Some((index, *x, *y)),
            _ => None,
        })
        .collect();
    let series_labels: Vec<(usize, f64)> = labels
        .iter()
        .filter_map(|(index, _, y)| {
            let content = match &scene.items[*index] {
                Prim::Text { content, .. } | Prim::StyledText { content, .. } => content,
                _ => return None,
            };
            (content == "First" || content == "Second").then_some((*index, *y))
        })
        .collect();
    assert_eq!(series_labels.len(), 2);
    assert!(
        series_labels[0].1 < series_labels[1].1,
        "reverse controls vertical row order"
    );
    assert!(matches!(
        scene.items[series_labels[0].0 - 1],
        Prim::Path { .. }
    ));
    let title_y = labels
        .iter()
        .find_map(|(index, _, y)| match &scene.items[*index] {
            Prim::Text { content, .. } | Prim::StyledText { content, .. }
                if content == "Series" =>
            {
                Some(*y)
            }
            _ => None,
        })
        .unwrap();
    assert!(title_y < series_labels[0].1);
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
