use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::model::build_model;
use fulgur_chart::raster_direct::{render_chart_to_png_default, render_chart_to_webp};
use fulgur_chart::render::render_chart;
use fulgur_chart::text::TextMeasurer;

const IMAGE_SPEC: &str = r##"{
  "width": 360,
  "height": 240,
  "mark": {"type":"image", "width":48, "height":32},
  "data": {"values":[
    {"x":0,"y":0,"src":"https://example.test/a.png?x=1&y=2"},
    {"x":10,"y":10,"src":"data:image/png;base64,AAAA"}
  ]},
  "encoding": {
    "x":{"field":"x","type":"quantitative"},
    "y":{"field":"y","type":"quantitative"},
    "url":{"field":"src","type":"nominal"}
  }
}"##;

fn image_tags(svg: &str) -> Vec<&str> {
    svg.split("<image ")
        .skip(1)
        .map(|part| part.split_once("/>").expect("self-closing image element").0)
        .collect()
}

fn attr(tag: &str, name: &str) -> f64 {
    let prefix = format!("{name}=\"");
    let value = tag
        .split_once(&prefix)
        .expect("attribute exists")
        .1
        .split_once('"')
        .expect("attribute closes")
        .0;
    value.parse().expect("numeric position")
}

#[test]
fn image_mark_emits_data_urls_at_scaled_data_positions_without_fetching() {
    let spec = vegalite::parse(IMAGE_SPEC, true).expect("image mark parses");
    let svg = render_chart(&spec);
    let tags = image_tags(&svg);

    assert_eq!(tags.len(), 2);
    assert!(tags[0].contains("href=\"https://example.test/a.png?x=1&amp;y=2\""));
    assert!(tags[1].contains("href=\"data:image/png;base64,AAAA\""));
    assert!(tags.iter().all(|tag| tag.contains("width=\"48\"")));
    assert!(tags.iter().all(|tag| tag.contains("height=\"32\"")));
    assert!(attr(tags[0], "x") < attr(tags[1], "x"));
    assert!(attr(tags[0], "y") > attr(tags[1], "y"));
}

#[test]
fn image_mark_rejects_non_image_or_unsafe_url_schemes() {
    for url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "//example.test/image.png",
        "https:/missing-host",
        "https://example.test/has space.png",
        "https://example.test:bad/image.png",
        "https://example.test:65536/image.png",
        "https://example.test/%zz",
        "data:text/html,hello",
        "data:image/png,hello world",
    ] {
        let json = format!(
            r#"{{"mark":{{"type":"image","width":10,"height":10}},"data":{{"values":[{{"x":1,"y":2,"src":{}}}]}},"encoding":{{"x":{{"field":"x","type":"quantitative"}},"y":{{"field":"y","type":"quantitative"}},"url":{{"field":"src"}}}}}}"#,
            serde_json::to_string(url).unwrap()
        );
        let error = vegalite::parse(&json, true).unwrap_err();
        assert!(error.contains("image URL"), "{url:?}: {error}");
    }
}

#[test]
fn image_mark_requires_a_url_and_positive_finite_dimensions() {
    let base = r#"{"mark":{"type":"image","width":10,"height":10},"data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"url":{"value":"https://example.test/a.png"}}}"#;
    let missing_url = base.replace(",\"url\":{\"value\":\"https://example.test/a.png\"}", "");
    assert!(vegalite::parse(&missing_url, true).is_err());

    for (width, height) in [(0, 10), (-1, 10), (10, 0)] {
        let json = base
            .replace("\"width\":10", &format!("\"width\":{width}"))
            .replace("\"height\":10", &format!("\"height\":{height}"));
        let error = vegalite::parse(&json, true).unwrap_err();
        assert!(
            error.contains("width") || error.contains("height"),
            "{error}"
        );
    }
}

#[test]
fn png_and_webp_rendering_report_image_mark_as_unsupported() {
    let spec = vegalite::parse(IMAGE_SPEC, true).expect("image mark parses");

    let png_error = render_chart_to_png_default(&spec, 1.0).unwrap_err();
    assert!(png_error.contains("image marks") && png_error.contains("SVG"));

    let webp_error = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).unwrap_err();
    assert!(webp_error.contains("image marks") && webp_error.contains("SVG"));
}

#[test]
fn image_mark_schema_accepts_channel_and_mark_level_urls() {
    let _: fulgur_chart::schema::VegaLiteSpec = serde_json::from_str(IMAGE_SPEC).unwrap();

    let mark_url = r#"{"mark":{"type":"image","width":8,"height":6,"url":"https://example.test/image.png"},"data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}}"#;
    let _: fulgur_chart::schema::VegaLiteSpec = serde_json::from_str(mark_url).unwrap();
    let spec = vegalite::parse(mark_url, true).unwrap();
    assert_eq!(image_tags(&render_chart(&spec)).len(), 1);

    let no_dimensions = r#"{"mark":"image","data":{"values":[{"x":1,"y":2,"src":"https://example.test/image.png"}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"},"url":{"field":"src"}}}"#;
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(no_dimensions).is_err());
    assert!(vegalite::parse(no_dimensions, true).is_err());
}

#[test]
fn image_mark_checks_url_fields_and_strict_unknown_keys() {
    let missing_field = r#"{"mark":{"type":"image","width":8,"height":6},"data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"url":{"field":"src"}}}"#;
    assert!(
        vegalite::parse(missing_field, true)
            .unwrap_err()
            .contains("URL string")
    );

    let mut unsupported = serde_json::from_str::<serde_json::Value>(IMAGE_SPEC).unwrap();
    unsupported["mark"]["aspect"] = serde_json::Value::Bool(false);
    let unsupported = serde_json::to_string(&unsupported).unwrap();
    let error = vegalite::parse(&unsupported, true).unwrap_err();
    assert!(error.contains("mark.aspect"), "{error}");
    assert!(vegalite::parse(&unsupported, false).is_ok());
}

#[test]
fn image_mark_respects_input_point_and_url_size_limits() {
    let json = r#"{"mark":{"type":"image","width":8,"height":6},"data":{"values":[{"x":1,"y":2,"src":"https://example.test/image.png"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"url":{"field":"src"}}}"#;
    let limits = fulgur_chart::guard::InputLimits {
        max_total_data_points: 0,
        ..fulgur_chart::guard::InputLimits::default()
    };
    assert!(
        vegalite::parse_with_limits(json, true, &limits)
            .unwrap_err()
            .contains("max_total_data_points")
    );

    let limits = fulgur_chart::guard::InputLimits {
        max_label_bytes: 12,
        ..fulgur_chart::guard::InputLimits::default()
    };
    assert!(
        vegalite::parse_with_limits(json, true, &limits)
            .unwrap_err()
            .contains("image URL")
    );
}

#[test]
fn image_mark_model_exposes_numeric_axes_and_image_geometry() {
    let spec = vegalite::parse(IMAGE_SPEC, true).unwrap();
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let model = build_model(&spec, &measurer);

    assert_eq!(model.meta.r#type, "image");
    assert_eq!(model.axes.as_ref().unwrap().x.kind, "linear");
    let geometry = model.geometry.unwrap();
    assert_eq!(geometry.elements.len(), 2);
    assert!(
        geometry
            .elements
            .iter()
            .all(|item| { item.kind == "image" && item.nw > 0.0 && item.nh > 0.0 })
    );
}
