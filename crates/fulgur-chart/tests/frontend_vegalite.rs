use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::{ChartKind, LegendPos, LineInterpolation, SizeMode, XPositions};
use fulgur_chart::palette::VEGALITE_PALETTE;
use fulgur_chart::temporal::parse_rfc3339_millis;

const BAR_SPEC: &str = r#"{
    "mark": "bar",
    "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5},{"cat":"C","val":2}]},
    "encoding": {"x": {"field":"cat","type":"nominal"}, "y": {"field":"val","type":"quantitative"}}
}"#;

const DOGFOOD_SHAPE: &str = r##"{
  "$schema":"https://vega.github.io/schema/vega-lite/v5.json",
  "title":"qtest nightly trend",
  "width":720,
  "height":320,
  "background":"white",
  "data":{"values":[
    {"timestamp":"2026-07-21T19:21:53Z","metric":"regressions","value":0}
  ]},
  "mark":{"type":"line","point":true,"interpolate":"monotone"},
  "encoding":{
    "x":{"field":"timestamp","type":"temporal","title":"date"},
    "y":{"field":"value","type":"quantitative","title":"subtests"},
    "color":{"field":"metric","type":"nominal","title":"metric",
             "scale":{"scheme":"tableau10"}}
  },
  "config":{"view":{"stroke":null},
            "axis":{"grid":true,"gridOpacity":0.15}}
}"##;

const DOGFOOD_MULTI_SERIES: &str = r##"{
  "mark":{"type":"line","point":true,"interpolate":"monotone"},
  "data":{"values":[
    {"timestamp":"2026-07-01T19:00:00Z","metric":"regressions","value":3},
    {"timestamp":"2026-06-29T19:00:00Z","metric":"candidates","value":2},
    {"timestamp":"2026-07-01T19:00:00Z","metric":"allowlist","value":6},
    {"timestamp":"2026-06-29T19:00:00Z","metric":"regressions","value":1},
    {"timestamp":"2026-07-01T19:00:00Z","metric":"candidates","value":5},
    {"timestamp":"2026-06-29T19:00:00Z","metric":"allowlist","value":4}
  ]},
  "encoding":{
    "x":{"field":"timestamp","type":"temporal","title":"date"},
    "y":{"field":"value","type":"quantitative","title":"subtests"},
    "color":{"field":"metric","type":"nominal","title":"metric"}
  }
}"##;

const CATEGORICAL_LINE_SHAPE: &str = r#"{
  "mark":"line",
  "data":{"values":[{"category":"a","value":1}]},
  "encoding":{
    "x":{"field":"category","type":"nominal"},
    "y":{"field":"value","type":"quantitative"}
  }
}"#;

#[test]
fn temporal_line_sorts_x_and_nominal_color_domain() {
    let spec = vegalite::parse(DOGFOOD_MULTI_SERIES, true).unwrap();
    let expected = vec![
        parse_rfc3339_millis("timestamp", "2026-06-29T19:00:00Z").unwrap(),
        parse_rfc3339_millis("timestamp", "2026-07-01T19:00:00Z").unwrap(),
    ];
    assert_eq!(
        spec.x_positions,
        XPositions::Temporal {
            unix_millis: expected
        }
    );
    assert_eq!(
        spec.series
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["allowlist", "candidates", "regressions"]
    );
    assert_eq!(
        spec.series.iter().map(|s| s.stroke[0]).collect::<Vec<_>>(),
        VEGALITE_PALETTE[..3]
    );
    assert_eq!(spec.series[0].values, vec![4.0, 6.0]);
}

#[test]
fn temporal_line_aggregates_offset_equivalent_timestamps() {
    let json = r#"{
        "mark":"line",
        "data":{"values":[
            {"timestamp":"2026-07-01T19:00:00Z","metric":"a","value":2},
            {"timestamp":"2026-07-02T04:00:00+09:00","metric":"a","value":3}
        ]},
        "encoding":{"x":{"field":"timestamp","type":"temporal"},"y":{"field":"value"},"color":{"field":"metric"}}
    }"#;
    let spec = vegalite::parse(json, true).unwrap();
    assert_eq!(spec.categories, vec!["2026-07-01T19:00:00Z"]);
    assert_eq!(spec.series[0].values, vec![5.0]);
    assert_eq!(
        spec.x_positions,
        XPositions::Temporal {
            unix_millis: vec![parse_rfc3339_millis("timestamp", "2026-07-01T19:00:00Z").unwrap()]
        }
    );
}

#[test]
fn temporal_line_rejects_non_finite_duplicate_aggregate() {
    let json = r#"{
        "mark":"line",
        "data":{"values":[
            {"timestamp":"2026-07-01T19:00:00Z","metric":"a","value":1e308},
            {"timestamp":"2026-07-02T04:00:00+09:00","metric":"a","value":1e308}
        ]},
        "encoding":{"x":{"field":"timestamp","type":"temporal"},"y":{"field":"value"},"color":{"field":"metric"}}
    }"#;

    let err = vegalite::parse(json, true).unwrap_err();
    assert_eq!(err, "temporal line aggregate must be finite");
}

#[test]
fn temporal_line_without_color_builds_one_named_series() {
    let json = r#"{
        "mark":"line",
        "data":{"values":[
            {"timestamp":"2026-07-01T00:00:00Z","value":2},
            {"timestamp":"2026-07-02T00:00:00Z","value":3}
        ]},
        "encoding":{
            "x":{"field":"timestamp","type":"temporal"},
            "y":{"field":"value","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, true).unwrap();
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].name, "");
    assert_eq!(spec.series[0].values, vec![2.0, 3.0]);
    assert_eq!(spec.legend, LegendPos::None);
    assert_eq!(spec.legend_title, None);
}

#[test]
fn temporal_line_rejects_invalid_timestamp_values() {
    for timestamp in [r#""not-a-date""#, "null", "42"] {
        let json = format!(
            r#"{{"mark":"line","data":{{"values":[{{"timestamp":{timestamp},"value":1}}]}},"encoding":{{"x":{{"field":"timestamp","type":"temporal"}},"y":{{"field":"value"}}}}}}"#
        );
        let err = vegalite::parse(&json, true).unwrap_err();
        assert!(err.contains("timestamp"), "unexpected error: {err}");
        assert!(err.is_ascii(), "error must be English: {err}");
    }
}

#[test]
fn temporal_line_errors_are_english_in_strict_and_non_strict_modes() {
    let cases = [
        r#"{"timestamp":null,"value":1}"#,
        r#"{"timestamp":42,"value":1}"#,
        r#"{"value":1}"#,
        r#"{"timestamp":"2026-07-01T00:00:00Z","value":null}"#,
        r#"{"timestamp":"2026-07-01T00:00:00Z","value":"one"}"#,
        r#"{"timestamp":"2026-07-01T00:00:00Z"}"#,
    ];
    for strict in [false, true] {
        for record in cases {
            let json = format!(
                r#"{{"mark":"line","data":{{"values":[{record}]}},"encoding":{{"x":{{"field":"timestamp","type":"temporal"}},"y":{{"field":"value","type":"quantitative"}}}}}}"#
            );
            let err = vegalite::parse(&json, strict).unwrap_err();
            assert!(err.is_ascii(), "strict={strict}: {err}");
            assert!(err.len() < 240, "strict={strict}: {err}");
            assert!(
                err.contains("timestamp") || err.contains("value"),
                "strict={strict}: {err}"
            );
        }
    }
}

#[test]
fn temporal_line_errors_bound_long_fields_and_unknown_key_paths() {
    let long_field = format!("field-{}-FIELD_TAIL", "x".repeat(600));
    let field_json = format!(
        r#"{{"mark":"line","data":{{"values":[{{"value":1}}]}},"encoding":{{"x":{{"field":"{long_field}","type":"temporal"}},"y":{{"field":"value"}}}}}}"#
    );
    let field_err = vegalite::parse(&field_json, false).unwrap_err();
    assert!(field_err.len() < 240, "{field_err}");
    assert!(!field_err.contains("FIELD_TAIL"), "{field_err}");

    let long_key = format!("key-{}-KEY_TAIL", "y".repeat(600));
    let key_json = DOGFOOD_SHAPE.replace(
        r#""title":"date""#,
        &format!(r#""title":"date","{long_key}":true"#),
    );
    let key_err = vegalite::parse(&key_json, true).unwrap_err();
    assert!(key_err.len() < 240, "{key_err}");
    assert!(!key_err.contains("KEY_TAIL"), "{key_err}");
}

#[test]
fn temporal_line_custom_limits_reject_dense_product_before_allocation() {
    let limits = fulgur_chart::guard::InputLimits {
        max_series: 1,
        max_categories: 10,
        max_categorical_primitives: 10,
        max_total_data_points: 10,
        ..fulgur_chart::guard::InputLimits::default()
    };
    let err = vegalite::parse_with_limits(DOGFOOD_MULTI_SERIES, true, &limits).unwrap_err();
    assert!(err.contains("series"), "{err}");
    assert!(err.contains("pre-allocation"), "{err}");
}

#[test]
fn temporal_line_rejects_sparse_pairs_after_sorting() {
    let json = r#"{
        "mark":"line",
        "data":{"values":[
            {"timestamp":"2026-07-02T00:00:00Z","metric":"a","value":1},
            {"timestamp":"2026-07-01T00:00:00Z","metric":"b","value":2},
            {"timestamp":"2026-07-01T00:00:00Z","metric":"a","value":3}
        ]},
        "encoding":{"x":{"field":"timestamp","type":"temporal"},"y":{"field":"value"},"color":{"field":"metric"}}
    }"#;
    let err = vegalite::parse(json, true).unwrap_err();
    assert!(err.contains("sparse"), "unexpected error: {err}");
}

#[test]
fn temporal_line_populates_positioned_ir_metadata() {
    let spec = vegalite::parse(DOGFOOD_SHAPE, true).unwrap();
    assert_eq!(
        spec.x_axis.title.as_ref().map(|title| title.text.as_str()),
        Some("date")
    );
    assert_eq!(
        spec.y_axis.title.as_ref().map(|title| title.text.as_str()),
        Some("subtests")
    );
    assert_eq!(spec.legend, LegendPos::Right);
    assert_eq!(spec.legend_title.as_deref(), Some("metric"));
    assert_eq!(spec.size_mode, SizeMode::PlotArea);
    assert_eq!(spec.series[0].stroke_width, 2.0);
    assert_eq!(spec.series[0].interpolation, LineInterpolation::Monotone);
    assert_eq!(spec.series[0].point_radius, Some(3.0));
    assert!(spec.x_axis.grid.draw_ticks);
    assert!(spec.y_axis.grid.draw_ticks);
    assert_eq!(spec.x_axis.grid.color.unwrap().a, 0.15);
    assert_eq!(spec.y_axis.grid.color.unwrap().a, 0.15);
    assert_eq!(spec.theme.background.unwrap().a, 1.0);
    assert_eq!(
        (
            spec.theme.background.unwrap().r,
            spec.theme.background.unwrap().g,
            spec.theme.background.unwrap().b
        ),
        (255, 255, 255)
    );
}

#[test]
fn temporal_line_rejects_invalid_background_color() {
    let json = DOGFOOD_SHAPE.replace("\"white\"", "\"not-a-color\"");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert_eq!(err, "background must be a valid color");
}

#[test]
fn dogfood_shape_is_accepted_by_typed_schema_and_strict_parser() {
    let _: fulgur_chart::schema::VegaLiteSpec = serde_json::from_str(DOGFOOD_SHAPE).unwrap();
    assert!(vegalite::parse(DOGFOOD_SHAPE, true).is_ok());
}

fn line_with_field(
    fixture: &str,
    path: &[&str],
    value: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut json: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let (key, parent_path) = path.split_last().expect("field path must not be empty");
    let mut parent = &mut json;
    for segment in parent_path {
        parent = parent
            .get_mut(*segment)
            .unwrap_or_else(|| panic!("missing fixture path segment: {segment}"));
    }
    let parent = parent
        .as_object_mut()
        .unwrap_or_else(|| panic!("fixture parent is not an object: {parent_path:?}"));
    match value {
        Some(value) => {
            parent.insert((*key).to_string(), value);
        }
        None => {
            parent.remove(*key);
        }
    }
    json
}

#[test]
fn temporal_line_nullable_typed_fields_treat_null_as_omission() {
    let nullable_paths: &[&[&str]] = &[
        &["$schema"],
        &["width"],
        &["height"],
        &["title"],
        &["background"],
        &["config"],
        &["mark", "point"],
        &["mark", "interpolate"],
        &["encoding", "color"],
        &["encoding", "x", "type"],
        &["encoding", "x", "title"],
        &["encoding", "y", "type"],
        &["encoding", "y", "title"],
        &["encoding", "color", "type"],
        &["encoding", "color", "title"],
        &["encoding", "color", "scale"],
        &["config", "view"],
        &["config", "axis"],
        &["config", "view", "stroke"],
        &["config", "axis", "grid"],
        &["config", "axis", "gridOpacity"],
    ];

    let mut failures = Vec::new();
    for path in nullable_paths {
        let fixture = if *path == ["encoding", "x", "type"] {
            CATEGORICAL_LINE_SHAPE
        } else {
            DOGFOOD_SHAPE
        };
        let null_json = line_with_field(fixture, path, Some(serde_json::Value::Null));
        let omitted_json = line_with_field(fixture, path, None);
        let path = path.join(".");

        if let Err(err) =
            serde_json::from_value::<fulgur_chart::schema::VegaLiteSpec>(null_json.clone())
        {
            failures.push(format!("{path}: typed schema rejected null: {err}"));
            continue;
        }

        for strict in [false, true] {
            let omitted = vegalite::parse(&omitted_json.to_string(), strict)
                .unwrap_or_else(|err| panic!("{path}: omitted value failed: {err}"));
            match vegalite::parse(&null_json.to_string(), strict) {
                Ok(actual) if actual == omitted => {}
                Ok(_) => failures.push(format!(
                    "{path}: strict={strict} null did not match omission"
                )),
                Err(err) => failures.push(format!("{path}: strict={strict} rejected null: {err}")),
            }
        }
    }

    assert!(
        failures.is_empty(),
        "nullable temporal-line mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn typed_line_schema_constrains_channel_types() {
    for json in [
        DOGFOOD_SHAPE.replace("\"temporal\"", "\"temporl\""),
        DOGFOOD_SHAPE.replace("\"quantitative\"", "\"nominal\""),
        DOGFOOD_SHAPE.replace("\"nominal\"", "\"quantitative\""),
    ] {
        assert!(
            serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&json).is_err(),
            "typed schema accepted unsupported line channel type"
        );
    }
}

#[test]
fn typed_categorical_line_schema_excludes_temporal_only_options() {
    let base = r#"{
        "mark":{"type":"line"},
        "data":{"values":[{"x":"a","y":1}]},
        "encoding":{
            "x":{"field":"x","type":"nominal"},
            "y":{"field":"y","type":"quantitative"}
        }
    }"#;
    let with_point = base.replace(r#""type":"line""#, r#""type":"line","point":false"#);
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&with_point).is_err());
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(base).is_ok());
}

#[test]
fn strict_temporal_line_rejects_interpolatee_with_full_key_path() {
    let json = DOGFOOD_SHAPE.replace("\"interpolate\"", "\"interpolatee\"");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(err.contains("mark.interpolatee"), "unexpected error: {err}");
}

#[test]
fn strict_temporal_line_rejects_grid_opacit_with_full_key_path() {
    let json = DOGFOOD_SHAPE.replace("\"gridOpacity\"", "\"gridOpacit\"");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(
        err.contains("config.axis.gridOpacit"),
        "unexpected error: {err}"
    );
}

#[test]
fn strict_temporal_line_rejects_scheeme_with_full_key_path() {
    let json = DOGFOOD_SHAPE.replace("\"scheme\"", "\"scheeme\"");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(
        err.contains("encoding.color.scale.scheeme"),
        "unexpected error: {err}"
    );
}

#[test]
fn strict_temporal_line_type_errors_do_not_echo_nested_payloads() {
    let payload_marker = "FULL_PAYLOAD_MARKER_".repeat(32);
    let invalid_interpolate = format!(r#"{{"marker":"{payload_marker}"}}"#);
    let json = DOGFOOD_SHAPE.replace("\"monotone\"", &invalid_interpolate);
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(err.contains("mark.interpolate"), "unexpected error: {err}");
    assert!(err.len() < 200, "unbounded error: {err}");
    assert!(
        !err.contains(&payload_marker),
        "error echoed nested payload: {err}"
    );
}

#[test]
fn strict_temporal_line_rejects_unsupported_explicit_values() {
    let err =
        vegalite::parse(&DOGFOOD_SHAPE.replace("\"monotone\"", "\"step\""), true).unwrap_err();
    assert!(err.contains("mark.interpolate"), "unexpected error: {err}");

    let err = vegalite::parse(
        &DOGFOOD_SHAPE.replace("\"tableau10\"", "\"category10\""),
        true,
    )
    .unwrap_err();
    assert!(
        err.contains("encoding.color.scale.scheme"),
        "unexpected error: {err}"
    );

    let err = vegalite::parse(&DOGFOOD_SHAPE.replace("0.15", "1.5"), true).unwrap_err();
    assert!(
        err.contains("config.axis.gridOpacity"),
        "unexpected error: {err}"
    );
}

#[test]
fn non_strict_temporal_line_rejects_unsupported_interpolation() {
    for replacement in ["\"step\"", "42"] {
        let err = vegalite::parse(&DOGFOOD_SHAPE.replace("\"monotone\"", replacement), false)
            .unwrap_err();
        assert!(err.contains("mark.interpolate"), "unexpected error: {err}");
    }
}

#[test]
fn non_strict_temporal_line_rejects_non_boolean_point() {
    let err = vegalite::parse(
        &DOGFOOD_SHAPE.replace(r#""point":true"#, r#""point":"true""#),
        false,
    )
    .unwrap_err();
    assert!(err.contains("mark.point"), "unexpected error: {err}");
}

#[test]
fn non_strict_temporal_line_rejects_out_of_range_grid_opacity() {
    for replacement in ["-0.5", "1.5", "\"opaque\""] {
        let err = vegalite::parse(
            &DOGFOOD_SHAPE.replace(
                r#""gridOpacity":0.15"#,
                &format!(r#""gridOpacity":{replacement}"#),
            ),
            false,
        )
        .unwrap_err();
        assert!(
            err.contains("config.axis.gridOpacity"),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn temporal_line_treats_null_color_scale_as_absent() {
    let json = DOGFOOD_SHAPE.replace(r#""scale":{"scheme":"tableau10"}"#, r#""scale":null"#);
    for strict in [false, true] {
        vegalite::parse(&json, strict).unwrap();
    }
}

#[test]
fn non_strict_temporal_line_rejects_unsupported_color_scheme() {
    for (json, expected) in [
        (
            DOGFOOD_SHAPE.replace("\"tableau10\"", "\"category10\""),
            "encoding.color.scale.scheme",
        ),
        (
            DOGFOOD_SHAPE.replace("\"tableau10\"", "42"),
            "encoding.color.scale.scheme",
        ),
        (
            DOGFOOD_SHAPE.replace(r#""scale":{"scheme":"tableau10"}"#, r#""scale":{}"#),
            "encoding.color.scale.scheme",
        ),
        (
            DOGFOOD_SHAPE.replace(r#""scale":{"scheme":"tableau10"}"#, r#""scale":42"#),
            "encoding.color.scale",
        ),
    ] {
        let err = vegalite::parse(&json, false).unwrap_err();
        assert!(err.contains(expected), "unexpected error: {err}");
    }
}

#[test]
fn strict_line_rejects_unsupported_channel_type_values() {
    let err =
        vegalite::parse(&DOGFOOD_SHAPE.replace("\"temporal\"", "\"temporl\""), true).unwrap_err();
    assert!(err.contains("encoding.x.type"), "unexpected error: {err}");

    let err = vegalite::parse(
        &DOGFOOD_SHAPE.replace("\"quantitative\"", "\"quantitativ\""),
        true,
    )
    .unwrap_err();
    assert!(err.contains("encoding.y.type"), "unexpected error: {err}");

    let err =
        vegalite::parse(&DOGFOOD_SHAPE.replace("\"nominal\"", "\"nominl\""), true).unwrap_err();
    assert!(
        err.contains("encoding.color.type"),
        "unexpected error: {err}"
    );

    let err = vegalite::parse(
        &DOGFOOD_SHAPE.replace(r#""type":"temporal""#, r#""type":42"#),
        true,
    )
    .unwrap_err();
    assert!(err.contains("encoding.x.type"), "unexpected error: {err}");
}

#[test]
fn strict_temporal_line_requires_color_field() {
    let json = DOGFOOD_SHAPE.replace(r#""field":"metric","#, "");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert_eq!(err, "encoding.color.field is required");
    assert!(vegalite::parse(&json, false).is_ok());
}

#[test]
fn strict_categorical_line_rejects_temporal_only_options() {
    let base = r#"{
        "mark":{"type":"line"},
        "data":{"values":[{"x":"a","y":1},{"x":"b","y":2}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}
    }"#;
    for (needle, replacement, expected) in [
        (
            r#""type":"line""#,
            r#""type":"line","point":false"#,
            "mark.point",
        ),
        (
            r#""type":"line""#,
            r#""type":"line","interpolate":"monotone""#,
            "mark.interpolate",
        ),
        (
            r#""mark":{"type":"line"}"#,
            r#""mark":{"type":"line"},"background":"white""#,
            "background",
        ),
        (
            r#""mark":{"type":"line"}"#,
            r#""mark":{"type":"line"},"config":{"axis":{"gridOpacity":0.5}}"#,
            "config",
        ),
        (
            r#""field":"x","type":"nominal""#,
            r#""field":"x","type":"nominal","title":"category""#,
            "encoding.x.title",
        ),
        (
            r#""field":"y","type":"quantitative""#,
            r#""field":"y","type":"quantitative","title":"value""#,
            "encoding.y.title",
        ),
        (
            r#""y":{"field":"y","type":"quantitative"}"#,
            r#""y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal","title":"group"}"#,
            "encoding.color.title",
        ),
        (
            r#""y":{"field":"y","type":"quantitative"}"#,
            r#""y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal","scale":{"scheme":"tableau10"}}"#,
            "encoding.color.scale",
        ),
    ] {
        let err = vegalite::parse(&base.replace(needle, replacement), true).unwrap_err();
        assert!(err.contains(expected), "{expected}: {err}");
    }
}

#[test]
fn strict_temporal_line_rejects_non_null_view_stroke() {
    let json = DOGFOOD_SHAPE.replace(r#""stroke":null"#, r##""stroke":"#ddd""##);
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(
        err.contains("config.view.stroke"),
        "unexpected error: {err}"
    );

    assert!(
        serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&json).is_err(),
        "typed schema must reject non-null config.view.stroke"
    );
}

#[test]
fn strict_temporal_line_reports_nested_type_and_required_key_errors() {
    let cases = [
        (
            DOGFOOD_SHAPE.replace(r#","interpolate":"monotone""#, ""),
            None,
        ),
        (
            DOGFOOD_SHAPE.replace(r#""title":"date""#, r#""title":"date","typo":true"#),
            Some("encoding.x.typo"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""field":"timestamp""#, r#""field":[] "#),
            Some("encoding.x.field"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""field":"timestamp""#, r#""field":false"#),
            Some("encoding.x.field"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""title":"date""#, r#""title":null"#),
            None,
        ),
        (
            DOGFOOD_SHAPE.replace(r#""title":"date""#, r#""title":42"#),
            Some("encoding.x.title"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""title":"metric""#, r#""title":"metric","typo":true"#),
            Some("encoding.color.typo"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""scheme":"tableau10""#, r#""scheme":42"#),
            Some("encoding.color.scale.scheme"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""scale":{"scheme":"tableau10"}"#, r#""scale":{}"#),
            Some("encoding.color.scale.scheme is required"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""stroke":null"#, r#""stroke":null,"typo":true"#),
            Some("config.view.typo"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""gridOpacity":0.15"#, r#""gridOpacity":"opaque""#),
            Some("config.axis.gridOpacity"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""point":true"#, r#""point":{"enabled":true}"#),
            Some("mark.point"),
        ),
        (
            DOGFOOD_SHAPE.replace(r#""grid":true"#, r#""grid":false"#),
            None,
        ),
    ];

    for (json, expected) in cases {
        match expected {
            Some(expected) => {
                let err = vegalite::parse(&json, true).unwrap_err();
                assert!(err.contains(expected), "{expected}: {err}");
            }
            None => {
                vegalite::parse(&json, true).unwrap();
            }
        }
    }
}

#[test]
fn bar_categorical_single_series() {
    let spec = vegalite::parse(BAR_SPEC, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].values, vec![3.0, 5.0, 2.0]);
}

#[test]
fn color_split_creates_one_series_per_group() {
    let json = r#"{
        "mark": "bar",
        "data": {"values": [
            {"cat":"A","val":3,"g":"x"},
            {"cat":"B","val":5,"g":"y"},
            {"cat":"A","val":1,"g":"y"},
            {"cat":"C","val":2,"g":"x"}
        ]},
        "encoding": {
            "x": {"field":"cat","type":"nominal"},
            "y": {"field":"val","type":"quantitative"},
            "color": {"field":"g"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series.len(), 2);
    // 系列名は g の first-seen 順: x, y
    assert_eq!(spec.series[0].name, "x");
    assert_eq!(spec.series[1].name, "y");
    // x グループ: A=3, B=0(欠落), C=2
    assert_eq!(spec.series[0].values, vec![3.0, 0.0, 2.0]);
    // y グループ: A=1, B=5, C=0(欠落)
    assert_eq!(spec.series[1].values, vec![1.0, 5.0, 0.0]);
}

#[test]
fn line_mark_maps_to_line() {
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line));
}

#[test]
fn point_mark_maps_to_scatter_with_points() {
    let json = r#"{
        "mark": "point",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(spec.series.len(), 1);
    let pts = &spec.series[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!((pts[0].x, pts[0].y), (1.0, 2.0));
    assert_eq!((pts[1].x, pts[1].y), (3.0, 4.0));
}

#[test]
fn arc_mark_maps_to_pie_with_theta_sums() {
    let json = r#"{
        "mark": "arc",
        "data": {"values": [
            {"cat":"A","val":3},
            {"cat":"B","val":5},
            {"cat":"A","val":2}
        ]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Pie { .. }));
    assert_eq!(spec.categories, vec!["A", "B"]);
    assert_eq!(spec.series.len(), 1);
    // A = 3+2 = 5, B = 5
    assert_eq!(spec.series[0].values, vec![5.0, 5.0]);
}

#[test]
fn mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "bar"},
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
}

#[test]
fn strict_accepts_known_keys() {
    assert!(vegalite::parse(BAR_SPEC, true).is_ok());
}

#[test]
fn strict_rejects_unknown_top_level_key() {
    let json = r#"{
        "mark": "bar",
        "wat": 1,
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn parse_is_deterministic() {
    let a = vegalite::parse(BAR_SPEC, false).unwrap();
    let b = vegalite::parse(BAR_SPEC, false).unwrap();
    assert_eq!(a, b);
}

#[test]
fn unknown_mark_errors() {
    let json = r#"{
        "mark": "wedge",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn url_data_errors() {
    let json = r#"{
        "mark": "bar",
        "data": {"url": "data.csv"},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn render_smoke_produces_svg() {
    let spec = vegalite::parse(BAR_SPEC, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
}

#[test]
fn category_field_non_scalar_errors() {
    // x の値が object → カテゴリ値にできず Err(空カテゴリへの統合を防ぐ)。
    let json = r#"{"mark":"bar","data":{"values":[{"cat":{"nested":1},"val":3}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn typo_missing_or_nonnumeric_field_errors() {
    // y.field の typo → 全 0 の誤チャートを防ぐため Err。
    let typo = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":3}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"vall"}}}"#;
    assert!(vegalite::parse(typo, false).is_err());
    // y が文字列(非数値) → Err。
    let nonnum = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":"x"}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(nonnum, false).is_err());
    // 必須 x.field 未指定 → Err。
    let missing = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":3}]},
        "encoding":{"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(missing, false).is_err());
}

#[test]
fn line_with_sparse_color_errors() {
    // 色分け line で (cat,color) が疎 → 0 埋めの誤った折れ線を防ぐため Err。
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3,"g":"x"},{"cat":"B","val":5,"g":"y"}]},
        "encoding": {"x":{"field":"cat"},"y":{"field":"val"},"color":{"field":"g"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn line_with_dense_color_ok() {
    // 全 (cat,color) が揃っていれば 2 系列で OK。
    let json = r#"{
        "mark": "line",
        "data": {"values": [
            {"cat":"A","val":3,"g":"x"},{"cat":"B","val":5,"g":"x"},
            {"cat":"A","val":2,"g":"y"},{"cat":"B","val":4,"g":"y"}
        ]},
        "encoding": {"x":{"field":"cat"},"y":{"field":"val"},"color":{"field":"g"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.series.len(), 2);
}

#[test]
fn honors_width_height_title() {
    // VL の width/height/title を ChartSpec に反映する(strict 許可と整合)。
    let json = r#"{
        "mark": "bar",
        "width": 400, "height": 300, "title": "売上",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.width, 400.0);
    assert_eq!(spec.height, 300.0);
    assert_eq!(spec.title.as_deref(), Some("売上"));
}

#[test]
fn strict_rejects_aggregate() {
    // aggregate は未実装。strict では未対応キーとして拒否する(誤った集計を黙認しない)。
    let json = r#"{
        "mark": "bar",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val","aggregate":"mean"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn circle_mark_maps_to_scatter_with_points() {
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(spec.series.len(), 1);
    let pts = &spec.series[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!((pts[0].x, pts[0].y), (1.0, 2.0));
    assert_eq!((pts[1].x, pts[1].y), (3.0, 4.0));
}

#[test]
fn circle_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "circle"},
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
}

#[test]
fn circle_mark_renders_svg() {
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    // scatter renderer emits one <circle> per point.
    assert_eq!(svg.matches("<circle ").count(), 2);
}

#[test]
fn strict_circle_rejects_shape_encoding() {
    // 構造的 shape 非対応の invariant を strict パーサで pin する。
    // 現状 check_unknown_keys の encoding allow-list は shape を含まない
    // ため強制されるが、allow-list ドリフトで壊れないようテストで固定する。
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}, "shape": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_bar_rejects_theta_encoding() {
    let json = r#"{
        "mark": "bar",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    // strict では VlBarEncoding が受理しない theta を拒否する。
    assert!(vegalite::parse(json, true).is_err());
    // 非 strict では現状通り黙って許容(挙動維持)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_line_rejects_theta_encoding() {
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_point_rejects_theta_encoding() {
    let json = r#"{
        "mark": "point",
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_arc_accepts_x_encoding() {
    // arc の allow-list は [theta, color, x, y] を含むので strict でも OK。
    let json = r#"{
        "mark": "arc",
        "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5}]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}, "x": {"field":"cat"}}
    }"#;
    assert!(vegalite::parse(json, true).is_ok());
}

#[test]
fn strict_arc_rejects_unknown_encoding_channel() {
    // arc の allow-list に含まれない channel(size)は strict で拒否される。
    // arc 側からも invariant を pin して、将来の allow-list ドリフトを検出する。
    let json = r#"{
        "mark": "arc",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}, "size": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    // 非 strict では現状通り黙って許容(挙動維持)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_object_form_mark_dispatches_allow_list() {
    // object 形の mark(`{"type": "bar"}`)からも mark 名を読めることを pin する。
    // read_mark_name の object 分岐が strict 経路で使われる保証。
    let json = r#"{
        "mark": {"type": "bar"},
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_unknown_mark_falls_through_to_parse_error() {
    // 未対応 mark は check_unknown_keys で早期 Err にせず、後段の
    // parse_mark へフォールスルーする。エラー文言も encoding.* ではなく
    // mark 名についてのものであることを確認して invariant を pin する。
    let json = r#"{
        "mark": "unknownX",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    let err = vegalite::parse(json, true).unwrap_err();
    assert!(
        err.contains("mark") && !err.contains("encoding."),
        "expected mark-name error, got: {err}"
    );
}

#[test]
fn rect_ir_variant_exists() {
    use fulgur_chart::ir::{ChartKind, Color};
    let kind = ChartKind::VegaRect {
        x_labels: vec!["A".to_string(), "B".to_string()],
        y_labels: vec!["X".to_string()],
        cells: vec![vec![
            Some(Color {
                r: 10,
                g: 20,
                b: 30,
                a: 1.0,
            }),
            None,
        ]],
    };
    assert!(matches!(kind, ChartKind::VegaRect { .. }));
}

#[test]
fn rect_mark_quantitative_maps_to_vegarect() {
    // 2x2 grid, quantitative color.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"day":"Mon","hour":"AM","v":1},
            {"day":"Tue","hour":"AM","v":3},
            {"day":"Mon","hour":"PM","v":5},
            {"day":"Tue","hour":"PM","v":7}
        ]},
        "encoding": {
            "x": {"field":"day","type":"nominal"},
            "y": {"field":"hour","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let (x_labels, y_labels, cells) = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } => (x_labels.clone(), y_labels.clone(), cells.clone()),
        _ => panic!("expected VegaRect, got {:?}", spec.kind),
    };
    // first-seen order
    assert_eq!(x_labels, vec!["Mon", "Tue"]);
    assert_eq!(y_labels, vec!["AM", "PM"]);
    // 2 rows x 2 cols
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].len(), 2);
    // min (v=1) at (Mon, AM) → color_lo (#ffffff white)
    let c00 = cells[0][0].expect("cell should not be None");
    assert_eq!(
        (c00.r, c00.g, c00.b),
        (255, 255, 255),
        "min cell should be white"
    );
    // max (v=7) at (Tue, PM) → color_hi (VL theme palette[0] = Tableau steel-blue #4c78a8 = (76, 120, 168))
    let c11 = cells[1][1].expect("cell should not be None");
    assert_eq!(
        (c11.r, c11.g, c11.b),
        (76, 120, 168),
        "max cell should be Tableau steel-blue"
    );
}

#[test]
fn rect_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "rect"},
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        fulgur_chart::ir::ChartKind::VegaRect { .. }
    ));
}

#[test]
fn rect_mark_nominal_color_uses_palette_roundrobin() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","c":"cat0"},
            {"x":"B","y":"X","c":"cat1"},
            {"x":"A","y":"Y","c":"cat0"}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"c","type":"nominal"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // Vega-Lite Tableau10 first color: #4c78a8 (76, 120, 168)
    // Vega-Lite Tableau10 second color: #f58518 (245, 133, 24)
    let cat0_color = cells[0][0].expect("cell (A,X) present"); // cat0 → palette[0]
    let cat1_color = cells[0][1].expect("cell (B,X) present"); // cat1 → palette[1]
    let cat0_color_again = cells[1][0].expect("cell (A,Y) present"); // cat0 → palette[0]
    assert_eq!((cat0_color.r, cat0_color.g, cat0_color.b), (76, 120, 168));
    assert_eq!((cat1_color.r, cat1_color.g, cat1_color.b), (245, 133, 24));
    assert_eq!(cat0_color, cat0_color_again, "same category → same color");
    // (B, Y) は未出現 → None
    assert!(cells[1][1].is_none(), "missing (B,Y) should be None");
}

#[test]
fn rect_mark_rejects_null_color_value() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":null}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"c"}}
    }"#;
    let err = vegalite::parse(json, false).unwrap_err();
    assert!(
        err.contains("見つかりません") || err.contains("null"),
        "got: {err}"
    );
}

#[test]
fn rect_mark_aggregate_mean() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) mean = (2 + 4) / 2 = 3.0 → min
    // (B, X) mean = 10 → max
    // range = 10 - 3 = 7, (A,X) t = 0.0 → white
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255), "mean=3 → min → white");
    // (B, X) is at column index 1, row 0
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!(
        (bx.r, bx.g, bx.b),
        (76, 120, 168),
        "mean=10 → max → Tableau blue"
    );
}

#[test]
fn rect_mark_aggregate_sum() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"sum"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) sum = 6, (B, X) sum = 10, range = 4
    // (A,X) t=0 → white, (B,X) t=1 → blue
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255));
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168));
}

#[test]
fn rect_mark_aggregate_mean_vs_sum_are_distinguishable() {
    // 3 x-buckets so the aggregated (A,X) can land at intermediate positions.
    // Data:
    //   (A,X): [10, 10] — mean=10, sum=20
    //   (B,X): [15]      — mean=15, sum=15
    //   (C,X): [5]       — mean=5,  sum=5
    // Mean: range [5, 15], (A,X)=10 → t=0.5 → intermediate color
    // Sum:  range [5, 20], (A,X)=20 → t=1.0 → RECT_COLOR_HI (blue)
    let json_mean = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":10},
            {"x":"A","y":"X","v":10},
            {"x":"B","y":"X","v":15},
            {"x":"C","y":"X","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let json_sum = json_mean.replace(r#""aggregate":"mean""#, r#""aggregate":"sum""#);

    let mean_cells = match &vegalite::parse(json_mean, false).unwrap().kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    let sum_cells = match &vegalite::parse(&json_sum, false).unwrap().kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };

    // (A,X) is at row 0, col 0.
    let mean_ax = mean_cells[0][0].expect("mean (A,X) present");
    let sum_ax = sum_cells[0][0].expect("sum (A,X) present");
    // Mean: (A,X) is intermediate (not blue).
    assert_ne!(
        (mean_ax.r, mean_ax.g, mean_ax.b),
        (76, 120, 168),
        "mean (A,X) should not be at max endpoint"
    );
    // Sum: (A,X) becomes the max (t=1.0) → Tableau blue.
    assert_eq!(
        (sum_ax.r, sum_ax.g, sum_ax.b),
        (76, 120, 168),
        "sum (A,X) should be at max endpoint"
    );
    // And they must differ.
    assert_ne!(
        mean_ax, sum_ax,
        "mean and sum should produce different colors here"
    );
}

#[test]
fn rect_mark_aggregate_none_preserves_last_finite_numeric() {
    // Explicit quantitative + no aggregate + a bool follows a number at the same cell.
    // With bucket-based None, the finite numeric value survives (not clobbered by the non-numeric).
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":7},
            {"x":"A","y":"X","v":true},
            {"x":"B","y":"X","v":1}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A,X) uses v=7 (last finite numeric); (B,X) uses v=1.
    // range [1, 7], (A,X)=7 → max → blue; (B,X)=1 → min → white.
    let ax = cells[0][0].expect("(A,X) should be Some");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
    let bx = cells[0][1].expect("(B,X) should be Some");
    assert_eq!((bx.r, bx.g, bx.b), (255, 255, 255));
}

#[test]
fn strict_rect_rejects_size_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "size": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "size should be rejected in strict"
    );
    assert!(
        vegalite::parse(json, false).is_ok(),
        "size should be tolerated in non-strict"
    );
}

#[test]
fn strict_rect_rejects_tooltip_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "tooltip": {"field":"v"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_x2_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "x2": {"field":"x2"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_rect_rejects_y2_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "y2": {"field":"y2"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_rect_rejects_quantitative_x() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":1,"y":2,"v":3}]},
        "encoding": {
            "x": {"field":"x","type":"quantitative"},
            "y": {"field":"y"},
            "color": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "quantitative x should be rejected in strict"
    );
    // 非 strict では文字列化して受理される(既存の緩さと同型)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_quantitative_y() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":2,"v":3}]},
        "encoding": {
            "x": {"field":"x"},
            "y": {"field":"y","type":"quantitative"},
            "color": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "quantitative y should be rejected in strict"
    );
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_temporal_axis_type() {
    // rect の x/y は nominal / ordinal のみ受理。temporal は sequence として
    // 意味的に不定なので strict では明示 Err にする。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x","type":"temporal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "temporal x axis type should be rejected in strict"
    );
    // 非 strict は既存の緩さを維持。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_typo_axis_type() {
    // "nomial" は "nominal" の typo。strict では silently カテゴリ扱いにせず
    // 明示 Err にする。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nomial"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "typo axis type should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_temporal_color_type() {
    // rect の color type は quantitative / nominal / ordinal のみ受理。
    // temporal は infer に落ちる前に strict Err にする。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"temporal"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "temporal color type should be rejected in strict"
    );
    // 非 strict は infer で扱う(既存挙動)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_unsupported_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"v","aggregate":"count"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "aggregate=count should be rejected"
    );
    // 非 strict では既存挙動(未対応値は無視)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_nominal_color_with_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":"cat0"}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"c","type":"nominal","aggregate":"sum"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "nominal + aggregate should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_non_string_aggregate() {
    // aggregate が数値など非文字列のとき、strict は明示 Err にする。
    // (as_str で None になり silently 無視されるバグを pin する。)
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"v","aggregate":1}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "non-string aggregate should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_inferred_nominal_with_aggregate() {
    // encoding.color.type 省略 + データが文字列 → 推論で nominal → aggregate は無効。
    // 現状 check_unknown_keys は explicit nominal しか reject しないので、
    // 推論経由のケースを pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":"cat0"}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"c","aggregate":"sum"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "inferred nominal + aggregate should be rejected in strict"
    );
    // 非 strict では既存の緩さを維持(aggregate 指定は silently 無視される)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_non_string_axis_type() {
    // Round 2 の non-string aggregate と同様、axis type の非文字列 (数値/bool/null 等)
    // も strict では明示 Err にする(as_str で silently None に落ちるバグを pin)。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x","type":1},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "non-string x axis type should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_non_string_color_type() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":1}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "non-string color type should be rejected in strict"
    );
}

#[test]
fn rect_mark_renders_svg_with_expected_rect_count() {
    // 2x2 grid, all cells present → 4 rects.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3},
            {"x":"B","y":"Y","v":4}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    let rect_count = svg.matches("<rect").count();
    // 4 cells + 1 vegalite_theme white background rect.
    assert_eq!(
        rect_count, 5,
        "expected 4 cells + 1 background, got {rect_count}"
    );
    // Axis labels appear.
    assert!(svg.contains(">A<"));
    assert!(svg.contains(">B<"));
    assert!(svg.contains(">X<"));
    assert!(svg.contains(">Y<"));
}

#[test]
fn rect_mark_skips_missing_cells() {
    // (B, Y) missing → 3 cells, None cell does not emit <rect>.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    let rect_count = svg.matches("<rect").count();
    // 3 cells (one None skipped) + 1 background.
    assert_eq!(
        rect_count, 4,
        "expected 3 cells + 1 background, got {rect_count}"
    );
}

#[test]
fn rect_mark_rejects_quantitative_with_non_numeric_color() {
    // encoding.color.type: "quantitative" は数値を要求。文字列/bool の場合、
    // 黙って空チャートを返さず明示 Err にする(bar/line の validate_numeric と同型)。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":"foo"},
            {"x":"B","y":"X","v":"bar"}
        ]},
        "encoding": {
            "x": {"field":"x"},
            "y": {"field":"y"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let err = vegalite::parse(json, false).unwrap_err();
    assert!(
        err.contains("数値") || err.contains("v"),
        "expected numeric-type error, got: {err}"
    );
}

#[test]
fn rect_mark_degenerate_all_equal_values_use_hi() {
    // range が 0 (全値同一) のとき、全セルは RECT_COLOR_HI = Tableau steel-blue。
    // 白セルが白背景に埋没しないよう HI を採用する invariant を pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":5},
            {"x":"B","y":"X","v":5},
            {"x":"A","y":"Y","v":5},
            {"x":"B","y":"Y","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    for row in &cells {
        for cell in row {
            let c = cell.expect("all cells should be Some (degenerate uses HI)");
            assert_eq!(
                (c.r, c.g, c.b),
                (76, 120, 168),
                "degenerate min==max should resolve every cell to RECT_COLOR_HI"
            );
        }
    }
}

#[test]
fn rect_hi_color_matches_vegalite_palette_head() {
    // RECT_COLOR_HI は Vega-Lite テーマの palette[0] (Tableau10 steel-blue) と揃える
    // 前提。パレット定数の drift を pin する。
    let palette_head = fulgur_chart::palette::vegalite_theme().palette[0];
    assert_eq!(
        (palette_head.r, palette_head.g, palette_head.b),
        (76, 120, 168),
        "vegalite palette[0] must remain (76, 120, 168); RECT_COLOR_HI depends on this"
    );
}

#[test]
fn rect_mark_aggregate_mean_running_avoids_overflow() {
    // [1e308, 1e308] の mean は数学的には 1e308。単純 sum → 除算だと intermediate が
    // inf に overflow して cell が None に落ちる。running mean は finite を返す。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1.0e308},
            {"x":"A","y":"X","v":1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // 単一有限セル → degenerate → RECT_COLOR_HI (76, 120, 168)。
    // 重要なのは None (overflow 起因) にならないこと。
    let ax = cells[0][0].expect("(A,X) should not be None due to sum overflow");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
}

#[test]
fn rect_mark_range_overflow_treated_as_degenerate() {
    // max - min が inf に overflow するケース → 全セル HI (白 埋没を防ぐ)。
    // 現状 lerp が NaN → 0 → LO で silently 全白になるバグを pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":-1.0e308},
            {"x":"B","y":"X","v":1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    let ax = cells[0][0].expect("(A,X) should be Some (degenerate uses HI)");
    let bx = cells[0][1].expect("(B,X) should be Some (degenerate uses HI)");
    // Degenerate treatment → both cells = HI, not both = LO (白背景に埋没する古い挙動)。
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168));
}

#[test]
fn rect_mark_inspect_model_reports_labels() {
    // build_model_core は spec.series/categories が空だと datasets=0/x_ticks=0 になるが、
    // VegaRect は x/y_labels に情報が入っているので特殊対応で正しい counts を返す。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let model = fulgur_chart::model::build_model_core(&spec);
    assert_eq!(model.meta.r#type, "vegaRect");
    assert_eq!(
        model.counts.x_ticks, 2,
        "x_ticks should reflect x_labels len"
    );
    assert_eq!(
        model.counts.y_ticks, 2,
        "y_ticks should reflect y_labels len"
    );
    assert_eq!(model.counts.datasets, 1);
    assert_eq!(model.counts.legend_items, 0);
}

#[test]
fn rect_mark_rejects_oversized_x_labels_at_parse_time() {
    // x_labels の数が max_categories を超える spec は、guard::validate_spec を通す前に
    // parse 時点で reject される(build_rect が dense に確保するため pre-allocation 検査)。
    let n = fulgur_chart::guard::InputLimits::default().max_categories + 1;
    let mut values = String::with_capacity(64 * n);
    values.push('[');
    for i in 0..n {
        if i > 0 {
            values.push(',');
        }
        values.push_str(&format!(r#"{{"x":"x{i}","y":"y","v":1}}"#));
    }
    values.push(']');
    let json = format!(
        r#"{{
            "mark": "rect",
            "data": {{"values": {values}}},
            "encoding": {{
                "x": {{"field":"x","type":"nominal"}},
                "y": {{"field":"y","type":"nominal"}},
                "color": {{"field":"v","type":"quantitative"}}
            }}
        }}"#
    );
    let err = vegalite::parse(&json, false).unwrap_err();
    assert!(
        err.contains("x_labels") && err.contains("pre-allocation"),
        "expected pre-allocation guard error, got: {err}"
    );
}

#[test]
fn rect_mark_aggregate_mean_opposite_signs_uses_fallback() {
    // [1e308, -1e308] の真の mean は 0。running mean は (v - mean) が -inf に
    // なり非有限になるが、naive sum / len にフォールバックすると 0 (finite) が
    // 得られる。二段構えで silently None にしないことを pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1.0e308},
            {"x":"A","y":"X","v":-1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // Single finite cell → degenerate → HI (RECT_COLOR_HI = Tableau blue).
    let ax = cells[0][0].expect("(A,X) mean should be 0 (finite via fallback), not None");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
}

#[test]
fn rect_mark_aggregate_mean_extreme_cancellation_uses_divide_then_sum() {
    // [1e308, 1e308, -1e308, -1e308] の真の mean は 0 だが、
    // running mean は -inf に、naive sum は inf に落ちる。三段目の
    // divide-then-sum (v / n を先にする) で個々の項を有限範囲に収め、
    // sum を取って正しい mean = 0 を得る。silently None にしない invariant を pin。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1.0e308},
            {"x":"A","y":"X","v":1.0e308},
            {"x":"A","y":"X","v":-1.0e308},
            {"x":"A","y":"X","v":-1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // Single finite cell (mean=0) → degenerate → HI (Tableau steel-blue).
    // 重要なのは None に落ちないこと。
    let ax = cells[0][0].expect("mean should be finite (0), not None from double-overflow");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
}

#[test]
fn rect_mark_rejects_excessive_records_at_parse_time() {
    // 小さい grid でも records.len() が上限を超えたら pre-aggregation で reject。
    // ここでは max_categorical_primitives + 1 件の (A,X,v) を作り、同一セルへの
    // 重複観測を pin する。1 セルにしか反映されないのに bucket が数百万件の f64 を
    // 抱えるメモリ圧を parse 時点で潰す。
    let n = fulgur_chart::guard::InputLimits::default().max_categorical_primitives + 1;
    let mut values = String::with_capacity(24 * n);
    values.push('[');
    for i in 0..n {
        if i > 0 {
            values.push(',');
        }
        values.push_str(r#"{"x":"A","y":"X","v":1}"#);
    }
    values.push(']');
    let json = format!(
        r#"{{
            "mark": "rect",
            "data": {{"values": {values}}},
            "encoding": {{
                "x": {{"field":"x","type":"nominal"}},
                "y": {{"field":"y","type":"nominal"}},
                "color": {{"field":"v","type":"quantitative"}}
            }}
        }}"#
    );
    let err = vegalite::parse(&json, false).unwrap_err();
    assert!(
        err.contains("records") && err.contains("pre-aggregation"),
        "expected pre-aggregation guard error, got: {err}"
    );
}

#[test]
fn rect_mark_parse_with_limits_respects_relaxed_max_categorical_primitives() {
    // default caps は max_categories = 100k / max_categorical_primitives = 1M。
    // default だと max_categories + 1 = 100_001 x_labels は pre-allocation guard で
    // reject されるが、relaxed limits (max_categories = n+10, max_categorical_primitives
    // = n+10) を渡すと accept される。API-additive の parse_with_limits が caller
    // limits を尊重することを pin する。
    let default_limits = fulgur_chart::guard::InputLimits::default();
    let n = default_limits.max_categories + 1;
    let mut values = String::with_capacity(64 * n);
    values.push('[');
    for i in 0..n {
        if i > 0 {
            values.push(',');
        }
        values.push_str(&format!(r#"{{"x":"x{i}","y":"y","v":1}}"#));
    }
    values.push(']');
    let json = format!(
        r#"{{
            "mark": "rect",
            "data": {{"values": {values}}},
            "encoding": {{
                "x": {{"field":"x","type":"nominal"}},
                "y": {{"field":"y","type":"nominal"}},
                "color": {{"field":"v","type":"quantitative"}}
            }}
        }}"#
    );

    // Default parse は既存 pre-allocation guard で拒否する。
    assert!(
        vegalite::parse(&json, false).is_err(),
        "default limits should reject n+1 x_labels"
    );

    // parse_with_limits に relaxed caps を渡すと受理される。
    let relaxed = fulgur_chart::guard::InputLimits {
        max_categories: n + 10,
        max_categorical_primitives: n + 10,
        ..default_limits
    };
    let spec = vegalite::parse_with_limits(&json, false, &relaxed).unwrap();
    match spec.kind {
        ChartKind::VegaRect { x_labels, .. } => {
            assert_eq!(
                x_labels.len(),
                n,
                "relaxed parse should retain all x_labels"
            );
        }
        _ => panic!("expected VegaRect"),
    }
}
