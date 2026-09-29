use serde_json::Value;

#[derive(Debug, PartialEq)]
enum ExpandedCompositionNode {
    Unit(ExpandedUnitSpec),
    Layer(ExpandedCompositionContainer),
    HConcat(ExpandedCompositionContainer),
    VConcat(ExpandedCompositionContainer),
}

#[derive(Debug, PartialEq)]
struct ExpandedCompositionContainer {
    path: String,
    raw_spec: Value,
    children: Vec<ExpandedCompositionNode>,
    effective_width: Option<Value>,
    effective_height: Option<Value>,
    effective_resolve: Option<Value>,
}

#[derive(Debug, PartialEq)]
struct ExpandedUnitSpec {
    effective_spec: Value,
    path: String,
    inherited_resolve: Option<Value>,
}

#[derive(Debug, Default, PartialEq)]
struct CompositionBudget {
    views: usize,
    rows: usize,
}

#[derive(Clone, Debug, Default)]
struct VegaInheritedSpec {
    data: Option<Value>,
    encoding: Option<Value>,
    width: Option<Value>,
    height: Option<Value>,
    resolve: Option<Value>,
}

fn expand_composition(value: &Value, strict: bool) -> Result<ExpandedCompositionNode, String> {
    expand_node(value, &VegaInheritedSpec::default(), strict, "", 0)
}

fn preflight_composition(
    value: &Value,
    limits: &crate::guard::InputLimits,
) -> Result<CompositionBudget, String> {
    let mut budget = CompositionBudget::default();
    preflight_node(
        value,
        &VegaInheritedSpec::default(),
        "",
        0,
        limits,
        &mut budget,
    )?;
    Ok(budget)
}

fn expand_node(
    value: &Value,
    inherited: &VegaInheritedSpec,
    strict: bool,
    path: &str,
    composition_depth: usize,
) -> Result<ExpandedCompositionNode, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{} must be an object", node_path(path)))?;
    reject_unsupported_fields(object, path)?;

    let operators = ["layer", "hconcat", "vconcat"]
        .into_iter()
        .filter(|name| object.contains_key(*name))
        .collect::<Vec<_>>();
    if operators.len() > 1 {
        return Err(format!(
            "{}composition operators cannot be combined",
            node_path(path)
        ));
    }
    let Some(operator) = operators.first().copied() else {
        let mut effective = value.clone();
        let effective_object = effective.as_object_mut().expect("object checked above");
        if !effective_object.contains_key("mark") {
            return Err(format!(
                "{}mark is required for a unit view",
                node_path(path)
            ));
        }
        let data = effective_object
            .get("data")
            .cloned()
            .or_else(|| inherited.data.clone());
        if let Some(data) = data {
            effective_object.insert("data".into(), data);
        }
        let own_encoding = effective_object.get("encoding").cloned();
        let encoding = merge_encoding(inherited.encoding.as_ref(), own_encoding.as_ref(), path)?;
        if let Some(encoding) = encoding {
            effective_object.insert("encoding".into(), encoding);
        }
        for (key, inherited_value) in [("width", &inherited.width), ("height", &inherited.height)] {
            if !effective_object.contains_key(key) {
                if let Some(value) = inherited_value {
                    effective_object.insert(key.into(), value.clone());
                }
            }
        }
        if strict {
            super::vegalite::check_unknown_value(&effective)?;
        }
        return Ok(ExpandedCompositionNode::Unit(ExpandedUnitSpec {
            effective_spec: effective,
            path: path.to_string(),
            inherited_resolve: inherited.resolve.clone(),
        }));
    };

    if object.contains_key("mark") {
        return Err(format!(
            "{}mark cannot be combined with a composition operator",
            node_path(path)
        ));
    }
    let depth = composition_depth + 1;
    validate_composition_object(object, operator, path, strict)?;
    let own = inherited_at_node(object, inherited, operator, path)?;
    let children = object[operator]
        .as_array()
        .ok_or_else(|| format!("{}.{} must be a non-empty array", node_path(path), operator))?;
    if children.is_empty() {
        return Err(format!(
            "{}.{} must be a non-empty array",
            node_path(path),
            operator
        ));
    }
    let mut expanded_children = Vec::with_capacity(children.len());
    for (index, child) in children.iter().enumerate() {
        let child_path = format!("{}[{}]", operator_path(path, operator), index);
        expanded_children.push(expand_node(child, &own, strict, &child_path, depth)?);
    }
    let container = ExpandedCompositionContainer {
        path: path.to_string(),
        raw_spec: value.clone(),
        children: expanded_children,
        effective_width: object
            .get("width")
            .cloned()
            .or_else(|| inherited.width.clone()),
        effective_height: object
            .get("height")
            .cloned()
            .or_else(|| inherited.height.clone()),
        effective_resolve: object
            .get("resolve")
            .cloned()
            .or_else(|| inherited.resolve.clone()),
    };
    match operator {
        "layer" => Ok(ExpandedCompositionNode::Layer(container)),
        "hconcat" => Ok(ExpandedCompositionNode::HConcat(container)),
        "vconcat" => Ok(ExpandedCompositionNode::VConcat(container)),
        _ => unreachable!("operator comes from fixed list"),
    }
}

fn validate_composition_object(
    object: &serde_json::Map<String, Value>,
    operator: &str,
    path: &str,
    strict: bool,
) -> Result<(), String> {
    if operator != "layer" && object.contains_key("encoding") {
        return Err(format!(
            "{}.encoding is only supported on layer nodes",
            node_path(path)
        ));
    }
    if strict {
        let common = [
            "$schema",
            "data",
            "resolve",
            "width",
            "height",
            "title",
            "background",
        ];
        let concat = ["spacing"];
        let mut allowed = common.to_vec();
        allowed.push(operator);
        if operator == "layer" {
            allowed.push("encoding");
        } else {
            allowed.extend(concat);
        }
        if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(format!("{}.{} is not supported", node_path(path), key));
        }
    }
    if let Some(data) = object.get("data") {
        validate_inline_data(data, &format!("{}.data", path_or_root(path)))?;
    }
    if let Some(children) = object.get(operator) {
        let array = children.as_array().ok_or_else(|| {
            format!(
                "{}.{} must be a non-empty array",
                path_or_root(path),
                operator
            )
        })?;
        if array.is_empty() {
            return Err(format!(
                "{}.{} must be a non-empty array",
                path_or_root(path),
                operator
            ));
        }
    }
    Ok(())
}

fn inherited_at_node(
    object: &serde_json::Map<String, Value>,
    inherited: &VegaInheritedSpec,
    operator: &str,
    path: &str,
) -> Result<VegaInheritedSpec, String> {
    let data = match object.get("data") {
        Some(value) => Some(value.clone()),
        None => inherited.data.clone(),
    };
    if let Some(data) = &data {
        validate_inline_data(data, &format!("{}.data", path_or_root(path)))?;
    }
    let encoding = if operator == "layer" {
        merge_encoding(inherited.encoding.as_ref(), object.get("encoding"), path)?
    } else {
        inherited.encoding.clone()
    };
    Ok(VegaInheritedSpec {
        data,
        encoding,
        width: object
            .get("width")
            .cloned()
            .or_else(|| inherited.width.clone()),
        height: object
            .get("height")
            .cloned()
            .or_else(|| inherited.height.clone()),
        resolve: object
            .get("resolve")
            .cloned()
            .or_else(|| inherited.resolve.clone()),
    })
}

fn merge_encoding(
    inherited: Option<&Value>,
    own: Option<&Value>,
    path: &str,
) -> Result<Option<Value>, String> {
    if own.is_none() {
        return Ok(inherited.cloned());
    }
    let own = own.expect("checked above");
    let own_object = own
        .as_object()
        .ok_or_else(|| format!("{}.encoding must be an object", path_or_root(path)))?;
    let mut merged = inherited
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| format!("{}.encoding must be an object", path_or_root(path)))
        })
        .transpose()?
        .unwrap_or_default();
    for (channel, definition) in own_object {
        merged.insert(channel.clone(), definition.clone());
    }
    Ok(Some(Value::Object(merged)))
}

fn reject_unsupported_fields(
    object: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), String> {
    for unsupported in ["transform", "facet", "repeat", "concat"] {
        if object.contains_key(unsupported) {
            return Err(format!(
                "{}.{} is not supported",
                path_or_root(path),
                unsupported
            ));
        }
    }
    if let Some(data) = object.get("data") {
        validate_inline_data(data, &format!("{}.data", path_or_root(path)))?;
    }
    Ok(())
}

fn validate_inline_data(data: &Value, path: &str) -> Result<(), String> {
    if let Some(object) = data.as_object()
        && object.contains_key("url")
    {
        return Err(format!(
            "{path}.url is not supported; provide inline data.values"
        ));
    }
    Ok(())
}

fn preflight_node(
    value: &Value,
    inherited: &VegaInheritedSpec,
    path: &str,
    composition_depth: usize,
    limits: &crate::guard::InputLimits,
    budget: &mut CompositionBudget,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{} must be an object", node_path(path)))?;
    reject_unsupported_fields(object, path)?;
    let operators = ["layer", "hconcat", "vconcat"]
        .into_iter()
        .filter(|name| object.contains_key(*name))
        .collect::<Vec<_>>();
    if operators.len() > 1 {
        return Err(format!(
            "{}composition operators cannot be combined",
            node_path(path)
        ));
    }
    let Some(operator) = operators.first().copied() else {
        if !object.contains_key("mark") {
            return Err(format!(
                "{}mark is required for a unit view",
                node_path(path)
            ));
        }
        let effective_data = object.get("data").or(inherited.data.as_ref());
        if let Some(data) = effective_data {
            validate_inline_data(data, &format!("{}.data", path_or_root(path)))?;
        }
        let rows = inline_row_count(effective_data);
        budget.views = budget.views.saturating_add(1);
        budget.rows = budget.rows.saturating_add(rows);
        if budget.views > limits.max_vega_composition_views {
            return Err(format!(
                "{}composition exceeds max_vega_composition_views ({})",
                node_path(path),
                limits.max_vega_composition_views
            ));
        }
        if budget.rows > limits.max_total_data_points {
            return Err(format!(
                "{}composition exceeds max_total_data_points ({})",
                node_path(path),
                limits.max_total_data_points
            ));
        }
        return Ok(());
    };
    if object.contains_key("mark") {
        return Err(format!(
            "{}mark cannot be combined with a composition operator",
            node_path(path)
        ));
    }
    let depth = composition_depth.saturating_add(1);
    if depth > limits.max_vega_composition_depth {
        return Err(format!(
            "{}composition exceeds max_vega_composition_depth ({})",
            node_path(path),
            limits.max_vega_composition_depth
        ));
    }
    let own = inherited_at_node(object, inherited, operator, path)?;
    let children = object[operator]
        .as_array()
        .ok_or_else(|| format!("{}.{} must be a non-empty array", node_path(path), operator))?;
    if children.is_empty() {
        return Err(format!(
            "{}.{} must be a non-empty array",
            node_path(path),
            operator
        ));
    }
    for (index, child) in children.iter().enumerate() {
        let child_path = format!("{}[{}]", operator_path(path, operator), index);
        preflight_node(child, &own, &child_path, depth, limits, budget)?;
    }
    Ok(())
}

fn inline_row_count(data: Option<&Value>) -> usize {
    data.and_then(Value::as_object)
        .and_then(|object| object.get("values"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn node_path(path: &str) -> String {
    if path.is_empty() {
        "root: ".to_string()
    } else {
        format!("{path}: ")
    }
}

fn path_or_root(path: &str) -> &str {
    if path.is_empty() { "root" } else { path }
}

fn operator_path(path: &str, operator: &str) -> String {
    if path.is_empty() {
        operator.to_string()
    } else {
        format!("{path}.{operator}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn expand(json: Value, strict: bool) -> Result<ExpandedCompositionNode, String> {
        expand_composition(&json, strict)
    }

    fn units(node: &ExpandedCompositionNode) -> Vec<&ExpandedUnitSpec> {
        fn visit<'a>(node: &'a ExpandedCompositionNode, out: &mut Vec<&'a ExpandedUnitSpec>) {
            match node {
                ExpandedCompositionNode::Unit(unit) => out.push(unit),
                ExpandedCompositionNode::Layer(container)
                | ExpandedCompositionNode::HConcat(container)
                | ExpandedCompositionNode::VConcat(container) => {
                    for child in &container.children {
                        visit(child, out);
                    }
                }
            }
        }

        let mut output = Vec::new();
        visit(node, &mut output);
        output
    }

    #[test]
    fn vegalite_composition_inherits_and_overrides_data_and_encoding() {
        let value = json!({
            "data": {"values": [{"day": "Mon", "base": 4, "current": 5}]},
            "encoding": {
                "x": {"field": "day", "type": "nominal"},
                "y": {"field": "base", "type": "quantitative"}
            },
            "layer": [
                {"mark": "bar"},
                {"mark": "line", "encoding": {"y": {"field": "current", "type": "quantitative"}}}
            ]
        });

        let expanded = expand(value, true).expect("layer should expand");
        let leaves = units(&expanded);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].effective_spec["data"]["values"][0]["base"], 4);
        assert_eq!(leaves[0].effective_spec["encoding"]["x"]["field"], "day");
        assert_eq!(leaves[0].effective_spec["encoding"]["y"]["field"], "base");
        assert_eq!(leaves[1].effective_spec["encoding"]["x"]["field"], "day");
        assert_eq!(
            leaves[1].effective_spec["encoding"]["y"]["field"],
            "current"
        );
    }

    #[test]
    fn vegalite_composition_child_data_replaces_parent_data() {
        let value = json!({
            "data": {"values": [{"day": "parent", "value": 1}]},
            "layer": [
                {
                    "mark": "bar",
                    "data": {"values": [{"day": "child", "value": 9}]},
                    "encoding": {"x": {"field": "day"}, "y": {"field": "value"}}
                },
                {"mark": "line", "encoding": {"x": {"field": "day"}, "y": {"field": "value"}}}
            ]
        });

        let expanded = expand(value, false).expect("child data should replace parent data");
        let leaves = units(&expanded);
        assert_eq!(
            leaves[0].effective_spec["data"]["values"][0]["day"],
            "child"
        );
        assert_eq!(
            leaves[1].effective_spec["data"]["values"][0]["day"],
            "parent"
        );
    }

    #[test]
    fn vegalite_composition_rejects_unsupported_nodes_in_both_modes() {
        let cases = [
            (
                json!({"layer": [{"mark": "bar", "transform": []}]}),
                "layer[0].transform",
            ),
            (json!({"layer": [{"mark": "bar"}], "facet": {}}), "facet"),
            (json!({"layer": [{"mark": "bar"}], "repeat": {}}), "repeat"),
            (json!({"layer": [{"mark": "bar"}], "concat": []}), "concat"),
            (
                json!({"layer": [{"mark": "bar", "data": {"url": "https://example.test/data.json"}}]}),
                "layer[0].data.url",
            ),
            (
                json!({"layer": [{"mark": "bar"}], "hconcat": [{"mark": "line"}]}),
                "composition operators",
            ),
        ];

        for strict in [false, true] {
            for (value, expected_path) in &cases {
                let error = expand(value.clone(), strict).expect_err("unsupported node must fail");
                assert!(
                    error.contains(expected_path),
                    "expected path {expected_path:?}, got {error:?}"
                );
            }
        }
    }

    #[test]
    fn composition_guard_checks_depth_and_view_boundaries() {
        let nested = json!({
            "layer": [{"layer": [{"mark": "bar"}]}],
            "data": {"values": [{"x": 1}]}
        });
        assert!(preflight(&nested, 2, 1, 1).is_ok());
        assert!(preflight(&nested, 1, 1, 1).is_err());

        let two_views = json!({
            "layer": [{"mark": "bar"}, {"mark": "line"}],
            "data": {"values": [{"x": 1}]}
        });
        assert!(preflight(&two_views, 1, 2, 2).is_ok());
        assert!(preflight(&two_views, 1, 1, 2).is_err());
    }

    #[test]
    fn composition_guard_counts_reused_data_across_leaves() {
        let value = json!({
            "layer": [{"mark": "bar"}, {"mark": "line"}],
            "data": {"values": [{"x": 1}, {"x": 2}]}
        });
        let budget = preflight(&value, 1, 2, 4).expect("4 inherited rows fit");
        assert_eq!(budget.views, 2);
        assert_eq!(budget.rows, 4);
        assert!(preflight(&value, 1, 2, 3).is_err());
    }

    fn preflight(
        value: &Value,
        depth: usize,
        views: usize,
        points: usize,
    ) -> Result<CompositionBudget, String> {
        let limits = crate::guard::InputLimits {
            max_vega_composition_depth: depth,
            max_vega_composition_views: views,
            max_total_data_points: points,
            ..crate::guard::InputLimits::default()
        };
        preflight_composition(value, &limits)
    }
}
