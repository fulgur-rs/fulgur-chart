use fulgur_chart::frontend::chartjs;
use fulgur_chart::ir::{ChartKind, LegendAlign, LegendPointStyle, Point, ScaleKind, SeriesType};

#[test]
fn parses_minimal_bar_spec() {
    let json = r#"{
      "type": "bar",
      "data": {
        "labels": ["1月", "2月", "3月"],
        "datasets": [{ "label": "売上", "data": [120, 200, 150] }]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            horizontal: false,
            ..
        }
    ));
    assert_eq!(spec.categories, vec!["1月", "2月", "3月"]);
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].name, "売上");
    assert_eq!(spec.series[0].values, vec![120.0, 200.0, 150.0]);
    // 色未指定 → パレット先頭(#36A2EB) を全点へブロードキャスト(len==1)
    let c = spec.series[0].fill_at(0);
    assert_eq!((c.r, c.g, c.b), (54, 162, 235));
    assert_eq!(spec.series[0].fill.len(), 1); // bar は系列1色
}

/// Mixed dataset order sorts rendered legends and inspection models consistently.
#[test]
fn mixed_dataset_order_controls_series_and_model_order() {
    let json = r#"{
      "type": "bar",
      "data": {"labels": ["x"], "datasets": [
        {"label":"late line", "type":"line", "order":4, "data":[4]},
        {"label":"early bar", "type":"bar", "order":-1, "data":[1]},
        {"label":"default line", "type":"line", "data":[2]},
        {"label":"default bar", "type":"bar", "order":0, "data":[3]},
        {"label":"fractional line", "type":"line", "order":0.5, "data":[5]}
      ]}
    }"#;
    let spec = chartjs::parse(json, true).unwrap();
    let labels: Vec<_> = spec
        .series
        .iter()
        .map(|series| series.name.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "early bar",
            "default line",
            "default bar",
            "fractional line",
            "late line"
        ]
    );
    let svg = fulgur_chart::render::render_chart(&spec);
    let legend_positions: Vec<_> = labels
        .iter()
        .map(|label| svg.find(label).expect("legend label in SVG"))
        .collect();
    assert!(legend_positions.windows(2).all(|pair| pair[0] < pair[1]));

    let model = fulgur_chart::model::build_model_core(&spec);
    let model_labels: Vec<_> = model
        .series
        .iter()
        .map(|series| series.label.as_str())
        .collect();
    assert_eq!(model_labels, labels);
}

/// Bar and line schemas accept numeric order values, while unsupported types reject them.
#[test]
fn dataset_order_is_accepted_by_bar_and_line_schemas() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let bar_json = r#"{"type":"bar","data":{"datasets":[
      {"type":"bar","order":2.5,"data":[1]},
      {"type":"line","order":1.5,"data":[2]}
    ]}}"#;
    let line_json = r#"{"type":"line","data":{"datasets":[
      {"order":-1.25,"data":[1]}
    ]}}"#;
    let pie_json = r#"{"type":"pie","data":{"datasets":[
      {"order":1,"data":[1]}
    ]}}"#;

    assert!(matches!(
        serde_json::from_str::<ChartJsSpec>(bar_json).unwrap(),
        ChartJsSpec::Bar(_)
    ));
    assert!(matches!(
        serde_json::from_str::<ChartJsSpec>(line_json).unwrap(),
        ChartJsSpec::Line(_)
    ));
    assert!(chartjs::parse(bar_json, true).is_ok());
    assert!(chartjs::parse(line_json, true).is_ok());
    assert!(serde_json::from_str::<ChartJsSpec>(pie_json).is_err());
    assert!(chartjs::parse(pie_json, true).is_err());
}

#[test]
fn horizontal_bar_via_index_axis_y() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y"} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn pie_with_per_slice_colors() {
    let json = r##"{ "type":"pie","data":{"labels":["a","b","c"],
      "datasets":[{"data":[1,2,3],"backgroundColor":["#ff0000","#00ff00","#0000ff"]}]} }"##;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 3);
    let c2 = spec.series[0].fill_at(2);
    assert_eq!((c2.r, c2.g, c2.b), (0, 0, 255));
}

#[test]
fn pie_without_colors_uses_palette_per_slice() {
    let json = r#"{ "type":"pie","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 2); // pie はスライス別パレット
    assert_ne!(spec.series[0].fill_at(0), spec.series[0].fill_at(1));
}

#[test]
fn area_fill_string_mode_is_filled() {
    let json = r#"{ "type":"line","data":{"labels":["a"],
      "datasets":[{"data":[1],"fill":"origin"}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[0].area);
}

#[test]
fn line_schema_accepts_fill_target_with_above_and_below_colors() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let json = r##"{"type":"line","data":{"datasets":[
      {"data":[1,3],"fill":{"target":"-1","above":"#ff0000","below":"#0000ff"}},
      {"data":[3,1],"fill":0}
    ]}}"##;

    assert!(serde_json::from_str::<ChartJsSpec>(json).is_ok());
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn line_and_scatter_dataset_line_style_options_are_schema_and_strict_parseable() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let line = r#"{
      "type":"line",
      "data":{"datasets":[{
        "data":[1,2],"pointStyle":"triangle","showLine":false,
        "borderDash":[5,3],"borderDashOffset":-2
      }]}
    }"#;
    let scatter = r#"{
      "type":"scatter",
      "data":{"datasets":[{
        "data":[{"x":1,"y":2},{"x":3,"y":4}],"pointStyle":false,
        "showLine":true,"borderDash":[2,1],"borderDashOffset":1
      }]}
    }"#;

    assert!(serde_json::from_str::<ChartJsSpec>(line).is_ok());
    assert!(chartjs::parse(line, true).is_ok());
    assert!(serde_json::from_str::<ChartJsSpec>(scatter).is_ok());
    assert!(chartjs::parse(scatter, true).is_ok());
}

#[test]
fn line_dataset_accepts_each_documented_point_style_and_rejects_invalid_values() {
    use fulgur_chart::ir::DatasetPointStyle;
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let cases = [
        ("circle", DatasetPointStyle::Circle),
        ("triangle", DatasetPointStyle::Triangle),
        ("rect", DatasetPointStyle::Rect),
        ("rectRounded", DatasetPointStyle::RectRounded),
        ("rectRot", DatasetPointStyle::RectRot),
        ("cross", DatasetPointStyle::Cross),
        ("crossRot", DatasetPointStyle::CrossRot),
        ("star", DatasetPointStyle::Star),
        ("line", DatasetPointStyle::Line),
        ("dash", DatasetPointStyle::Dash),
    ];
    let circle = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2],"pointStyle":"circle"}]}}"#,
        true,
    )
    .unwrap();
    let circle_png =
        fulgur_chart::raster_direct::render_chart_to_png_default(&circle, 1.0).unwrap();
    let hidden = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2],"pointStyle":false}]}}"#,
        true,
    )
    .unwrap();
    let hidden_png =
        fulgur_chart::raster_direct::render_chart_to_png_default(&hidden, 1.0).unwrap();
    for (name, expected) in cases {
        let json = format!(
            r#"{{"type":"line","data":{{"labels":["a","b"],"datasets":[{{"data":[1,2],"pointStyle":"{name}"}}]}}}}"#
        );
        assert!(
            serde_json::from_str::<ChartJsSpec>(&json).is_ok(),
            "schema rejected {name}"
        );
        let spec = chartjs::parse(&json, true).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            spec.series[0].line_style.as_ref().unwrap().point_style,
            Some(expected)
        );
        let png = fulgur_chart::raster_direct::render_chart_to_png_default(&spec, 1.0).unwrap();
        assert!(
            hidden_png != png,
            "pointStyle {name} should render a raster marker"
        );
        if name != "circle" {
            assert!(
                circle_png != png,
                "pointStyle {name} should change raster output"
            );
        }
    }

    for invalid in [
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"pointStyle":true}]}}"#,
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"pointStyle":"image.png"}]}}"#,
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"showLine":"false"}]}}"#,
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"borderDash":"5 2"}]}}"#,
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"borderDash":[5,-2]}]}}"#,
        r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1],"borderDashOffset":"2"}]}}"#,
    ] {
        assert!(
            serde_json::from_str::<ChartJsSpec>(invalid).is_err(),
            "schema accepted {invalid}"
        );
        assert!(chartjs::parse(invalid, false).is_err());
    }

    let bar_dataset = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"type":"bar","data":[1],"showLine":false}]}}"#;
    assert!(serde_json::from_str::<ChartJsSpec>(bar_dataset).is_ok());
    assert!(chartjs::parse(bar_dataset, false).is_err());
}

#[test]
fn line_dataset_style_controls_line_markers_and_dash_rendering() {
    let styled = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{
          "data":[1,3],"pointStyle":"triangle","showLine":true,
          "borderDash":[5,3],"borderDashOffset":-2
        }]}}"#,
        true,
    )
    .unwrap();
    let style = styled.series[0].line_style.as_ref().unwrap();
    assert!(style.show_line);
    assert_eq!(
        style.point_style,
        Some(fulgur_chart::ir::DatasetPointStyle::Triangle)
    );
    assert_eq!(style.border_dash, vec![5.0, 3.0]);
    assert_eq!(style.border_dash_offset, -2.0);
    let svg = fulgur_chart::render::render_chart(&styled);
    assert!(svg.contains("stroke-dasharray=\"5 3\""), "{svg}");
    assert!(svg.contains("stroke-dashoffset=\"-2\""));
    assert!(!svg.contains("<circle"));

    let hidden_line = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,3],"showLine":false}]}}"#,
        true,
    )
    .unwrap();
    let hidden_line_svg = fulgur_chart::render::render_chart(&hidden_line);
    assert!(!hidden_line_svg.contains("<polyline"));
    assert_eq!(hidden_line_svg.matches("<circle").count(), 2);

    let hidden_points = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,3],"pointStyle":false}]}}"#,
        true,
    )
    .unwrap();
    let hidden_points_svg = fulgur_chart::render::render_chart(&hidden_points);
    assert!(hidden_points_svg.contains("<polyline"));
    assert!(!hidden_points_svg.contains("<circle"));
}

#[test]
fn show_line_false_keeps_markers_when_default_decimation_is_active() {
    let values: Vec<usize> = (0..2_000).map(|index| index % 73).collect();
    let labels = vec![""; values.len()];
    let json = serde_json::json!({
        "type": "line",
        "data": {
            "labels": labels,
            "datasets": [{ "data": values, "showLine": false }]
        },
        "width": 240,
        "height": 180,
        "options": { "plugins": { "legend": { "display": false } } }
    })
    .to_string();
    let spec = chartjs::parse(&json, true).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);

    assert!(
        !svg.contains("<polyline"),
        "showLine=false must suppress lines"
    );
    assert!(
        svg.contains("<circle"),
        "default decimation must not suppress every marker when showLine=false"
    );
}

#[test]
fn scatter_line_style_keeps_default_line_hidden_and_supports_opt_in() {
    let default = chartjs::parse(
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4}]}]}}"#,
        true,
    )
    .unwrap();
    assert!(!default.series[0].line_style.as_ref().unwrap().show_line);
    let default_svg = fulgur_chart::render::render_chart(&default);
    assert!(!default_svg.contains("<polyline"));

    let enabled = chartjs::parse(
        r#"{"type":"scatter","data":{"datasets":[{
          "data":[{"x":1,"y":2},{"x":3,"y":4}],"showLine":true,
          "pointStyle":false,"borderDash":[2,1],"borderDashOffset":1
        }]}}"#,
        true,
    )
    .unwrap();
    let enabled_svg = fulgur_chart::render::render_chart(&enabled);
    assert!(enabled_svg.contains("<polyline"));
    assert!(enabled_svg.contains("stroke-dasharray=\"2 1\""));
    assert!(enabled_svg.contains("stroke-dashoffset=\"1\""));
    assert!(!enabled_svg.contains("<circle"));

    let clipped = chartjs::parse(
        r##"{"type":"scatter","data":{"datasets":[{"data":[{"x":-1,"y":0.2},{"x":0.5,"y":0.5},{"x":2,"y":0.8}],"showLine":true,"pointStyle":"triangle","backgroundColor":"#0000ff","borderColor":"#ff0000","borderWidth":2}]},"options":{"scales":{"x":{"min":0,"max":1},"y":{"min":0,"max":1}}}}"##,
        true,
    )
    .unwrap();
    let clipped_svg = fulgur_chart::render::render_chart(&clipped);
    assert!(
        clipped_svg.contains("<polyline"),
        "visible portions of out-of-range segments should remain drawn"
    );
    assert!(clipped_svg.contains("fill=\"#0000ff\" stroke=\"#ff0000\" stroke-width=\"2\""));
}

#[test]
fn scatter_show_line_clips_segments_to_the_plot_rectangle() {
    let spec = chartjs::parse(
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":-1,"y":0.2},{"x":0.5,"y":0.5}],"showLine":true}]},"options":{"scales":{"x":{"min":0,"max":1},"y":{"min":0,"max":1}}}}"#,
        true,
    )
    .unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    let measurer = fulgur_chart::text::TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let layout = fulgur_chart::layout::scatter::compute_scatter_layout(&spec, &measurer);
    let clipped_intersection_x = fulgur_chart::num::fmt_num(layout.plot_left);
    // The segment from (-1, 0.2) to (0.5, 0.5) intersects x=0 at y=0.4.
    let clipped_intersection_y = fulgur_chart::num::fmt_num(layout.ys.map(0.4));
    assert!(svg.contains(&format!(
        "points=\"{clipped_intersection_x},{clipped_intersection_y} "
    )));

    let outside = chartjs::parse(
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":-2,"y":0.2},{"x":-1,"y":0.8}],"showLine":true}]},"options":{"scales":{"x":{"min":0,"max":1},"y":{"min":0,"max":1}}}}"#,
        true,
    )
    .unwrap();
    let outside_svg = fulgur_chart::render::render_chart(&outside);
    assert!(
        !outside_svg.contains("<polyline"),
        "a segment wholly outside the plot must not be clamped into view"
    );
}

#[test]
fn line_style_options_apply_to_line_datasets_in_line_root_mixed_charts() {
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[
          {"type":"bar","data":[1,2]},
          {"type":"line","data":[2,3],"showLine":true,"borderDash":[4,2]}
        ]}}"#,
        true,
    )
    .unwrap();
    assert!(spec.series[0].line_style.is_none());
    assert!(spec.series[1].line_style.as_ref().unwrap().show_line);
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.contains("stroke-dasharray=\"4 2\""));

    let hidden = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[
          {"type":"bar","data":[1,2]},
          {"type":"line","data":[2,3],"showLine":false}
        ]}}"#,
        true,
    )
    .unwrap();
    assert!(!fulgur_chart::render::render_chart(&hidden).contains("<polyline"));
}

#[test]
fn line_style_options_apply_to_bar_root_mixed_line_datasets() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let json = r##"{"type":"bar","data":{"labels":["a","b"],"datasets":[
      {"data":[1,2]},
      {"type":"line","data":[2,3],"pointStyle":"triangle","showLine":true,"borderDash":[4,2],"borderDashOffset":1}
    ]}}"##;
    assert!(serde_json::from_str::<ChartJsSpec>(json).is_ok());
    let spec = chartjs::parse(json, true).unwrap();
    let line_style = spec.series[1].line_style.as_ref().unwrap();
    assert_eq!(
        line_style.point_style,
        Some(fulgur_chart::ir::DatasetPointStyle::Triangle)
    );
    assert!(line_style.show_line);
    assert_eq!(line_style.border_dash, vec![4.0, 2.0]);
    assert_eq!(line_style.border_dash_offset, 1.0);
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.contains("stroke-dasharray=\"4 2\""));
    assert!(svg.contains("stroke-dashoffset=\"1\""));

    let hidden = r##"{"type":"bar","data":{"labels":["a","b"],"datasets":[
      {"data":[1,2]},
      {"type":"line","data":[2,3],"pointStyle":false,"showLine":false,"borderDash":[4,2]}
    ]}}"##;
    let hidden = chartjs::parse(hidden, true).unwrap();
    assert!(!fulgur_chart::render::render_chart(&hidden).contains("<polyline"));
}

#[test]
fn mixed_line_dataset_schema_accepts_advanced_fill_targets() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let json = r##"{"type":"bar","data":{"datasets":[
      {"type":"line","data":[1,3],"fill":{"target":"origin","above":"#ff0000"}},
      {"type":"bar","data":[2,2]}
    ]}}"##;

    assert!(serde_json::from_str::<ChartJsSpec>(json).is_ok());
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn title_from_plugins() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},
      "options":{"plugins":{"title":{"display":true,"text":"四半期売上"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.title.as_deref(), Some("四半期売上"));
}

#[test]
fn title_not_displayed_is_none() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},
      "options":{"plugins":{"title":{"display":false,"text":"x"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.title, None);
}

#[test]
fn legend_options_and_title_are_resolved_from_plugin_config() {
    let json = r##"{
      "type":"line",
      "data":{"labels":["A"],"datasets":[{"label":"Alpha","data":[1]}]},
      "options":{"plugins":{"legend":{
        "align":"start","reverse":true,
        "labels":{"color":"#123456","font":{"size":15,"family":"Fira Sans","weight":600,"style":"italic"},
          "padding":8,"boxWidth":22,"boxHeight":14,"usePointStyle":true,"pointStyle":"triangle"},
        "title":{"display":true,"text":"Series","color":"#abcdef",
          "font":{"size":18,"family":"Fira Mono","weight":"bold","style":"oblique"},
          "padding":{"top":2,"right":3,"bottom":4,"left":5}}
      }}}
    }"##;
    let spec = chartjs::parse(json, true).unwrap();

    assert_eq!(spec.legend_options.align, LegendAlign::Start);
    assert!(spec.legend_options.reverse);
    let labels_color = spec.legend_options.labels_color.unwrap();
    assert_eq!(
        (labels_color.r, labels_color.g, labels_color.b),
        (18, 52, 86)
    );
    assert_eq!(spec.legend_options.labels_font_size, Some(15.0));
    assert_eq!(
        spec.legend_options.labels_font_family.as_deref(),
        Some("Fira Sans")
    );
    assert_eq!(
        spec.legend_options.labels_font_weight.as_deref(),
        Some("600")
    );
    assert_eq!(
        spec.legend_options.labels_font_style.as_deref(),
        Some("italic")
    );
    assert_eq!(spec.legend_options.labels_padding, Some(8.0));
    assert_eq!(spec.legend_options.labels_box_width, Some(22.0));
    assert_eq!(spec.legend_options.labels_box_height, Some(14.0));
    assert!(spec.legend_options.labels_use_point_style);
    assert_eq!(
        spec.legend_options.labels_point_style,
        Some(LegendPointStyle::Triangle)
    );
    assert!(spec.legend_options.title_display);
    assert_eq!(spec.legend_title.as_deref(), Some("Series"));
    let title_color = spec.legend_options.title_color.unwrap();
    assert_eq!(
        (title_color.r, title_color.g, title_color.b),
        (171, 205, 239)
    );
    assert_eq!(spec.legend_options.title_font_size, Some(18.0));
    assert_eq!(
        spec.legend_options.title_font_family.as_deref(),
        Some("Fira Mono")
    );
    assert_eq!(
        spec.legend_options.title_font_weight.as_deref(),
        Some("bold")
    );
    assert_eq!(
        spec.legend_options.title_font_style.as_deref(),
        Some("oblique")
    );
    assert_eq!(
        spec.legend_options.title_padding,
        fulgur_chart::ir::LegendTitlePadding {
            top: 2.0,
            right: 3.0,
            bottom: 4.0,
            left: 5.0,
        }
    );
}

#[test]
fn strict_legend_config_rejects_unknown_nested_keys() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"legend":{"labels":{"font":{"famly":"typo"}}}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}

#[test]
fn invalid_json_is_err() {
    assert!(chartjs::parse("{ not json", false).is_err());
}

#[test]
fn unknown_type_is_err() {
    let json = r#"{ "type":"unknownChart","data":{"labels":[],"datasets":[]} }"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn parses_radar_spec() {
    let json = r#"{
      "type": "radar",
      "data": {
        "labels": ["速度", "力", "技"],
        "datasets": [
          { "label": "A", "data": [60, 80, 40] },
          { "label": "B", "data": [50, 30, 90] }
        ]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Radar));
    assert_eq!(spec.categories, vec!["速度", "力", "技"]);
    assert_eq!(spec.series.len(), 2);
    assert_eq!(spec.series[0].values, vec![60.0, 80.0, 40.0]);
    assert_eq!(spec.series[1].values, vec![50.0, 30.0, 90.0]);
    // radar はカテゴリ系なので点データは空。
    assert!(spec.series[0].points.is_empty());
    // r 軸はゼロ起点(begin_at_zero)。
    assert!(spec.y_axis.begin_at_zero);
}

#[test]
fn strict_accepts_radar() {
    let json = r#"{ "type":"radar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_unknown_top_level_key() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},"wat":1 }"#;
    assert!(chartjs::parse(json, true).is_err()); // strict は未知キーで Err
    assert!(chartjs::parse(json, false).is_ok()); // 非strict は無視
}

#[test]
fn strict_rejects_unknown_dataset_key() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],
      "datasets":[{"data":[1],"bogusKey":1}]} }"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}

#[test]
fn datalabels_key_present_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_true_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_false_disables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":false}}} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_absent_is_false() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn strict_accepts_known_datalabels_keys() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}
#[test]
fn strict_rejects_unknown_datalabels_key() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"foo":1}}} }"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn scales_y_only_on_vertical_is_not_stacked() {
    // 縦棒(既定 indexAxis:x)で値軸 y のみ stacked → chart.js は棒を dodge(並置)する。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: true,
            ..
        }
    ));
}

#[test]
fn scales_x_stacked_true_marks_bar_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn horizontal_y_stacked_marks_bar_stacked() {
    // 横棒(indexAxis:y)は index 軸が y。y.stacked → 積み上げ。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y","scales":{"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: false,
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn horizontal_x_stacked_only_is_not_stacked() {
    // 横棒(indexAxis:y)で値軸 x のみ stacked → index 軸(y)未指定なので dodge。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y","scales":{"x":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: true,
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn scales_absent_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn scales_stacked_false_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":false}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn strict_accepts_scales_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_accepts_scales_offset() {
    // chart.js category スケールの offset は認識済みキー。strict でも通る。
    let json = r#"{ "type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"scales":{"x":{"offset":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn both_axes_stacked_sets_both_flags() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: true,
            ..
        }
    ));
}

#[test]
fn parses_scatter_point_data() {
    let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(
        spec.series[0].points,
        vec![
            Point {
                x: 1.0,
                y: 2.0,
                r: None
            },
            Point {
                x: 3.0,
                y: 4.0,
                r: None
            },
        ]
    );
    // scatter は数値配列を使わない。
    assert!(spec.series[0].values.is_empty());
}

#[test]
fn categorical_bar_has_empty_points() {
    // 既存のカテゴリ系パース(数値配列)は points を空に保つ。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].values, vec![1.0, 2.0]);
    assert!(spec.series[0].points.is_empty());
}

#[test]
fn strict_accepts_scatter() {
    let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn parses_bubble_point_data_with_radius() {
    // bubble は scatter と同じ点データだが、第3次元 r を保持する。
    let json = r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":10}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bubble));
    assert_eq!(spec.series[0].points[0].r, Some(10.0));
    assert_eq!(
        spec.series[0].points[0],
        Point {
            x: 1.0,
            y: 2.0,
            r: Some(10.0)
        }
    );
    // 点ベースなので数値配列は使わない。
    assert!(spec.series[0].values.is_empty());
}

#[test]
fn bar_base_with_line_dataset_is_mixed() {
    // 基本型 bar + dataset 別 type:"line" → Mixed、種別は [Bar, Line]。
    let json = r#"{"type":"bar","data":{"labels":["a","b","c"],
      "datasets":[{"label":"棒","data":[1,2,3]},{"type":"line","label":"折れ線","data":[4,5,6]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Mixed));
    assert_eq!(spec.series[0].series_type, SeriesType::Bar);
    assert_eq!(spec.series[1].series_type, SeriesType::Line);
}

#[test]
fn all_bar_without_type_stays_bar() {
    // dataset 別 type 未指定の全棒は従来どおり Bar(混合に昇格しない)。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]},{"data":[3,4]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
    assert_eq!(spec.series[0].series_type, SeriesType::Bar);
    assert_eq!(spec.series[1].series_type, SeriesType::Bar);
}

#[test]
fn line_base_with_bar_dataset_is_mixed() {
    // 基本型 line + dataset 別 type:"bar" でも混合になる(対称性の確認)。
    let json = r#"{"type":"line","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]},{"type":"bar","data":[3,4]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Mixed));
    assert_eq!(spec.series[0].series_type, SeriesType::Line);
    assert_eq!(spec.series[1].series_type, SeriesType::Bar);
}

#[test]
fn strict_accepts_dataset_type() {
    let json = r#"{"type":"bar","data":{"labels":["a"],
      "datasets":[{"data":[1]},{"type":"line","data":[2]}]}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn single_dataset_type_override_changes_kind() {
    // 基本 type=bar でも、全 dataset が type:"line" なら kind は Line になる
    // (混合でない単独上書き。以前は kind=Bar のままで line が棒描画されていた)。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],
      "datasets":[{"type":"line","data":[1,2]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: false, .. }));
    assert_eq!(spec.series[0].series_type, SeriesType::Line);
}

#[test]
fn scales_y_stacked_true_marks_line_stacked() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],
      "datasets":[{"data":[10,20]},{"data":[5,15]}]},
      "options":{"scales":{"y":{"stacked":true}}}}"#;
    let spec = chartjs::parse(json, true).unwrap();

    assert!(matches!(
        spec.kind,
        ChartKind::Line {
            stacked: true,
            stacked_missing_values_are_gaps: true
        }
    ));
}

#[test]
fn empty_line_preserves_value_axis_stacked_flag() {
    let json = r#"{"type":"line","data":{"labels":[],"datasets":[]},
      "options":{"scales":{"y":{"stacked":true}}}}"#;
    let spec = chartjs::parse(json, false).unwrap();

    assert!(matches!(spec.kind, ChartKind::Line { stacked: true, .. }));
}

#[test]
fn horizontal_stacked_line_is_rejected_until_supported() {
    let json = r#"{"type":"line","data":{"labels":["A","B"],
      "datasets":[{"data":[10,20]},{"data":[5,15]}]},
      "options":{"indexAxis":"y","scales":{"x":{"stacked":true}}}}"#;
    let error = chartjs::parse(json, false).expect_err("horizontal stacked line is unsupported");

    assert!(error.contains("積み上げ line chart は横向き(indexAxis:y)に未対応です"));
}

#[test]
fn unsupported_dataset_type_errors() {
    // bar 基本型に scatter dataset を混ぜるのは未対応。点データが空で「成功扱いの
    // 空チャート」になるのを防ぎ、明示エラーにする。
    let json = r#"{"type":"bar","data":{"labels":["a"],
      "datasets":[{"type":"scatter","data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn radar_rejects_negative_values() {
    // 負の半径は頂点が反対スポークへ反転するため、レーダーは負値を拒否する。
    let json = r#"{"type":"radar","data":{"labels":["a","b","c"],
      "datasets":[{"data":[3,-1,2]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn mixed_with_horizontal_or_stacked_errors() {
    let base_datasets =
        r#""data":{"labels":["a"],"datasets":[{"data":[1]},{"type":"line","data":[2]}]}"#;
    // 横棒×混合 → エラー(mixed は縦・非積み上げのみ)。
    let horiz = format!(r#"{{"type":"bar",{base_datasets},"options":{{"indexAxis":"y"}}}}"#);
    assert!(chartjs::parse(&horiz, false).is_err());
    // placement_stacked×混合 → エラー。
    let stk = format!(
        r#"{{"type":"bar",{base_datasets},"options":{{"scales":{{"x":{{"stacked":true}}}}}}}}"#
    );
    assert!(chartjs::parse(&stk, false).is_err());
    // value_stacked×混合 → エラー(ChartKind::Mixed にフラグが伝わらず消えるため)。
    let vstk = format!(
        r#"{{"type":"bar",{base_datasets},"options":{{"scales":{{"y":{{"stacked":true}}}}}}}}"#
    );
    assert!(chartjs::parse(&vstk, false).is_err());
    // 通常の混合は従来どおり Mixed。
    let ok = format!(r#"{{"type":"bar",{base_datasets}}}"#);
    assert!(matches!(
        chartjs::parse(&ok, false).unwrap().kind,
        ChartKind::Mixed
    ));
}

#[test]
fn strict_rejects_unknown_point_key() {
    // 点オブジェクト {x,y,r} の typo(radius は r が正) を strict で検出。
    let json = r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"radius":20}]}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok()); // 非strict は無視
}

#[test]
fn data_shape_mismatch_errors() {
    // scatter に数値配列 → 点データが空になる空チャート化を防ぎ、明示エラーに。
    let scatter_nums = r#"{"type":"scatter","data":{"datasets":[{"data":[1,2,3]}]}}"#;
    assert!(chartjs::parse(scatter_nums, false).is_err());
    // bar に {x,y} 点配列 → エラー(values が空になる欠損を防ぐ)。
    let bar_points =
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(bar_points, false).is_err());
}

#[test]
fn dataset_type_on_non_mixable_base_errors() {
    // pie に dataset type:line → 無視して別種描画せず、明示エラーに。
    let json = r#"{"type":"pie","data":{"labels":["a","b"],
      "datasets":[{"type":"line","data":[1,2]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn strict_rejects_scales_typo() {
    // stacked は描画に効くので、typo を strict で取りこぼさない。
    // non-strict では Chart.js の未実装フィールド(ticks/type など)を silently 通す
    // 互換方針のため、scales.{x,y} 直下は `AxisOptions` の deny_unknown_fields を
    // 外し、strict モードの検出は frontend/chartjs.rs の check_unknown_keys allow-list
    // に一本化している。
    let typo = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stakced":true}}}}"#;
    assert!(chartjs::parse(typo, true).is_err());
    assert!(chartjs::parse(typo, false).is_ok());
    // 正しい stacked キーは strict でも通る。
    let ok = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}}}"#;
    assert!(chartjs::parse(ok, true).is_ok());
    assert!(chartjs::parse(ok, false).is_ok());
}

#[test]
fn linear_tick_options_roundtrip_in_schema_and_parse_in_strict_mode() {
    let json = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[10]}]},
      "options":{"scales":{"y":{"ticks":{"stepSize":2,"maxTicksLimit":4,"count":3,
        "precision":1,"format":{"minimumFractionDigits":2,"maximumFractionDigits":3,
          "notation":"scientific"}}}}}}"#;

    let schema_spec: fulgur_chart::schema::chartjs::ChartJsSpec =
        serde_json::from_str(json).expect("schema should accept supported linear tick options");
    let roundtrip = serde_json::to_value(schema_spec).expect("schema should serialize");
    assert_eq!(
        roundtrip["options"]["scales"]["y"]["ticks"],
        serde_json::json!({
            "stepSize": 2.0,
            "maxTicksLimit": 4,
            "count": 3,
            "precision": 1,
            "format": {
                "minimumFractionDigits": 2,
                "maximumFractionDigits": 3,
                "notation": "scientific"
            }
        })
    );
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn linear_ticks_default_max_ticks_limit_matches_chartjs() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    let spec = chartjs::parse(json, true).expect("parse default axis ticks");

    assert_eq!(spec.y_axis.ticks.max_ticks_limit, Some(11));
}

#[test]
fn strict_mode_rejects_unknown_linear_tick_and_format_options() {
    let unknown_tick = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"ticks":{"stepSzie":2}}}}}"#;
    assert!(chartjs::parse(unknown_tick, true).is_err());

    let unknown_format = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"ticks":{"format":{"maximumFractionDigts":2}}}}}}"#;
    assert!(chartjs::parse(unknown_format, true).is_err());
}

#[test]
fn non_strict_mode_ignores_unimplemented_chartjs_tick_options() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"ticks":{"display":false,"autoSkip":false,
        "format":{"useGrouping":false}}}}}}"#;

    assert!(chartjs::parse(json, false).is_ok());
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn matrix_parses_categories_and_series() {
    let json = r#"{
        "type": "matrix",
        "data": {"datasets": [{"label": "h", "data": [
            {"x": "Mon", "y": "Morning", "v": 5.0},
            {"x": "Tue", "y": "Morning", "v": 8.0},
            {"x": "Mon", "y": "Evening", "v": 3.0},
            {"x": "Tue", "y": "Evening", "v": 9.0}
        ], "backgroundColor": "rgba(54,162,235,1.0)"}]}
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Matrix { .. }));
    assert_eq!(spec.categories, vec!["Mon", "Tue"]);
    assert_eq!(spec.series.len(), 2);
    assert_eq!(spec.series[0].name, "Morning");
    assert_eq!(spec.series[0].values, vec![5.0, 8.0]);
    assert_eq!(spec.series[1].name, "Evening");
    assert_eq!(spec.series[1].values, vec![3.0, 9.0]);
}

#[test]
fn matrix_multiple_datasets_is_error() {
    let json = r#"{"type":"matrix","data":{"datasets":[
        {"data":[{"x":"A","y":"X","v":1}]},
        {"data":[{"x":"A","y":"X","v":2}]}
    ]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn matrix_missing_cell_becomes_nan() {
    let json = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":1.0},
        {"x":"Tue","y":"Morning","v":2.0},
        {"x":"Mon","y":"Evening","v":3.0}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[1].values[1].is_nan());
}

#[test]
fn matrix_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "matrix",
        "data": {
            "datasets": [{
                "label": "Heat",
                "data": [{"x": "Mon", "y": "AM", "v": 5.0}],
                "backgroundColor": "#36a2eb"
            }]
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Matrix(_)));
}

#[test]
fn schema_strict_parity_decimation_matrix() {
    // schema(MatrixPlugins)は options.plugins.decimation を受理する。strict matrix パーサも
    // 受理し、危険方向のパリティ破れ(schema OK / strict NG)を作らないこと。decimation は
    // matrix では no-op(line のみ参照)だが Chart.js のグローバルプラグイン挙動どおり
    // 「受理して無視」する。
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "matrix",
        "data": { "datasets": [{ "data": [{"x": "Mon", "y": "AM", "v": 5.0}] }] },
        "options": { "plugins": { "decimation": { "enabled": true, "algorithm": "lttb" } } }
    }"##;
    // strict 側: 厳格パーサも受理する。
    assert!(chartjs::parse(json, true).is_ok());
    // schema 側: ChartJsSpec でも受理される。
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Matrix(_)));
}

#[test]
fn matrix_schema_and_parser_both_reject_datalabels() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // matrix は datalabels を描画しない(parse_matrix は theme のみ消費)。schema(MatrixPlugins)と
    // strict パーサ(check_unknown_keys_matrix)が共に拒否し、「schema 受理 → strict 拒否」の
    // 危険方向のパリティ破れを起こさないこと。sankey #87 と同型の契約。
    let json = r##"{"type":"matrix","data":{"datasets":[{"data":[{"x":"Mon","y":"AM","v":5.0}]}]},"options":{"plugins":{"datalabels":{}}}}"##;
    assert!(
        serde_json::from_str::<ChartJsSpec>(json).is_err(),
        "schema は matrix の plugins.datalabels を拒否すべき"
    );
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict パーサも plugins.datalabels を拒否すべき"
    );
    // title/legend/decimation は schema・parser とも受理する(正常系の確認)。
    let ok = r##"{"type":"matrix","data":{"datasets":[{"data":[{"x":"Mon","y":"AM","v":5.0}]}]},"options":{"plugins":{"title":{"display":true,"text":"T"},"legend":{"display":true}}}}"##;
    assert!(matches!(
        serde_json::from_str::<ChartJsSpec>(ok).unwrap(),
        ChartJsSpec::Matrix(_)
    ));
    assert!(chartjs::parse(ok, true).is_ok());
}

#[test]
fn schema_strict_parity_decimation_sparkline() {
    // sparkline はマーカー無し。line と同じ decimation を受理する。runtime strict と
    // 公開 schema の両方が options.plugins.decimation を受理し、危険方向のパリティ破れ
    // (schema OK / strict NG、またはその逆) を作らないこと。
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "sparkline",
        "data": { "datasets": [{ "data": [1.0, 2.0, 3.0] }] },
        "options": { "plugins": { "decimation": { "enabled": false, "algorithm": "lttb" } } }
    }"##;
    // strict 側: 厳格パーサが受理し、enabled=false が spec に届く。
    let spec = chartjs::parse(json, true).unwrap();
    assert!(!spec.decimation.enabled);
    // schema 側: ChartJsSpec でも受理される。
    let schema_spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(schema_spec, ChartJsSpec::Sparkline(_)));
}

#[test]
fn treemap_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    // Grouped tree (objects + key/groups), exercising the hierarchy path.
    let grouped = r##"{
        "type": "treemap",
        "options": {"plugins": {"title": {"display": true, "text": "T"}}},
        "data": {
            "datasets": [{
                "key": "value",
                "groups": ["region", "product"],
                "tree": [
                    {"region": "EMEA", "product": "A", "value": 12},
                    {"region": "APAC", "product": "B", "value": 7}
                ]
            }]
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(grouped).unwrap();
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    // The same document must be accepted by the runtime parser in strict mode.
    assert!(
        chartjs::parse(grouped, true).is_ok(),
        "strict parser should accept grouped treemap"
    );

    // Flat numeric tree (the untagged Numbers branch).
    let numeric = r##"{
        "type": "treemap",
        "data": {"datasets": [{"tree": [6, 4, 3, 2, 1]}]}
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(numeric).unwrap();
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    assert!(
        chartjs::parse(numeric, true).is_ok(),
        "strict parser should accept numeric treemap"
    );

    // Documented asymmetry: the JSON Schema is a deliberate superset, so it accepts
    // an object tree without `key` (the untagged enum can't make `key` conditionally
    // required), but the runtime parser rejects it because `key` is required to sum
    // object values into a hierarchy.
    let object_no_key = r##"{
        "type": "treemap",
        "data": {"datasets": [{"groups": ["g"], "tree": [{"g": "a", "v": 1}]}]}
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(object_no_key)
        .expect("schema (superset) should accept object tree without key");
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    assert!(
        chartjs::parse(object_no_key, false).is_err(),
        "runtime parser must reject an object tree without key"
    );
}

#[test]
fn matrix_strict_mode_accepts_v_key() {
    let json = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1}
    ]}]}}"#;
    // strict モードでも matrix は受理されるべき
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict mode should accept matrix with v key"
    );
}

#[test]
fn parses_polar_area_spec() {
    let json = r#"{
      "type": "polarArea",
      "data": {
        "labels": ["A", "B", "C"],
        "datasets": [{ "data": [10, 20, 30] }]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::PolarArea));
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series[0].values, vec![10.0, 20.0, 30.0]);
}

#[test]
fn wordcloud_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    // color 配列 + options 付き
    let json = r##"{
        "type": "wordCloud",
        "data": {
            "labels": ["Rust", "SVG", "Chart"],
            "datasets": [{"data": [90.0, 60.0, 45.0], "color": ["#e63946", "#457b9d", "#2a9d8f"]}]
        },
        "options": {
            "elements": {"word": {"minRotation": -90.0, "maxRotation": 0.0, "rotationSteps": 2, "padding": 2.0}}
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::WordCloud(_)));

    // scalar color
    let scalar = r##"{"type":"wordCloud","data":{"labels":["Hi"],"datasets":[{"data":[40.0],"color":"#ff0000"}]}}"##;
    let s: ChartJsSpec = serde_json::from_str(scalar).unwrap();
    assert!(matches!(s, ChartJsSpec::WordCloud(_)));

    // options なし
    let minimal = r##"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[20.0]}]}}"##;
    let m: ChartJsSpec = serde_json::from_str(minimal).unwrap();
    assert!(matches!(m, ChartJsSpec::WordCloud(_)));
}

#[test]
fn strict_accepts_wordcloud_with_width_height() {
    let json = r#"{"type":"wordCloud","width":800,"height":600,"data":{"labels":["A"],"datasets":[{"data":[30.0]}]}}"#;
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict mode should allow width/height"
    );
}

#[test]
fn strict_rejects_wordcloud_unknown_top_level_key() {
    let json =
        r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"typo":1}"#;
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict mode should reject unknown top-level key"
    );
    assert!(
        chartjs::parse(json, false).is_ok(),
        "non-strict should ignore unknown key"
    );
}

#[test]
fn strict_rejects_wordcloud_unknown_dataset_key() {
    let json =
        r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0],"typo":1}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}

#[test]
fn strict_accepts_wordcloud_elements_word() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"elements":{"word":{"minRotation":-90,"maxRotation":0,"rotationSteps":2,"padding":2}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_word_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"elements":{"word":{"minRotation":-90,"typo":1}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_accepts_wordcloud_plugins_title() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"plugins":{"title":{"display":true,"text":"Cloud"}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_plugins_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"plugins":{"legend":{}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_accepts_wordcloud_theme() {
    let json = r##"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"theme":{"palette":"warm","textColor":"#333"}}}"##;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_theme_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"theme":{"unknownKey":1}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_rejects_wordcloud_unknown_options_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"typo":1}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn sankey_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "sankey",
        "data": { "datasets": [{
            "label": "Energy",
            "data": [
                {"from": "A", "to": "B", "flow": 10},
                {"from": "A", "to": "C", "flow": 5},
                {"from": "B", "to": "C", "flow": 10}
            ],
            "colorFrom": "#36a2eb",
            "colorTo": "#ff6384",
            "colorMode": "gradient",
            "labels": {"A": "Alpha"},
            "priority": {"A": 0},
            "column": {"A": 0}
        }],
        "labels": []
        },
        "options": { "plugins": { "title": {"display": true, "text": "T"} } }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Sankey(_)));
    // 同じ文書を strict パーサも受理すること(parser↔schema パリティ)。
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict parser should accept sankey"
    );
}

#[test]
fn sankey_basic_parse() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[
        {"from":"A","to":"B","flow":10},
        {"from":"A","to":"C","flow":5},
        {"from":"B","to":"C","flow":10},
        {"from":"C","to":"D","flow":15}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        fulgur_chart::ir::ChartKind::Sankey { .. }
    ));
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].links.len(), 4);
    assert_eq!(spec.series[0].links[0].from, "A");
    assert_eq!(spec.series[0].links[0].flow, 10.0);
}

#[test]
fn sankey_defaults_match_chartjs() {
    use fulgur_chart::ir::{ChartKind, Color, SankeyColorMode, SankeyModeX, SankeySize};
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    let ChartKind::Sankey {
        color_from,
        color_to,
        color_mode,
        alpha,
        node_width,
        node_padding,
        mode_x,
        size,
        border_width,
        ..
    } = spec.kind
    else {
        panic!()
    };
    assert_eq!(
        color_from,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0
        }
    ); // 'red'
    assert_eq!(
        color_to,
        Color {
            r: 0,
            g: 128,
            b: 0,
            a: 1.0
        }
    ); // 'green'
    assert_eq!(color_mode, SankeyColorMode::Gradient);
    assert!((alpha - 0.5).abs() < 1e-9);
    assert_eq!(node_width, 10.0);
    assert_eq!(node_padding, 10.0);
    assert_eq!(mode_x, SankeyModeX::Edge);
    assert_eq!(size, SankeySize::Max);
    assert_eq!(border_width, 1.0);
}

#[test]
fn sankey_rejects_non_finite_flow() {
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":"x"}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn sankey_strict_rejects_unknown_key() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}],"bogus":1}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn sankey_options_override() {
    use fulgur_chart::ir::{ChartKind, Color, SankeyColorMode, SankeyModeX, SankeySize};
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":3}],
        "colorFrom":"#102030",
        "colorTo":"#405060",
        "colorMode":"from",
        "alpha":0.25,
        "borderColor":"#708090",
        "borderWidth":2.5,
        "color":"#a0b0c0",
        "nodeWidth":14,
        "nodePadding":8,
        "modeX":"even",
        "size":"min",
        "labels":{"A":"Alpha"},
        "priority":{"A":0},
        "column":{"A":2}
    }]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    let ChartKind::Sankey {
        color_from,
        color_to,
        color_mode,
        alpha,
        node_width,
        node_padding,
        mode_x,
        size,
        border,
        border_width,
        label_color,
        labels,
        priority,
        columns,
    } = spec.kind
    else {
        panic!()
    };
    assert_eq!(
        color_from,
        Color {
            r: 16,
            g: 32,
            b: 48,
            a: 1.0
        }
    );
    assert_eq!(
        color_to,
        Color {
            r: 64,
            g: 80,
            b: 96,
            a: 1.0
        }
    );
    assert_eq!(color_mode, SankeyColorMode::From);
    assert!((alpha - 0.25).abs() < 1e-9);
    assert_eq!(node_width, 14.0);
    assert_eq!(node_padding, 8.0);
    assert_eq!(mode_x, SankeyModeX::Even);
    assert_eq!(size, SankeySize::Min);
    assert_eq!(
        border,
        Color {
            r: 112,
            g: 128,
            b: 144,
            a: 1.0
        }
    );
    assert_eq!(border_width, 2.5);
    assert_eq!(
        label_color,
        Color {
            r: 160,
            g: 176,
            b: 192,
            a: 1.0
        }
    );
    assert_eq!(labels.get("A").map(String::as_str), Some("Alpha"));
    assert_eq!(priority.get("A"), Some(&0.0));
    assert_eq!(columns.get("A"), Some(&2usize));

    // colorMode "to" → To も検証(From↔To のスワップ回帰を捕捉)。
    let json_to = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}],"colorMode":"to"}]}}"#;
    let spec_to = chartjs::parse(json_to, false).unwrap();
    let ChartKind::Sankey { color_mode, .. } = spec_to.kind else {
        panic!()
    };
    assert_eq!(color_mode, SankeyColorMode::To);
}

#[test]
fn sankey_rejects_negative_flow() {
    // 数値として有効だが負の flow は guard で弾く(deserialize は通過する)。
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":-5}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn sankey_schema_and_parser_both_reject_datalabels() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // sankey は datalabels を持たない。schema(SankeyPlugins)と strict パーサが共に拒否し、
    // 「schema 受理 → strict 拒否」のパリティ破れを起こさないこと。
    let json = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"datalabels":{}}}}"##;
    assert!(
        serde_json::from_str::<ChartJsSpec>(json).is_err(),
        "schema は sankey の plugins.datalabels を拒否すべき"
    );
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict パーサも plugins.datalabels を拒否すべき"
    );
    // title は schema・parser とも受理する(正常系の確認)。
    let ok = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"title":{"display":true,"text":"T"}}}}"##;
    assert!(matches!(
        serde_json::from_str::<ChartJsSpec>(ok).unwrap(),
        ChartJsSpec::Sankey(_)
    ));
    assert!(chartjs::parse(ok, true).is_ok());
}

#[test]
fn sankey_schema_and_parser_both_reject_legend() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // sankey は legend を描画しないため契約から外す。schema・strict パーサ双方が拒否し、
    // 「strict が受理するのに描画では無視される」silent-drop を避ける。
    let json = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"legend":{"display":true}}}}"##;
    assert!(
        serde_json::from_str::<ChartJsSpec>(json).is_err(),
        "schema は sankey の plugins.legend を拒否すべき"
    );
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict パーサも plugins.legend を拒否すべき"
    );
}

#[test]
fn sankey_rejects_negative_node_width() {
    // 負の nodeWidth は <rect width="-5"> 等の不正 SVG を生むため parse で弾く。
    let json = r#"{"type":"sankey","data":{"datasets":[{"nodeWidth":-5,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
    // 非有限も拒否。
    let nan = r#"{"type":"sankey","data":{"datasets":[{"borderWidth":1e400,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(nan, false).is_err());
}

#[test]
fn sankey_rejects_huge_node_padding() {
    // 巨大な有限 nodePadding は layout の (max_y/height)*node_padding で ∞ に overflow し
    // 図形を潰すため、canvas 最大寸法を超える値は parse で弾く。
    let json = r#"{"type":"sankey","data":{"datasets":[{"nodePadding":1e308,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
    // 妥当な範囲(canvas 上限以下)は受理。
    let ok = r#"{"type":"sankey","data":{"datasets":[{"nodePadding":20,"nodeWidth":15,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(ok, false).is_ok());
}

#[test]
fn sankey_preserves_dataset_label() {
    // 他のパーサと同様、dataset の label を Series.name に保持する。
    let json = r#"{"type":"sankey","data":{"datasets":[{"label":"Energy","data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].name, "Energy");
}

#[test]
fn sankey_rejects_unknown_enum_values() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // colorMode/modeX/size のタイポは silent default にせず、schema・parser とも拒否する。
    for (field, bad) in [("colorMode", "form"), ("modeX", "edeg"), ("size", "mn")] {
        let json = format!(
            r#"{{"type":"sankey","data":{{"datasets":[{{"{field}":"{bad}","data":[{{"from":"A","to":"B","flow":1}}]}}]}}}}"#
        );
        assert!(
            serde_json::from_str::<ChartJsSpec>(&json).is_err(),
            "schema should reject {field}={bad}"
        );
        assert!(
            chartjs::parse(&json, false).is_err(),
            "parser should reject {field}={bad}"
        );
    }
}

#[test]
fn sankey_accepts_null_options() {
    // schema は optional フィールドを nullable として描くため、parser も明示 null を
    // 既定として受理し、schema-valid な spec が strict で落ちないようにする。
    let null_opts = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":null}"#;
    assert!(chartjs::parse(null_opts, true).is_ok());
    let null_plugins = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":null}}"#;
    assert!(chartjs::parse(null_plugins, true).is_ok());
}

// ──────────────────────────────────────────────
// fulgur-chart-tgb: top-level width/height は全チャート種別で
// ChartSpec.width/height に反映される(ハードコード 800x450 の置換)。
// ──────────────────────────────────────────────

#[test]
fn main_path_honors_top_level_width_height() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 640.0);
    assert_eq!(spec.height, 360.0);
}

#[test]
fn main_path_defaults_width_height_when_absent() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 800.0);
    assert_eq!(spec.height, 450.0);
}

#[test]
fn strict_allows_top_level_width_height() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn special_paths_honor_top_level_width_height() {
    // matrix/treemap/sankey/gauge は主要 parse とは別パス。progress は主要 parse 経由だが
    // 専用 strict check を持つため strict 受理も併せて検証する。
    let cases = [
        r#"{"type":"matrix","data":{"datasets":[{"data":[{"x":"A","y":"X","v":1}]}]},"width":640,"height":360}"#,
        r#"{"type":"treemap","data":{"datasets":[{"tree":[6,4,3,2,1]}]},"width":640,"height":360}"#,
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":10}]}]},"width":640,"height":360}"#,
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],"backgroundColor":["green","yellow","red"]}]},"width":640,"height":360}"#,
        r#"{"type":"progress","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
    ];
    for json in cases {
        let spec = chartjs::parse(json, false).unwrap();
        assert_eq!(spec.width, 640.0, "non-strict width for {json}");
        assert_eq!(spec.height, 360.0, "non-strict height for {json}");
        assert!(
            chartjs::parse(json, true).is_ok(),
            "strict should accept top-level width/height for {json}"
        );
    }
}

#[test]
fn schema_accepts_top_level_width_height_all_kinds() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // 各 *Spec 構造体を最低 1 つカバーする(PieSpec=pie, OutlabeledPieSpec=outlabeledPie で代表)。
    let cases = [
        r#"{"type":"bar","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"line","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"pie","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2}]}]},"width":640,"height":360}"#,
        r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":10}]}]},"width":640,"height":360}"#,
        r#"{"type":"radar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"matrix","data":{"datasets":[{"data":[{"x":"A","y":"X","v":1}]}]},"width":640,"height":360}"#,
        r#"{"type":"treemap","data":{"datasets":[{"tree":[6]}]},"width":640,"height":360}"#,
        r#"{"type":"progress","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"boxplot","data":{"labels":["A"],"datasets":[{"data":[[10,25,50,75,90]]}]},"width":640,"height":360}"#,
        r#"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4]}]},"width":640,"height":360}"#,
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6]}]},"width":640,"height":360}"#,
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[70]}]},"width":640,"height":360}"#,
        r#"{"type":"outlabeledPie","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},"width":640,"height":360}"#,
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":10}]}]},"width":640,"height":360}"#,
    ];
    for json in cases {
        let r: Result<ChartJsSpec, serde_json::Error> = serde_json::from_str(json);
        assert!(
            r.is_ok(),
            "schema should accept top-level width/height for {json}: {:?}",
            r.err()
        );
    }
}

#[test]
fn rendered_svg_reflects_top_level_width_height() {
    // JSON → ChartSpec.width/height → Scene → SVG ルート属性まで一貫してサイズが届くことを
    // end-to-end で検証する(既定 800x450 ではなく指定値で描画される)。
    use fulgur_chart::render::render_chart;
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    let svg = render_chart(&chartjs::parse(json, false).unwrap());
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(svg.contains(r#"width="640""#), "{svg}");
    assert!(svg.contains(r#"height="360""#), "{svg}");
    assert!(svg.contains(r#"viewBox="0 0 640 360""#), "{svg}");
}

// ── decimation (options.plugins.decimation) ──

#[test]
fn decimation_defaults_to_enabled_minmax_when_absent() {
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#,
        false,
    )
    .unwrap();
    assert!(spec.decimation.enabled);
    assert_eq!(
        spec.decimation.algorithm,
        fulgur_chart::ir::DecimationAlgorithm::MinMax
    );
}

#[test]
fn decimation_explicit_disable_and_lttb() {
    let json = r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
        "options":{"plugins":{"decimation":{"enabled":false,"algorithm":"lttb","samples":300,"threshold":1000}}}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(!spec.decimation.enabled);
    assert_eq!(
        spec.decimation.algorithm,
        fulgur_chart::ir::DecimationAlgorithm::Lttb
    );
    assert_eq!(spec.decimation.samples, Some(300.0));
    assert_eq!(spec.decimation.threshold, Some(1000.0));
}

#[test]
fn decimation_invalid_algorithm_errors() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"algorithm":"bogus"}}}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn strict_accepts_decimation_keys() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"min-max","samples":100,"threshold":500}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_unknown_decimation_subkey() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"bogus":1}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn schema_strict_parity_decimation_line() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"lttb","samples":50,"threshold":200}}}}"#;
    // strict side: 厳格パーサも受理する。
    assert!(chartjs::parse(json, true).is_ok());
    // schema side: ChartJsSpec(internally tagged, deny_unknown_fields)でも受理されること。
    let spec: fulgur_chart::schema::ChartJsSpec = serde_json::from_str(json).unwrap();
    let decimation = match spec {
        fulgur_chart::schema::ChartJsSpec::Line(line) => line
            .options
            .and_then(|o| o.plugins)
            .and_then(|p| p.decimation),
        _ => panic!("expected line variant"),
    };
    assert!(decimation.is_some());
}

#[test]
fn schema_rejects_unknown_decimation_algorithm() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"algorithm":"bogus"}}}}"#;
    // strict side は不正 algorithm を拒否する。
    assert!(chartjs::parse(json, true).is_err());
    // schema side も enum 制約で拒否すること（value レベルの parity）。
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    assert!(serde_json::from_value::<fulgur_chart::schema::ChartJsSpec>(v).is_err());
}

#[test]
fn radar_scales_r_populates_radial_axis() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{"min":-10,"max":50,"suggestedMin":-20,"suggestedMax":80,"beginAtZero":true}}}}"##,
        false,
    ).unwrap();
    let r = spec.radial_axis.expect("radar should populate radial_axis");
    assert_eq!(r.min, Some(-10.0));
    assert_eq!(r.max, Some(50.0));
    assert_eq!(r.suggested_min, Some(-20.0));
    assert_eq!(r.suggested_max, Some(80.0));
    assert!(r.begin_at_zero);
}

#[test]
fn polar_area_scales_r_populates_radial_axis() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[10,20]}]},
             "options":{"scales":{"r":{"max":100}}}}"##,
        false,
    )
    .unwrap();
    let r = spec
        .radial_axis
        .expect("polarArea should populate radial_axis");
    assert_eq!(r.max, Some(100.0));
    assert!(r.begin_at_zero, "polarArea beginAtZero default true");
}

#[test]
fn radar_without_scales_leaves_radial_axis_none() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r#"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#,
        false,
    )
    .unwrap();
    assert!(
        spec.radial_axis.is_none(),
        "backward compat: no scales → None"
    );
}

#[test]
fn scales_r_axis_silently_accepted_in_non_strict() {
    // radar/polar chart で使う `scales.r` を非 strict で silently 通す(Chart.js 互換)。
    let json = r##"{
      "type":"line",
      "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"scales":{"r":{"beginAtZero":true}}}
    }"##;
    assert!(
        chartjs::parse(json, false).is_ok(),
        "non-strict should silently accept scales.r"
    );
}

#[test]
fn non_radial_charts_leave_radial_axis_none() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
             "options":{"scales":{"y":{"beginAtZero":true}}}}"##,
        false,
    )
    .unwrap();
    assert!(spec.radial_axis.is_none());
}

#[test]
fn strict_mode_allows_scales_r_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":{"min":0,"max":100,"suggestedMin":-5,"suggestedMax":120,"beginAtZero":true}}}}"##;
    chartjs::parse(json, true).expect("strict mode should accept scales.r on radar");
}

#[test]
fn strict_mode_allows_scales_r_on_polar_area() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
        "options":{"scales":{"r":{"max":50}}}}"##;
    chartjs::parse(json, true).expect("strict mode should accept scales.r on polarArea");
}

#[test]
fn strict_mode_rejects_scales_r_on_bar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"scales":{"r":{"min":0}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("r") && err.contains("scales"), "err: {err}");
}

#[test]
fn strict_mode_rejects_scales_r_on_doughnut() {
    // doughnut は pie と PieSpec を共有する。scales.r は radar/polarArea 専用。
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"doughnut","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"scales":{"r":{"min":0}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("r") && err.contains("scales"), "err: {err}");
}

#[test]
fn strict_mode_rejects_scales_r_typo_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":{"beginAtZeroo":true}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("beginAtZeroo"), "err: {err}");
}

#[test]
fn strict_mode_rejects_scales_xy_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"x":{"min":0}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("x") && err.contains("scales"), "err: {err}");
}

#[test]
fn strict_mode_rejects_non_object_scales_r_on_radar() {
    // Codex Fix 7: axis 値が object でない (例: "r": 5) 場合は strict で拒否する。
    // 従来は as_object() の None 分岐で無音スキップされていたため typo/型ミスが漏れていた。
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":5}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("r") && err.contains("object"), "err: {err}");
}

#[test]
fn strict_mode_rejects_non_object_scales_y_on_bar() {
    // Fix 7 は cartesian にも適用される。
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"scales":{"y":"foo"}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("y") && err.contains("object"), "err: {err}");
}

#[test]
fn scales_r_axis_rejected_in_strict() {
    let json = r##"{
      "type":"line",
      "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"scales":{"r":{"beginAtZero":true}}}
    }"##;
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict should reject unknown scale axis"
    );
}

#[test]
fn strict_mode_treats_null_scales_axis_as_absent() {
    // Codex Fix 10: `options.scales.<axis>: null` は「軸未指定」と同義。
    // schema 側 (`RadialLinearScales.r` / `BarScales.y`) は `Option<_>` なので
    // null は None に deserialize される。optional フィールドを nullable に
    // serialize するクライアントが strict で落ちないようにする。
    let radar = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":null}}}"##;
    chartjs::parse(radar, true).expect("strict should treat scales.r: null as absent");

    let bar = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"scales":{"y":null}}}"##;
    chartjs::parse(bar, true).expect("strict should treat scales.y: null as absent");

    // null 軸は radial_axis を populate しない (未指定と同じ)。
    let spec = chartjs::parse(radar, false).unwrap();
    assert!(spec.radial_axis.is_none(), "null 軸は未指定と同義");

    // schema 側も同じ結論に到達すること (value レベルの parity)。
    for json in [radar, bar] {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        serde_json::from_value::<fulgur_chart::schema::ChartJsSpec>(v)
            .expect("schema should deserialize a null axis as unset");
    }

    // 非 object かつ非 null (数値・文字列) は従来通り拒否する。
    let bad = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":5}}}"##;
    assert!(chartjs::parse(bad, true).is_err(), "非 object は拒否");
}

#[test]
fn empty_scales_r_does_not_populate_radial_axis() {
    // Codex Fix 9: ドメインキーを 1 つも持たない `scales.r` は no-op。
    // 空 object でも `Some(RadialAxis)` を返すと layout が override 経路に入り、
    // 既定の nice ドメインが raw データ範囲に置き換わってしまう。
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{}}}}"##,
        false,
    )
    .unwrap();
    assert!(spec.radial_axis.is_none(), "空の scales.r は no-op");

    // 視覚キーのみ (非 strict では silently 無視される) も同様に no-op。
    let spec = chartjs::parse(
        r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
             "options":{"scales":{"r":{"ticks":{"display":false}}}}}"##,
        false,
    )
    .unwrap();
    assert!(
        spec.radial_axis.is_none(),
        "視覚キーのみの scales.r は no-op"
    );

    // 逆に、ドメインキーが 1 つでもあれば populate される。
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{"beginAtZero":false}}}}"##,
        false,
    )
    .unwrap();
    let r = spec
        .radial_axis
        .expect("beginAtZero 明示時は populate される");
    assert!(!r.begin_at_zero);
}

#[test]
fn non_radial_charts_ignore_malformed_scales_r_in_non_strict() {
    // Codex Fix 17 のリグレッションテスト。
    // `RawScales.r` を `RawRadialAxis` に型付けすると、chart kind が確定する前に
    // 検証が走り、非 radial チャートに紛れ込んだ無関係な `scales.r` が非 strict でも
    // deserialize エラーになる。main では未知キーとして silently 無視されていたので
    // これは後退にあたる。kind が radial のときだけ typed に解釈すること。
    for json in [
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
             "options":{"scales":{"r":5}}}"##,
        r##"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
             "options":{"scales":{"r":"nonsense"}}}"##,
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
             "options":{"scales":{"r":{"min":"not-a-number"}}}}"##,
    ] {
        let spec = chartjs::parse(json, false)
            .unwrap_or_else(|e| panic!("非 strict では silently 無視されるべき: {e} / {json}"));
        assert!(
            spec.radial_axis.is_none(),
            "非 radial チャートは radial_axis を持たない"
        );
    }
}

#[test]
fn radial_charts_ignore_malformed_scales_r_in_non_strict() {
    // radial チャート側でも、型不一致の `scales.r` は非 strict では silently 無視する
    // (Chart.js 互換)。strict モードでは check_unknown_keys が拒否する。
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":5}}}"##,
        false,
    )
    .expect("非 strict では silently 無視されるべき");
    assert!(spec.radial_axis.is_none());

    // strict では従来通り拒否されること (パリティ確認)。
    assert!(
        chartjs::parse(
            r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
                 "options":{"scales":{"r":5}}}"##,
            true,
        )
        .is_err(),
        "strict では非 object の scales.r を拒否する"
    );
}

#[test]
fn strict_mode_rejects_wrong_typed_scales_r_field() {
    // Codex Fix 20 のリグレッションテスト。
    // `check_unknown_keys` はキー名しか見ないので `{"max": "100"}` のように
    // 「キーは正しいが型が違う」入力は素通りする。typed deserialize の失敗を
    // `.ok()` で握り潰すと、strict なのに既定ドメインで描画されてしまう。
    for json in [
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{"max":"100"}}}}"##,
        r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
             "options":{"scales":{"r":{"beginAtZero":"yes"}}}}"##,
    ] {
        let err =
            chartjs::parse(json, true).expect_err("strict は scales.r の型不一致を拒否すべき");
        assert!(err.contains("scales.r"), "err: {err}");
    }

    // 非 strict では従来通り silently 無視して既定ドメインで描画する (Chart.js 互換)。
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{"max":"100"}}}}"##,
        false,
    )
    .expect("非 strict は silently 無視する");
    assert!(spec.radial_axis.is_none());
}

// ---------------------------------------------------------------------------
// Logarithmic scale (options.scales.<axis>.type == "logarithmic")
//
// `frontend/chartjs.rs` の `mod tests` (white-box) 側で is_logarithmic/masking
// の分岐網羅は既に取れている(vertical/horizontal bar・line・pie・mixed・strict
// allow-list の "type" 単体受理)。ここでは公開 API 経由でしか見えない挙動、
// つまり (a) 対数軸が他の軸オプション(title/grid/suggestedMin)と共存しても
// 壊れないこと、(b) x 軸側(横棒)のマスキング分岐、(c) strict の allow-list が
// 軸種別(radial vs cartesian)で非対称であること、(d) 未知の type 文字列が
// strict でもエラーにならないこと、を確認する。
// ---------------------------------------------------------------------------

#[test]
fn cartesian_axis_min_max_populate_ir_for_x_and_y() {
    let spec = chartjs::parse(
        r##"{
          "type":"bar",
          "data":{"labels":["a","b"],"datasets":[{"data":[10,20]}]},
          "options":{"scales":{
            "x":{"min":-5,"max":5},
            "y":{"min":10,"max":90}
          }}
        }"##,
        true,
    )
    .expect("strict mode accepts numeric cartesian min/max");

    assert_eq!((spec.x_axis.min, spec.x_axis.max), (Some(-5.0), Some(5.0)));
    assert_eq!((spec.y_axis.min, spec.y_axis.max), (Some(10.0), Some(90.0)));
}

#[test]
fn strict_end_to_end_bar_chart_with_logarithmic_y_axis_and_axis_options() {
    // 実運用に近い chart.js JSON: 対数 y 軸に title/grid/suggestedMin/suggestedMax
    // を同居させ、strict でも通ること・IR 側で両方がちゃんと共存することを検証する。
    let json = r##"{
      "type":"bar",
      "data":{"labels":["a","b","c"],"datasets":[{"label":"売上","data":[1,10,100]}]},
      "options":{
        "scales":{
          "y":{
            "type":"logarithmic",
            "title":{"display":true,"text":"件数"},
            "grid":{"display":false},
            "suggestedMin":1,
            "suggestedMax":1000
          }
        }
      }
    }"##;
    let spec = chartjs::parse(json, true).expect("strict は既知キーの組み合わせを受理する");
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Logarithmic);
    let title = spec.y_axis.title.as_ref().expect("title should be Some");
    assert_eq!(title.text, "件数");
    assert!(!spec.y_axis.grid.display);
    assert_eq!(spec.y_axis.suggested_min, Some(1.0));
    assert_eq!(spec.y_axis.suggested_max, Some(1000.0));
}

#[test]
fn horizontal_bar_logarithmic_x_axis_preserves_negative_values() {
    // 縦棒(y が値軸)の負値保持は white-box テストで既に確認済み。
    // ここでは横棒(indexAxis:"y" → x が値軸)側の分岐、つまり x_axis_is_log の
    // OR 経路でも入力値をそのまま IR に保持することを公開 API から確認する。
    let json = r##"{
      "type":"bar",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[1,-5,10]}]},
      "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}
    }"##;
    let spec = chartjs::parse(json, false).expect("parse ok");
    assert_eq!(spec.x_axis.scale_kind, ScaleKind::Logarithmic);
    let values = &spec.series[0].values;
    assert_eq!(values[1], -5.0, "negative value should remain in the IR");
    assert_eq!(values[0], 1.0);
    assert_eq!(values[2], 10.0);
}

#[test]
fn strict_mode_rejects_scales_r_type_key_on_radar() {
    // Task 4 は cartesian (x/y) の allow-list にのみ "type" を追加した。radial (r) の
    // allow-list ([min, max, suggestedMin, suggestedMax, beginAtZero], `RadialLinearAxisOptions`
    // のフィールド集合と一致)には含まれていないため、strict モードで scales.r.type は
    // x/y とは非対称に拒否される(v1 は radial 軸の対数化をそもそもサポートしない)。
    // 拒否は値ではなくキーの存在で決まる ("type":"logarithmic" でも "type":"linear" でも
    // 同じエラーになる) ため、テスト名・コメントはキー起因であることを明示する。
    let json = r##"{
      "type":"radar",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
      "options":{"scales":{"r":{"type":"logarithmic"}}}
    }"##;
    let err = chartjs::parse(json, true).expect_err("radial の type は strict allow-list に無い");
    assert!(err.contains("options.scales.r.type"), "err: {err}");

    // 非 strict では `RawScales.r` が生の serde_json::Value のまま保持され (chartjs.rs
    // 冒頭コメント参照)、`type` はドメインキー(min/max/suggestedMin/suggestedMax/
    // beginAtZero)ではないので `empty_scales_r_does_not_populate_radial_axis` の
    // "視覚キーのみ" ケースと同じく no-op(radial_axis は populate されない)。
    let spec = chartjs::parse(json, false).expect("非 strict は silently 無視する");
    assert!(
        spec.radial_axis.is_none(),
        "type 単体はドメインキーではないので no-op"
    );
}

#[test]
fn unknown_scale_type_value_is_accepted_in_strict_mode_and_defaults_to_linear() {
    // `AxisOptions.type` は closed enum ではなく Option<String> (schema/common.rs 参照)。
    // 既存 Chart.js JSON が "category"/"time" のような非対数値を明示することは多く、
    // strict モードでもエラーにせず素通りし、IR 側では Linear 既定に倒れることを
    // 公開 API 経由で確認する。
    let json = r##"{
      "type":"bar",
      "data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"type":"category"}}}
    }"##;
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict は未知の type 値をエラーにしない"
    );
    let spec = chartjs::parse(json, false).expect("parse ok");
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Linear);
}

#[test]
fn scatter_logarithmic_axes_are_accepted() {
    let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4}]}]},
      "options":{"scales":{"x":{"type":"logarithmic"},"y":{"type":"logarithmic"}}}}"#;
    let spec = chartjs::parse(json, true).expect("strict mode should accept logarithmic axes");
    assert_eq!(spec.x_axis.scale_kind, ScaleKind::Logarithmic);
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Logarithmic);
}

#[test]
fn horizontal_bar_border_radius_matches_svg_and_png_corners() {
    let json = r##"{
      "type":"bar",
      "data":{"labels":["positive","negative"],"datasets":[
        {"data":[10,-10],"backgroundColor":"#ff0000","borderRadius":16}
      ]},
      "options":{"indexAxis":"y"},
      "width":400,
      "height":240
    }"##;
    let spec = chartjs::parse(json, false).unwrap();
    let fill = spec.series[0].fill_at(0);
    let fill_hex = format!("#{:02x}{:02x}{:02x}", fill.r, fill.g, fill.b);
    let svg = fulgur_chart::render::render_chart(&spec);
    let bar_path = svg
        .split("<path ")
        .find(|tag| tag.contains(&format!(r#"fill="{fill_hex}""#)))
        .expect("rounded bar should be an SVG path with the dataset fill");
    assert!(
        bar_path.contains(" C "),
        "bar path should contain cubic corners: {bar_path}"
    );

    let png = fulgur_chart::raster_direct::render_chart_to_png(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("rendered PNG should decode");

    // Use an otherwise identical square chart to obtain the exact bar rectangles, then sample
    // inside each extreme corner of the rounded render. Radius-only settings preserve geometry.
    let square_json = json.replace(r#","borderRadius":16"#, "");
    let square_spec = chartjs::parse(&square_json, false).unwrap();
    let measurer = fulgur_chart::text::TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();
    let square_scene = fulgur_chart::layout::bar::build(&square_spec, &measurer);
    let bars: Vec<_> = square_scene
        .items
        .iter()
        .filter_map(|prim| match prim {
            fulgur_chart::scene::Prim::Rect {
                x,
                y,
                w,
                h,
                fill: rect_fill,
            } if *rect_fill == fill => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 2, "expected one positive and one negative bar");

    let is_red = |x: f64, y: f64| {
        let pixel_x = (x + 0.5).floor() as u32;
        let pixel_y = (y + 0.5).floor() as u32;
        let pixel = pixmap.pixel(pixel_x, pixel_y).expect("sample inside PNG");
        pixel.red() == 255 && pixel.green() == 0 && pixel.blue() == 0 && pixel.alpha() == 255
    };
    let margin = 3.0; // inner enough to avoid edge antialiasing, outer to the radius-16 arc
    let (x, y, w, h) = bars[0];
    assert!(
        is_red(x + margin, y + margin),
        "positive bar base-side top-left corner stays square"
    );
    assert!(
        is_red(x + margin, y + h - margin),
        "positive bar base-side bottom-left corner stays square"
    );
    assert!(
        !is_red(x + w - margin, y + margin),
        "positive bar value-side top-right corner is rounded"
    );
    assert!(
        !is_red(x + w - margin, y + h - margin),
        "positive bar value-side bottom-right corner is rounded"
    );

    let (x, y, w, h) = bars[1];
    assert!(
        !is_red(x + margin, y + margin),
        "negative bar value-side top-left corner is rounded"
    );
    assert!(
        !is_red(x + margin, y + h - margin),
        "negative bar value-side bottom-left corner is rounded"
    );
    assert!(
        is_red(x + w - margin, y + margin),
        "negative bar base-side top-right corner stays square"
    );
    assert!(
        is_red(x + w - margin, y + h - margin),
        "negative bar base-side bottom-right corner stays square"
    );
}

#[test]
fn pie_schema_roundtrip_preserves_cutout_and_dataset_arc_options() {
    use fulgur_chart::schema::ChartJsSpec;

    let cases = [
        r#"{"type":"pie","data":{"datasets":[{"data":[1.0,2.0],"spacing":2.0,"offset":5.0,"borderRadius":4.0}]},"options":{"cutout":24.0}}"#,
        r#"{"type":"doughnut","data":{"datasets":[{"data":[1.0,2.0],"spacing":3.0,"offset":[1.0,3.0],"borderRadius":[2.0,{"outerStart":4.0,"outerEnd":3.0,"innerStart":2.0,"innerEnd":1.0}]}]},"options":{"cutout":"25%"}}"#,
    ];

    for json in cases {
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        let spec: ChartJsSpec = serde_json::from_value(expected.clone()).unwrap();
        let actual = serde_json::to_value(spec).unwrap();
        assert_eq!(actual, expected);
        assert!(chartjs::parse(json, true).is_ok());
    }

    let invalid_cutout =
        r#"{"type":"pie","data":{"datasets":[{"data":[1]}]},"options":{"cutout":"half"}}"#;
    assert!(serde_json::from_str::<ChartJsSpec>(invalid_cutout).is_err());
}

#[test]
fn pie_arc_options_are_rejected_by_schema_and_strict_parser_on_other_arc_charts() {
    use fulgur_chart::schema::ChartJsSpec;

    let invalid = [
        r#"{"type":"polarArea","data":{"datasets":[{"data":[1,2],"spacing":2} ]}}"#,
        r#"{"type":"outlabeledPie","data":{"datasets":[{"data":[1,2],"spacing":2}]}}"#,
        r#"{"type":"outlabeledDoughnut","data":{"datasets":[{"data":[1,2],"spacing":2}]}}"#,
    ];
    for json in invalid {
        assert!(
            serde_json::from_str::<ChartJsSpec>(json).is_err(),
            "schema should reject pie-only arc options for {json}"
        );
        assert!(
            chartjs::parse(json, true).is_err(),
            "strict parser should reject pie-only arc options for {json}"
        );
    }
}

#[test]
fn strict_rejects_pie_only_cutout_and_arc_options_on_bar() {
    let cases = [
        r#"{"type":"bar","data":{"datasets":[{"data":[1]}]},"options":{"cutout":20}}"#,
        r#"{"type":"bar","data":{"datasets":[{"data":[1],"spacing":2}]}}"#,
        r#"{"type":"bar","data":{"datasets":[{"data":[1],"offset":2}]}}"#,
    ];
    for json in cases {
        assert!(chartjs::parse(json, true).is_err(), "should reject {json}");
    }
}

#[test]
fn pie_arc_geometry_is_deterministic_in_svg_and_png() {
    let json = r#"{"type":"doughnut","data":{"labels":["A","B"],"datasets":[{"data":[2,1],"spacing":4,"offset":[0,7],"borderRadius":[{"outerStart":12,"outerEnd":10,"innerStart":8,"innerEnd":6},5]}]},"options":{"cutout":"35%"}}"#;
    let spec = chartjs::parse(json, true).unwrap();
    let svg_first = fulgur_chart::render::render_chart(&spec);
    let svg_second = fulgur_chart::render::render_chart(&spec);
    assert_eq!(svg_first, svg_second);
    assert!(svg_first.contains("<path") && !svg_first.contains("NaN"));

    let png_first = fulgur_chart::raster_direct::render_chart_to_png(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .unwrap();
    let png_second = fulgur_chart::raster_direct::render_chart_to_png(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .unwrap();
    assert_eq!(png_first, png_second);
    tiny_skia::Pixmap::decode_png(&png_first).expect("pie PNG should decode");
}
