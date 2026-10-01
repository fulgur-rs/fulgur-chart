use fulgur_chart::{
    frontend::{self, ParseError, chartjs, vegalite},
    ir::ChartKind,
};
use serde_json::json;

#[test]
fn chartjs_value_preserves_data_and_dimensions() {
    let spec = chartjs::parse_value(
        json!({
            "type":"bar", "width":640, "height":320,
            "data":{"labels":["A","B"],"datasets":[{"label":"Sales","data":[2,5]}]}
        }),
        true,
    )
    .unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            horizontal: false,
            ..
        }
    ));
    assert_eq!(spec.categories, ["A", "B"]);
    assert_eq!(spec.series[0].values, [2.0, 5.0]);
    assert_eq!((spec.width, spec.height), (640.0, 320.0));
}

#[test]
fn chartjs_value_dispatches_special_chart_types_with_identical_output() {
    for name in [
        "matrix",
        "treemap",
        "sankey",
        "gauge",
        "radial-gauge",
        "wordcloud",
        "progress",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/specs")
            .join(format!("{name}.json"));
        let json = std::fs::read_to_string(path).unwrap();
        for strict in [false, true] {
            match chartjs::parse(&json, strict) {
                Ok(text_spec) => {
                    let value_spec =
                        chartjs::parse_value(serde_json::from_str(&json).unwrap(), strict).unwrap();
                    assert_eq!(
                        fulgur_chart::render::render_chart(&value_spec),
                        fulgur_chart::render::render_chart(&text_spec),
                        "{name}"
                    );
                }
                Err(error) => assert_eq!(
                    chartjs::parse_value(serde_json::from_str(&json).unwrap(), strict).unwrap_err(),
                    error
                ),
            }
        }
    }
}

#[test]
fn value_inputs_reject_unknown_keys_in_strict_mode() {
    let value = json!({"type":"bar","typo":true,"data":{"datasets":[{"data":[1]}]}});
    assert!(chartjs::parse_value(value.clone(), false).is_ok());
    assert!(
        chartjs::parse_value(value, true)
            .unwrap_err()
            .contains("typo")
    );
    assert!(chartjs::parse_value(json!([]), false).is_err());
    assert!(chartjs::parse("[]", false).is_err());
    assert!(chartjs::parse_value(json!({"type":"bar","data":false}), false).is_err());
}

#[test]
fn vegalite_value_preserves_category_aggregation() {
    let spec = vegalite::parse_value(
        json!({
            "mark":"bar", "data":{"values":[{"x":"A","y":2},{"x":"A","y":3},{"x":"B","y":4}]},
            "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}
        }),
        false,
    )
    .unwrap();
    assert_eq!(spec.categories, ["A", "B"]);
    assert_eq!(spec.series[0].values, [5.0, 4.0]);
}

#[test]
fn strict_classification_keeps_parse_errors_ahead_of_unknown_keys() {
    for json in ["{", r#"{"type":"bar","typo":true,"data":false}"#] {
        assert!(matches!(
            frontend::parse_with_error_kind(json, "chartjs", true),
            Err(ParseError::Parse(_))
        ));
    }
    let json = r#"{"type":"bar","typo":true,"data":{"datasets":[{"data":[1]}]}}"#;
    assert!(matches!(
        frontend::parse_with_error_kind(json, "chartjs", true),
        Err(ParseError::Strict(_))
    ));
    assert!(frontend::parse_with_error_kind(json, "chartjs", false).is_ok());
}

#[test]
fn strict_classification_handles_valid_and_invalid_vegalite() {
    let valid = r#"{"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}"#;
    assert!(frontend::parse_with_error_kind(valid, "vegalite", true).is_ok());
    let mut unknown: serde_json::Value = serde_json::from_str(valid).unwrap();
    unknown["typo"] = json!(true);
    assert!(matches!(
        frontend::parse_with_error_kind(&unknown.to_string(), "vegalite", true),
        Err(ParseError::Strict(_))
    ));
    unknown["data"] = json!(false);
    assert!(matches!(
        frontend::parse_with_error_kind(&unknown.to_string(), "vegalite", true),
        Err(ParseError::Parse(_))
    ));
}

#[test]
fn chart_type_detection_keeps_last_key_for_special_charts() {
    let json = r#"{"type":"bar","data":{"datasets":[{"data":[{"x":"A","y":"B","v":2}]}]},"type":"matrix"}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Matrix { .. }));
}

#[test]
fn vegalite_value_checks_composition_limits_and_strict_keys() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/specs/vegalite-layer.json");
    let json = std::fs::read_to_string(path).unwrap();
    let spec = vegalite::parse_value(serde_json::from_str(&json).unwrap(), false).unwrap();
    assert!(matches!(spec.kind, ChartKind::VegaComposition(_)));
    let limits = fulgur_chart::guard::InputLimits {
        max_vega_composition_views: 1,
        ..Default::default()
    };
    assert!(
        vegalite::parse_value_with_limits(serde_json::from_str(&json).unwrap(), false, &limits,)
            .unwrap_err()
            .contains("max_vega_composition_views")
    );
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["layer"][0]["typo"] = json!(true);
    assert!(
        vegalite::parse_value(value, true)
            .unwrap_err()
            .contains("typo")
    );
}

#[test]
fn dsl_detection_ignores_data_and_keeps_key_presence_and_priority() {
    for (json, expected) in [
        (
            r#"{"type":"bar","data":{"labels":["mark"],"datasets":[{"data":[1,2]}]}}"#,
            Some("chartjs"),
        ),
        (r#"{"type":"bar","mark":null}"#, Some("vegalite")),
        (r#"{"type":null}"#, Some("chartjs")),
        ("{}", None),
        ("[]", None),
        ("{", None),
        (r#"{"mark":"bar"} trailing"#, None),
    ] {
        assert_eq!(frontend::detect_dsl(json), expected, "{json}");
    }
    assert!(matches!(
        frontend::parse_with_error_kind("{", "chartjs", false),
        Err(ParseError::Parse(_))
    ));
}

#[test]
fn type_detection_preserves_value_validation_of_ignored_fields() {
    let nested = format!("{}0{}", "[".repeat(129), "]".repeat(129));
    for ignored in ["1e400", nested.as_str()] {
        let input = format!(
            r#"{{"type":"matrix","data":{{"datasets":[{{"data":[{{"x":"A","y":"B","v":2}}]}}]}},"ignored":{ignored}}}"#
        );
        for strict in [false, true] {
            assert!(chartjs::parse(&input, strict).is_err(), "{ignored}");
        }
        assert_eq!(frontend::detect_dsl(&input), None);
    }
}
