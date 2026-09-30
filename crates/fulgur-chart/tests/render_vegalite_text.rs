use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::{ChartKind, VegaTextAlign};
use fulgur_chart::layout;
use fulgur_chart::scene::{Anchor, Prim, StyledText};
use fulgur_chart::text::TextMeasurer;

fn styled_texts(scene: &fulgur_chart::scene::Scene) -> Vec<&StyledText> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::StyledText(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn vegalite_text_scene_maps_coordinates_and_styles_labels() {
    let json = r##"{
      "mark":{"type":"text","font":"serif","fontWeight":"bold","fontStyle":"italic"},
      "width":240,"height":180,
      "data":{"values":[
        {"x":0,"y":0,"label":"Zero","group":"a","size":10,"opacity":0},
        {"x":5,"y":5,"label":"Middle","group":"b","size":20,"opacity":0.5},
        {"x":10,"y":10,"label":"Ten","group":"a","size":30,"opacity":1},
        {"x":20,"y":20,"label":"Outside","group":"b","size":40,"opacity":1}
      ]},
      "encoding":{
        "x":{"field":"x","type":"quantitative"},
        "y":{"field":"y","type":"quantitative"},
        "text":{"field":"label"},
        "color":{"field":"group","type":"nominal"},
        "size":{"field":"size","type":"quantitative"},
        "opacity":{"field":"opacity","type":"quantitative"}
      }
    }"##;
    let mut spec = vegalite::parse(json, true).expect("text spec should parse");
    spec.x_axis.min = Some(0.0);
    spec.x_axis.max = Some(10.0);
    spec.y_axis.min = Some(0.0);
    spec.y_axis.max = Some(10.0);
    let ChartKind::VegaText(data) = &spec.kind else {
        panic!("VegaText expected")
    };
    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let frame = layout::scatter::compute_scatter_layout(&spec, &measurer);
    let scene = layout::build_scene(&spec, &measurer);
    let labels = styled_texts(&scene);

    assert_eq!(labels.len(), 3, "each input row should produce one label");
    assert_eq!(
        labels
            .iter()
            .map(|label| label.content.as_str())
            .collect::<Vec<_>>(),
        ["Zero", "Middle", "Ten"]
    );
    for (label, mark) in labels.iter().zip(&data.marks) {
        assert!((label.x - frame.xs.map(mark.point.x)).abs() < 1e-9);
        assert!((label.y - frame.ys.map(mark.point.y)).abs() < 1e-9);
        assert_eq!(label.size, mark.size);
        assert_eq!(label.fill, mark.fill);
        assert_eq!(label.font_family.as_deref(), Some("serif"));
        assert_eq!(label.font_weight.as_deref(), Some("bold"));
        assert_eq!(label.font_style.as_deref(), Some("italic"));
    }
    assert!(labels[0].size < labels[1].size && labels[1].size < labels[2].size);
    assert!(labels[0].fill.a < labels[1].fill.a && labels[1].fill.a < labels[2].fill.a);
    assert_ne!(
        labels[0].fill, labels[1].fill,
        "nominal color field should style rows"
    );

    let model = fulgur_chart::model::build_model(&spec, &measurer);
    assert_eq!(model.meta.r#type, "text");
    assert_eq!(model.counts.datasets, 1);
    assert_eq!(model.axes.as_ref().unwrap().x.kind, "linear");
    assert_eq!(model.geometry.as_ref().unwrap().elements.len(), 3);
}

#[test]
fn vegalite_text_scene_applies_baseline_and_offsets() {
    let json = r##"{
      "mark":{"type":"text","baseline":"top","align":"right","angle":30,"dx":6,"dy":-4,"fontSize":12},
      "width":240,"height":180,
      "data":{"values":[
        {"x":0,"y":0,"label":"Origin"},
        {"x":10,"y":10,"label":"Corner"}
      ]},
      "encoding":{
        "x":{"field":"x","type":"quantitative"},
        "y":{"field":"y","type":"quantitative"},
        "text":{"field":"label"}
      }
    }"##;
    let spec = vegalite::parse(json, true).expect("text spec should parse");
    let ChartKind::VegaText(data) = &spec.kind else {
        panic!("VegaText expected")
    };
    let measurer = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let frame = layout::scatter::compute_scatter_layout(&spec, &measurer);
    let scene = layout::build_scene(&spec, &measurer);
    let labels = styled_texts(&scene);

    assert_eq!(labels.len(), 2);
    let label = labels[0];
    assert!((label.x - (frame.xs.map(data.marks[0].point.x) + 6.0)).abs() < 1e-9);
    assert!((label.y - (frame.ys.map(data.marks[0].point.y) - 4.0)).abs() < 1e-9);
    assert_eq!(label.anchor, Anchor::End);
    assert_eq!(label.rotate_deg, Some(30.0));
    assert_eq!(label.baseline, fulgur_chart::ir::TextBaseline::Top);

    let svg = fulgur_chart::svg::render_svg(&scene, "sans-serif");
    assert!(
        svg.contains("dominant-baseline=\"text-before-edge\""),
        "top baseline should be explicit in SVG: {svg}"
    );
    assert_eq!(data.marks[0].align, VegaTextAlign::Right);
}

#[test]
fn raster_rejects_text_font_family_that_the_selected_font_cannot_supply() {
    let json = r#"{"mark":{"type":"text","font":"serif"},"data":{"values":[{"x":0,"y":0,"label":"A"}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"},"text":{"field":"label"}}}"#;
    let spec = vegalite::parse(json, true).expect("text font parses for SVG output");
    let png_error = fulgur_chart::raster_direct::render_chart_to_png_default(&spec, 1.0)
        .expect_err("raster must not silently ignore an unavailable mark font family");
    assert!(png_error.contains("font family"), "{png_error}");
    let webp_error = fulgur_chart::raster_direct::render_chart_to_webp(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .expect_err("WebP must not silently ignore an unavailable mark font family");
    assert!(webp_error.contains("font family"), "{webp_error}");

    let composed = vegalite::parse(
        r#"{"layer":[{"mark":{"type":"text","font":"serif"},"data":{"values":[{"x":0,"y":0,"label":"A"}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"},"text":{"field":"label"}}}]}"#,
        true,
    )
    .expect("composed text font parses");
    let error = fulgur_chart::raster_direct::render_chart_to_png_default(&composed, 1.0)
        .expect_err("font validation must include text leaves in compositions");
    assert!(error.contains("font family"), "{error}");
}

#[test]
fn raster_accepts_sans_serif_alias_for_bundled_text_font() {
    let json = r#"{"mark":{"type":"text","font":"sans-serif"},"data":{"values":[{"x":0,"y":0,"label":"A"}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"},"text":{"field":"label"}}}"#;
    let spec = vegalite::parse(json, true).expect("text font parses");
    fulgur_chart::raster_direct::render_chart_to_png_default(&spec, 1.0)
        .expect("bundled Noto Sans JP supplies the sans-serif alias");
    fulgur_chart::raster_direct::render_chart_to_webp(&spec, 1.0, fulgur_chart::font::DEFAULT_FONT)
        .expect("WebP uses the same bundled text font mapping");

    let explicit_json = json.replace("sans-serif", "Noto Sans JP");
    let explicit_family =
        vegalite::parse(&explicit_json, true).expect("exact bundled family parses");
    fulgur_chart::raster_direct::render_chart_to_png_default(&explicit_family, 1.0)
        .expect("the exact supplied family is accepted");
}
