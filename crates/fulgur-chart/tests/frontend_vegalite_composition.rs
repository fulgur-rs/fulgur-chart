use fulgur_chart::schema::VegaLiteSpec;

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
