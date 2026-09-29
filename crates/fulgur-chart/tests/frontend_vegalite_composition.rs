use fulgur_chart::schema::VegaLiteSpec;
use fulgur_chart::{
    frontend::vegalite,
    ir::{ChartKind, VegaCompositionNode},
};

#[test]
fn vegalite_composition_schema_accepts_each_operator_and_recursive_children() {
    let layer = r#"{
      "layer":[
        {"mark":"bar"},
        {"mark":{"type":"line","point":true}}
      ],
      "data":{"values":[{"day":"Mon","sales":4}]},
      "encoding":{
        "x":{"field":"day","type":"nominal"},
        "y":{"field":"sales","type":"quantitative"}
      },
      "resolve":{
        "scale":{"x":"shared","color":"independent"},
        "axis":{"x":"shared"},
        "legend":{"color":"independent"}
      }
    }"#;
    let hconcat = r#"{
      "hconcat":[
        {"mark":"point","encoding":{"x":{"field":"x"},"y":{"field":"y"}}},
        {"mark":"square","encoding":{"x":{"field":"x"},"y":{"field":"y"}}}
      ],
      "data":{"values":[{"x":1,"y":2}]},
      "spacing":12
    }"#;
    let nested_vconcat = r#"{
      "vconcat":[
        {
          "layer":[{"mark":"bar"},{"mark":"line"}],
          "data":{"values":[{"day":"Mon","sales":4}]},
          "encoding":{"x":{"field":"day"},"y":{"field":"sales"}}
        },
        {
          "hconcat":[
            {"mark":"circle","encoding":{"x":{"field":"x"},"y":{"field":"y"}}}
          ],
          "data":{"values":[{"x":1,"y":2}]}
        }
      ]
    }"#;

    assert!(
        serde_json::from_str::<VegaLiteSpec>(layer).is_ok(),
        "typed schema must accept inherited layer data and encoding"
    );
    assert!(
        serde_json::from_str::<VegaLiteSpec>(hconcat).is_ok(),
        "typed schema must accept inherited hconcat data"
    );
    assert!(
        serde_json::from_str::<VegaLiteSpec>(nested_vconcat).is_ok(),
        "typed schema must accept recursively nested composition operators"
    );
}

#[test]
fn vegalite_composition_schema_rejects_empty_arrays_and_unknown_composition_keys() {
    for json in [
        r#"{"layer":[]}"#,
        r#"{"hconcat":[]}"#,
        r#"{"vconcat":[]}"#,
        r#"{"layer":[{"mark":"bar"}],"futureOption":true}"#,
        r#"{"layer":[{"mark":"bar"}],"resolve":{"scale":{"z":"shared"}}}"#,
    ] {
        assert!(
            serde_json::from_str::<VegaLiteSpec>(json).is_err(),
            "composition schema accepted malformed spec: {json}"
        );
    }
}

#[test]
fn vegalite_composition_generated_schema_is_recursive_and_requires_children() {
    let schema =
        serde_json::to_value(schemars::schema_for!(VegaLiteSpec)).expect("schema serializes");
    let definitions = &schema["$defs"];

    for (definition, operator) in [
        ("VlLayerSpec", "layer"),
        ("VlHConcatSpec", "hconcat"),
        ("VlVConcatSpec", "vconcat"),
    ] {
        let children = &definitions[definition]["properties"][operator];
        assert_eq!(
            children["minItems"], 1,
            "{definition}.{operator} must describe at least one child"
        );
        assert!(
            children["items"]["$ref"].as_str().is_some(),
            "{definition}.{operator} must use the recursive child schema"
        );
    }
}

#[test]
fn vegalite_composition_inherits_and_overrides_data_and_encoding() {
    let json = r#"{
      "data":{"values":[{"day":"Mon","base":4,"current":5}]},
      "encoding":{"x":{"field":"day","type":"nominal"},"y":{"field":"base","type":"quantitative"}},
      "layer":[
        {"mark":"bar"},
        {"mark":"line","encoding":{"y":{"field":"current","type":"quantitative"}}},
        {"mark":"bar","data":{"values":[{"day":"Tue","base":8}]}}
      ]
    }"#;

    let parsed = vegalite::parse(json, true).expect("composition parses");
    let ChartKind::VegaComposition(root) = &parsed.kind else {
        panic!("composition root expected: {:?}", parsed.kind);
    };
    let VegaCompositionNode::Layer(layer) = root.as_ref() else {
        panic!("layer root expected");
    };
    let leaves = layer
        .children
        .iter()
        .map(|node| match node {
            VegaCompositionNode::Unit(leaf) => &leaf.spec,
            _ => panic!("unit leaf expected"),
        })
        .collect::<Vec<_>>();
    assert_eq!(leaves[0].categories, ["Mon"]);
    assert_eq!(leaves[0].series[0].values, [4.0]);
    assert_eq!(leaves[1].categories, ["Mon"]);
    assert_eq!(leaves[1].series[0].values, [5.0]);
    assert_eq!(leaves[2].categories, ["Tue"]);
    assert_eq!(leaves[2].series[0].values, [8.0]);
}

#[test]
fn vegalite_composition_accepts_boxplot_unit_marks_in_both_modes() {
    let json = r#"{
      "layer":[{"mark":"boxplot"}],
      "data":{"values":[{"value":1}]},
      "encoding":{"y":{"field":"value","type":"quantitative"}}
    }"#;

    for strict in [false, true] {
        let parsed = vegalite::parse(json, strict).expect("boxplot composition parses");
        let ChartKind::VegaComposition(root) = &parsed.kind else {
            panic!("composition root expected: {:?}", parsed.kind);
        };
        let VegaCompositionNode::Layer(layer) = root.as_ref() else {
            panic!("layer root expected");
        };
        let [VegaCompositionNode::Unit(leaf)] = layer.children.as_slice() else {
            panic!("one boxplot leaf expected: {:?}", layer.children);
        };
        assert!(matches!(leaf.spec.kind, ChartKind::VegaBoxPlot(_)));
    }
}

#[test]
fn vegalite_composition_rejects_unsupported_nodes_in_both_modes() {
    let cases = [
        (
            r#"{"layer":[{"mark":"bar"},{"mark":"line","transform":[{"filter":"datum.x > 1"}]}],"data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}"#,
            "layer[1].transform",
        ),
        (
            r#"{"layer":[{"mark":"bar","data":{"url":"https://example.test/data.json"},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}]}"#,
            "layer[0].data.url",
        ),
        (
            r#"{"mark":"bar","layer":[{"mark":"line"}]}"#,
            "mark cannot be combined with a composition operator",
        ),
        (
            r#"{"layer":[{"mark":"arc","data":{"values":[{"category":"a","value":1}]},"encoding":{"theta":{"field":"value"},"color":{"field":"category"}}}]}"#,
            "layer[0] uses a mark that cannot share a Cartesian layer frame",
        ),
    ];

    for strict in [false, true] {
        for (json, expected_path) in cases {
            let error = vegalite::parse(json, strict).unwrap_err();
            assert!(
                error.contains(expected_path),
                "strict={strict}, expected {expected_path:?} in {error:?}"
            );
        }
    }
}

#[test]
fn composition_guard_counts_reused_data_and_primitives_across_leaves() {
    let json = r#"{
      "data":{"values":[{"x":"A","y":1},{"x":"B","y":2}]},
      "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}},
      "layer":[{"mark":"bar"},{"mark":"bar"}]
    }"#;
    let point_limit = fulgur_chart::guard::InputLimits {
        max_total_data_points: 3,
        ..Default::default()
    };
    let error = vegalite::parse_with_limits(json, false, &point_limit).unwrap_err();
    assert!(error.contains("max_total_data_points"), "{error}");

    let parsed = vegalite::parse(json, false).expect("input parses before primitive guard");
    let primitive_limit = fulgur_chart::guard::InputLimits {
        max_categorical_primitives: 3,
        ..Default::default()
    };
    let error = fulgur_chart::guard::validate_spec(&parsed, &primitive_limit).unwrap_err();
    assert!(
        error.contains("primitive") || error.contains("limit"),
        "{error}"
    );

    let depth_limit = fulgur_chart::guard::InputLimits {
        max_vega_composition_depth: 1,
        ..Default::default()
    };
    assert!(vegalite::parse_with_limits(
        r#"{"layer":[{"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}]}"#,
        false,
        &depth_limit,
    )
    .is_ok());
    let nested = r#"{"layer":[{"layer":[{"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}]}]}"#;
    let error = vegalite::parse_with_limits(nested, false, &depth_limit).unwrap_err();
    assert!(error.contains("max_vega_composition_depth"), "{error}");
}

#[test]
fn composition_guard_aggregates_geoshape_points_and_primitives_across_views() {
    let json = r#"{
      "data":{"values":{"type":"FeatureCollection","features":[
        {"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[1,2]}}
      ]}},
      "hconcat":[{"mark":"geoshape"},{"mark":"geoshape"}]
    }"#;

    let point_limit = fulgur_chart::guard::InputLimits {
        max_total_data_points: 1,
        ..Default::default()
    };
    let error = vegalite::parse_with_limits(json, false, &point_limit).unwrap_err();
    assert!(error.contains("max_total_data_points"), "{error}");

    let primitive_limit = fulgur_chart::guard::InputLimits {
        max_geo_primitives: 1,
        ..Default::default()
    };
    let error = vegalite::parse_with_limits(json, false, &primitive_limit).unwrap_err();
    assert!(error.contains("max_geo_primitives"), "{error}");
}
