use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

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

#[derive(Clone, Debug, Default, PartialEq)]
struct ResolvedScaleInput {
    leaf_scales: BTreeMap<String, crate::ir::VegaLeafScaleDomains>,
    node_resolve: BTreeMap<String, crate::ir::VegaCompositionResolve>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct VegaUnitScaleOverrides {
    pub(super) color_categories: Option<Vec<String>>,
    pub(super) color_numeric_domain: Option<(f64, f64)>,
    pub(super) size_numeric_domain: Option<(f64, f64)>,
}

impl VegaUnitScaleOverrides {
    pub(super) fn from_domains(domains: &crate::ir::VegaLeafScaleDomains) -> Self {
        use crate::ir::VegaScaleDomain;
        let mut overrides = Self::default();
        match &domains.color {
            Some(VegaScaleDomain::Categories(categories)) => {
                overrides.color_categories = Some(categories.clone());
            }
            Some(VegaScaleDomain::Numeric { min, max }) => {
                overrides.color_numeric_domain = Some((*min, *max));
            }
            Some(VegaScaleDomain::Temporal { .. }) | None => {}
        }
        if let Some(VegaScaleDomain::Numeric { min, max }) = &domains.size {
            overrides.size_numeric_domain = Some((*min, *max));
        }
        overrides
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ResolveOverrides {
    x_scale: Option<crate::ir::VegaResolutionMode>,
    y_scale: Option<crate::ir::VegaResolutionMode>,
    color_scale: Option<crate::ir::VegaResolutionMode>,
    size_scale: Option<crate::ir::VegaResolutionMode>,
    x_axis: Option<crate::ir::VegaResolutionMode>,
    y_axis: Option<crate::ir::VegaResolutionMode>,
    color_legend: Option<crate::ir::VegaResolutionMode>,
    size_legend: Option<crate::ir::VegaResolutionMode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RawScaleType {
    Categories,
    Numeric,
    Temporal,
}

#[derive(Clone, Debug)]
struct RawUnitScales {
    domains: crate::ir::VegaLeafScaleDomains,
    kinds: BTreeMap<&'static str, RawScaleType>,
}

#[derive(Clone, Debug)]
enum ResolvedRawNode {
    Unit {
        path: String,
        scales: RawUnitScales,
    },
    Layer {
        path: String,
        resolve: crate::ir::VegaCompositionResolve,
        children: Vec<ResolvedRawNode>,
    },
    HConcat {
        path: String,
        resolve: crate::ir::VegaCompositionResolve,
        children: Vec<ResolvedRawNode>,
    },
    VConcat {
        path: String,
        resolve: crate::ir::VegaCompositionResolve,
        children: Vec<ResolvedRawNode>,
    },
}

fn resolve_raw_color_size_scales(
    node: &ExpandedCompositionNode,
) -> Result<ResolvedScaleInput, String> {
    let mut output = ResolvedScaleInput::default();
    let tree = resolve_raw_node(node, ResolveOverrides::default(), &mut output)?;
    validate_shared_channel_types(&tree)?;

    let mut color_inherited = None;
    let mut size_inherited = None;
    assign_leaf_domains(
        &tree,
        &mut color_inherited,
        &mut size_inherited,
        &mut output.leaf_scales,
    )?;
    Ok(output)
}

fn resolve_raw_node(
    node: &ExpandedCompositionNode,
    inherited: ResolveOverrides,
    output: &mut ResolvedScaleInput,
) -> Result<ResolvedRawNode, String> {
    match node {
        ExpandedCompositionNode::Unit(unit) => Ok(ResolvedRawNode::Unit {
            path: unit.path.clone(),
            scales: raw_unit_scales(&unit.effective_spec, &unit.path)?,
        }),
        ExpandedCompositionNode::Layer(container) => {
            resolve_raw_container(container, &container.children, "layer", inherited, output)
        }
        ExpandedCompositionNode::HConcat(container) => {
            resolve_raw_container(container, &container.children, "hconcat", inherited, output)
        }
        ExpandedCompositionNode::VConcat(container) => {
            resolve_raw_container(container, &container.children, "vconcat", inherited, output)
        }
    }
}

fn resolve_raw_container(
    container: &ExpandedCompositionContainer,
    children: &[ExpandedCompositionNode],
    operator: &str,
    inherited: ResolveOverrides,
    output: &mut ResolvedScaleInput,
) -> Result<ResolvedRawNode, String> {
    let own = parse_resolve_overrides(
        inherited,
        container.raw_spec.get("resolve"),
        &container.path,
    )?;
    let resolve = resolved_modes(operator, own, &container.path)?;
    output.node_resolve.insert(container.path.clone(), resolve);
    let mut resolved_children = Vec::with_capacity(children.len());
    for child in children {
        resolved_children.push(resolve_raw_node(child, own, output)?);
    }
    let path = container.path.clone();
    Ok(match operator {
        "layer" => ResolvedRawNode::Layer {
            path,
            resolve,
            children: resolved_children,
        },
        "hconcat" => ResolvedRawNode::HConcat {
            path,
            resolve,
            children: resolved_children,
        },
        "vconcat" => ResolvedRawNode::VConcat {
            path,
            resolve,
            children: resolved_children,
        },
        _ => unreachable!("operator comes from fixed list"),
    })
}

fn parse_resolve_overrides(
    inherited: ResolveOverrides,
    value: Option<&Value>,
    path: &str,
) -> Result<ResolveOverrides, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(inherited);
    };
    let resolve_path = format!("{}.resolve", path_or_root(path));
    let object = value
        .as_object()
        .ok_or_else(|| format!("{resolve_path} must be an object"))?;
    for key in object.keys() {
        if !["scale", "axis", "legend"].contains(&key.as_str()) {
            return Err(format!("{resolve_path}.{key} is not supported"));
        }
    }
    let mut result = inherited;
    if let Some(scale) = object.get("scale").filter(|value| !value.is_null()) {
        let scale = scale
            .as_object()
            .ok_or_else(|| format!("{resolve_path}.scale must be an object"))?;
        for (channel, value) in scale {
            let target = match channel.as_str() {
                "x" => &mut result.x_scale,
                "y" => &mut result.y_scale,
                "color" => &mut result.color_scale,
                "size" => &mut result.size_scale,
                _ => return Err(format!("{resolve_path}.scale.{channel} is not supported")),
            };
            *target = Some(parse_resolve_mode(
                value,
                &format!("{resolve_path}.scale.{channel}"),
            )?);
        }
    }
    if let Some(axis) = object.get("axis").filter(|value| !value.is_null()) {
        let axis = axis
            .as_object()
            .ok_or_else(|| format!("{resolve_path}.axis must be an object"))?;
        for (channel, value) in axis {
            let target = match channel.as_str() {
                "x" => &mut result.x_axis,
                "y" => &mut result.y_axis,
                _ => return Err(format!("{resolve_path}.axis.{channel} is not supported")),
            };
            *target = Some(parse_resolve_mode(
                value,
                &format!("{resolve_path}.axis.{channel}"),
            )?);
        }
    }
    if let Some(legend) = object.get("legend").filter(|value| !value.is_null()) {
        let legend = legend
            .as_object()
            .ok_or_else(|| format!("{resolve_path}.legend must be an object"))?;
        for (channel, value) in legend {
            let target = match channel.as_str() {
                "color" => &mut result.color_legend,
                "size" => &mut result.size_legend,
                _ => return Err(format!("{resolve_path}.legend.{channel} is not supported")),
            };
            *target = Some(parse_resolve_mode(
                value,
                &format!("{resolve_path}.legend.{channel}"),
            )?);
        }
    }
    Ok(result)
}

fn parse_resolve_mode(value: &Value, path: &str) -> Result<crate::ir::VegaResolutionMode, String> {
    match value.as_str() {
        Some("shared") => Ok(crate::ir::VegaResolutionMode::Shared),
        Some("independent") => Ok(crate::ir::VegaResolutionMode::Independent),
        _ => Err(format!("{path} must be \"shared\" or \"independent\"")),
    }
}

fn resolved_modes(
    operator: &str,
    overrides: ResolveOverrides,
    path: &str,
) -> Result<crate::ir::VegaCompositionResolve, String> {
    use crate::ir::VegaResolutionMode::{Independent, Shared};
    let is_layer = operator == "layer";
    let x_scale = overrides
        .x_scale
        .unwrap_or(if is_layer { Shared } else { Independent });
    let y_scale = overrides
        .y_scale
        .unwrap_or(if is_layer { Shared } else { Independent });
    let color_scale = overrides.color_scale.unwrap_or(Shared);
    let size_scale = overrides.size_scale.unwrap_or(Shared);
    let x_axis = overrides.x_axis.unwrap_or(if x_scale == Independent {
        Independent
    } else if is_layer {
        Shared
    } else {
        Independent
    });
    let y_axis = overrides.y_axis.unwrap_or(if y_scale == Independent {
        Independent
    } else if is_layer {
        Shared
    } else {
        Independent
    });
    let color_legend = overrides
        .color_legend
        .unwrap_or(if color_scale == Independent {
            Independent
        } else {
            Shared
        });
    let size_legend = overrides
        .size_legend
        .unwrap_or(if size_scale == Independent {
            Independent
        } else {
            Shared
        });
    if !is_layer && (x_axis == Shared || y_axis == Shared) {
        return Err(format!(
            "{}.resolve.axis shared is not supported for concat nodes",
            path_or_root(path)
        ));
    }
    for (scale, guide, channel) in [
        (x_scale, x_axis, "x"),
        (y_scale, y_axis, "y"),
        (color_scale, color_legend, "color"),
        (size_scale, size_legend, "size"),
    ] {
        if scale == Independent && guide == Shared {
            let group = if channel == "x" || channel == "y" {
                "axis"
            } else {
                "legend"
            };
            return Err(format!(
                "{}.resolve.{group}.{channel} cannot be shared with an independent scale",
                path_or_root(path)
            ));
        }
    }
    Ok(crate::ir::VegaCompositionResolve {
        x_scale,
        y_scale,
        color_scale,
        size_scale,
        x_axis,
        y_axis,
        color_legend,
        size_legend,
    })
}

fn raw_unit_scales(spec: &Value, path: &str) -> Result<RawUnitScales, String> {
    let object = spec
        .as_object()
        .ok_or_else(|| format!("{} must be an object", node_path(path)))?;
    let encoding = object
        .get("encoding")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mark = object
        .get("mark")
        .and_then(Value::as_str)
        .or_else(|| {
            object
                .get("mark")
                .and_then(Value::as_object)
                .and_then(|m| m.get("type"))?
                .as_str()
        })
        .unwrap_or_default();
    let records = raw_scale_records(object, mark);
    let mut domains = crate::ir::VegaLeafScaleDomains::default();
    let mut kinds = BTreeMap::new();
    for channel in ["x", "y", "color", "size"] {
        let Some(binding) = encoding.get(channel).and_then(Value::as_object) else {
            continue;
        };
        let Some(field) = binding.get("field").and_then(Value::as_str) else {
            continue;
        };
        let kind = raw_channel_kind(channel, mark, binding, &records, field)
            .map_err(|error| format!("{}.encoding.{channel}: {error}", path_or_root(path)))?;
        kinds.insert(channel, kind);
        let domain = raw_channel_domain(kind, &records, field);
        match channel {
            "x" => domains.x = domain,
            "y" => domains.y = domain,
            "color" => domains.color = domain,
            "size" => domains.size = domain,
            _ => unreachable!(),
        }
    }
    Ok(RawUnitScales { domains, kinds })
}

fn raw_channel_kind(
    channel: &str,
    mark: &str,
    binding: &serde_json::Map<String, Value>,
    records: &[serde_json::Map<String, Value>],
    field: &str,
) -> Result<RawScaleType, String> {
    use RawScaleType::{Categories, Numeric, Temporal};
    let hinted = binding
        .get("type")
        .filter(|value| !value.is_null())
        .map(|hint| match hint.as_str() {
            Some("quantitative") => Ok(Numeric),
            Some("temporal") => Ok(Temporal),
            Some("nominal" | "ordinal") => Ok(Categories),
            Some(other) => Err(format!("unsupported scale type {other:?}")),
            None => Err("type must be a string".into()),
        })
        .transpose()?;
    if channel == "color" && !matches!(mark, "rect" | "geoshape") {
        return Ok(Categories);
    }
    if let Some(hinted) = hinted {
        return Ok(hinted);
    }
    Ok(match channel {
        "size" => Numeric,
        "color"
            if matches!(mark, "rect" | "geoshape")
                && records
                    .iter()
                    .any(|record| record.get(field).is_some_and(Value::is_number)) =>
        {
            Numeric
        }
        "x" if mark == "point" || mark == "circle" || mark == "square" => Numeric,
        "x" if mark == "rect" => Categories,
        "x" => Categories,
        "y" if matches!(mark, "rect") => Categories,
        "y" => Numeric,
        _ => Categories,
    })
}

fn raw_scale_records(
    object: &serde_json::Map<String, Value>,
    mark: &str,
) -> Vec<serde_json::Map<String, Value>> {
    let Some(values) = object
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
    else {
        return Vec::new();
    };
    if mark == "geoshape" {
        if values
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "FeatureCollection")
        {
            return values
                .get("features")
                .and_then(Value::as_array)
                .map(|features| features.iter().filter_map(raw_scale_record).collect())
                .unwrap_or_default();
        }
        if values
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "Feature")
        {
            return raw_scale_record(values).into_iter().collect();
        }
    }
    if let Some(array) = values.as_array() {
        return array.iter().filter_map(raw_scale_record).collect();
    }
    raw_scale_record(values).into_iter().collect()
}

fn raw_scale_record(value: &Value) -> Option<serde_json::Map<String, Value>> {
    let mut record = value.as_object()?.clone();
    if let Some(properties) = record.get("properties").and_then(Value::as_object).cloned() {
        for (key, value) in properties {
            record.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Some(record)
}

fn raw_channel_domain(
    kind: RawScaleType,
    records: &[serde_json::Map<String, Value>],
    field: &str,
) -> Option<crate::ir::VegaScaleDomain> {
    use crate::ir::VegaScaleDomain;
    match kind {
        RawScaleType::Categories => {
            let mut seen = HashSet::new();
            let values = records
                .iter()
                .filter_map(|record| record.get(field))
                .filter_map(raw_category_value)
                .filter(|value| seen.insert(value.clone()))
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(VegaScaleDomain::Categories(values))
        }
        RawScaleType::Numeric => numeric_field_domain(records, field)
            .map(|(min, max)| VegaScaleDomain::Numeric { min, max }),
        RawScaleType::Temporal => {
            let mut values = records
                .iter()
                .filter_map(|record| record.get(field).and_then(Value::as_str))
                .filter_map(|value| crate::temporal::parse_rfc3339_millis(field, value).ok());
            let first = values.next()?;
            let (min, max) = values.fold((first, first), |(min, max), value| {
                (min.min(value), max.max(value))
            });
            Some(VegaScaleDomain::Temporal {
                min_millis: min,
                max_millis: max,
            })
        }
    }
}

fn raw_category_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn numeric_field_domain(
    records: &[serde_json::Map<String, Value>],
    field: &str,
) -> Option<(f64, f64)> {
    let mut values = records
        .iter()
        .filter_map(|record| record.get(field).and_then(Value::as_f64))
        .filter(|value| value.is_finite());
    let first = values.next()?;
    Some(values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

fn validate_shared_channel_types(node: &ResolvedRawNode) -> Result<(), String> {
    for channel in ["x", "y", "color", "size"] {
        let Some(resolve) = raw_node_resolve(node) else {
            continue;
        };
        if scale_mode(resolve, channel) != crate::ir::VegaResolutionMode::Shared {
            continue;
        }
        let mut kinds = Vec::new();
        collect_shared_channel_types(node, channel, &mut kinds);
        if let Some((first_path, first_kind)) = kinds.first()
            && let Some((other_path, other_kind)) = kinds
                .iter()
                .find(|(_, other_kind)| other_kind != first_kind)
        {
            let path = raw_node_path(node);
            return Err(format!(
                "{}.resolve.scale.{channel} has incompatible scale types: {first_kind:?} at {first_path} and {other_kind:?} at {other_path}",
                path_or_root(path)
            ));
        }
    }
    match node {
        ResolvedRawNode::Unit { .. } => Ok(()),
        ResolvedRawNode::Layer { children, .. }
        | ResolvedRawNode::HConcat { children, .. }
        | ResolvedRawNode::VConcat { children, .. } => {
            for child in children {
                validate_shared_channel_types(child)?;
            }
            Ok(())
        }
    }
}

fn collect_shared_channel_types<'a>(
    node: &'a ResolvedRawNode,
    channel: &str,
    output: &mut Vec<(&'a str, RawScaleType)>,
) {
    match node {
        ResolvedRawNode::Unit { path, scales } => {
            if let Some(kind) = scales.kinds.get(channel) {
                output.push((path, *kind));
            }
        }
        ResolvedRawNode::Layer {
            resolve, children, ..
        }
        | ResolvedRawNode::HConcat {
            resolve, children, ..
        }
        | ResolvedRawNode::VConcat {
            resolve, children, ..
        } => {
            if scale_mode(resolve, channel) == crate::ir::VegaResolutionMode::Shared {
                for child in children {
                    collect_shared_channel_types(child, channel, output);
                }
            }
        }
    }
}

fn raw_node_resolve(node: &ResolvedRawNode) -> Option<&crate::ir::VegaCompositionResolve> {
    match node {
        ResolvedRawNode::Unit { .. } => None,
        ResolvedRawNode::Layer { resolve, .. }
        | ResolvedRawNode::HConcat { resolve, .. }
        | ResolvedRawNode::VConcat { resolve, .. } => Some(resolve),
    }
}

fn raw_node_path(node: &ResolvedRawNode) -> &str {
    match node {
        ResolvedRawNode::Unit { path, .. }
        | ResolvedRawNode::Layer { path, .. }
        | ResolvedRawNode::HConcat { path, .. }
        | ResolvedRawNode::VConcat { path, .. } => path,
    }
}

fn scale_mode(
    resolve: &crate::ir::VegaCompositionResolve,
    channel: &str,
) -> crate::ir::VegaResolutionMode {
    match channel {
        "x" => resolve.x_scale,
        "y" => resolve.y_scale,
        "color" => resolve.color_scale,
        "size" => resolve.size_scale,
        _ => unreachable!("scale channel is fixed"),
    }
}

fn raw_node_domains(
    node: &ResolvedRawNode,
    channel: &str,
) -> Result<Option<crate::ir::VegaScaleDomain>, String> {
    if let ResolvedRawNode::Unit { scales, .. } = node {
        return Ok(match channel {
            "x" => scales.domains.x.clone(),
            "y" => scales.domains.y.clone(),
            "color" => scales.domains.color.clone(),
            "size" => scales.domains.size.clone(),
            _ => unreachable!("scale channel is fixed"),
        });
    }
    let Some(resolve) = raw_node_resolve(node) else {
        unreachable!("composition node has resolution")
    };
    if scale_mode(resolve, channel) == crate::ir::VegaResolutionMode::Independent {
        return Ok(None);
    }
    let mut domain = None;
    for child in raw_node_children(node).expect("composition node has children") {
        if let Some(child_domain) = raw_node_domains(child, channel)? {
            domain = Some(union_scale_domains(
                domain,
                child_domain,
                &format!(
                    "{}.resolve.scale.{channel}",
                    path_or_root(raw_node_path(node))
                ),
            )?);
        }
    }
    Ok(domain)
}

fn raw_node_children(node: &ResolvedRawNode) -> Option<&[ResolvedRawNode]> {
    match node {
        ResolvedRawNode::Unit { .. } => None,
        ResolvedRawNode::Layer { children, .. }
        | ResolvedRawNode::HConcat { children, .. }
        | ResolvedRawNode::VConcat { children, .. } => Some(children),
    }
}

fn union_scale_domains(
    current: Option<crate::ir::VegaScaleDomain>,
    next: crate::ir::VegaScaleDomain,
    path: &str,
) -> Result<crate::ir::VegaScaleDomain, String> {
    use crate::ir::VegaScaleDomain;
    let Some(current) = current else {
        return Ok(next);
    };
    match (current, next) {
        (VegaScaleDomain::Categories(mut left), VegaScaleDomain::Categories(right)) => {
            let mut seen = left.iter().cloned().collect::<HashSet<_>>();
            for value in right {
                if seen.insert(value.clone()) {
                    left.push(value);
                }
            }
            Ok(VegaScaleDomain::Categories(left))
        }
        (
            VegaScaleDomain::Numeric {
                min: left_min,
                max: left_max,
            },
            VegaScaleDomain::Numeric {
                min: right_min,
                max: right_max,
            },
        ) => Ok(VegaScaleDomain::Numeric {
            min: left_min.min(right_min),
            max: left_max.max(right_max),
        }),
        (
            VegaScaleDomain::Temporal {
                min_millis: left_min,
                max_millis: left_max,
            },
            VegaScaleDomain::Temporal {
                min_millis: right_min,
                max_millis: right_max,
            },
        ) => Ok(VegaScaleDomain::Temporal {
            min_millis: left_min.min(right_min),
            max_millis: left_max.max(right_max),
        }),
        (left, right) => Err(format!(
            "{path} has incompatible domains {left:?} and {right:?}"
        )),
    }
}

fn assign_leaf_domains(
    node: &ResolvedRawNode,
    inherited_color: &mut Option<crate::ir::VegaScaleDomain>,
    inherited_size: &mut Option<crate::ir::VegaScaleDomain>,
    output: &mut BTreeMap<String, crate::ir::VegaLeafScaleDomains>,
) -> Result<(), String> {
    match node {
        ResolvedRawNode::Unit { path, scales } => {
            output.insert(
                path.clone(),
                crate::ir::VegaLeafScaleDomains {
                    color: inherited_color
                        .clone()
                        .or_else(|| scales.domains.color.clone()),
                    size: inherited_size
                        .clone()
                        .or_else(|| scales.domains.size.clone()),
                    ..crate::ir::VegaLeafScaleDomains::default()
                },
            );
        }
        ResolvedRawNode::Layer {
            resolve, children, ..
        }
        | ResolvedRawNode::HConcat {
            resolve, children, ..
        }
        | ResolvedRawNode::VConcat {
            resolve, children, ..
        } => {
            let old_color = inherited_color.clone();
            let old_size = inherited_size.clone();
            if resolve.color_scale == crate::ir::VegaResolutionMode::Shared {
                *inherited_color = old_color.clone().or(raw_node_domains(node, "color")?);
            } else {
                *inherited_color = None;
            }
            if resolve.size_scale == crate::ir::VegaResolutionMode::Shared {
                *inherited_size = old_size.clone().or(raw_node_domains(node, "size")?);
            } else {
                *inherited_size = None;
            }
            for child in children {
                assign_leaf_domains(child, inherited_color, inherited_size, output)?;
            }
            *inherited_color = old_color;
            *inherited_size = old_size;
        }
    }
    Ok(())
}

fn parse_resolved_composition(
    node: ExpandedCompositionNode,
    scales: &ResolvedScaleInput,
    strict: bool,
    limits: &crate::guard::InputLimits,
) -> Result<crate::ir::VegaCompositionNode, String> {
    let mut parsed = parse_resolved_node(node, scales, strict, limits)?;
    finalize_position_domains(&mut parsed)?;
    Ok(parsed)
}

pub(super) fn parse_composition_value(
    value: &Value,
    strict: bool,
    limits: &crate::guard::InputLimits,
) -> Result<crate::ir::ChartSpec, String> {
    preflight_composition(value, limits)?;
    let expanded = expand_composition(value, strict)?;
    let resolved_scales = resolve_raw_color_size_scales(&expanded)?;
    let node = parse_resolved_composition(expanded, &resolved_scales, strict, limits)?;
    let (width, height) = composition_node_dimensions(&node);
    let first_leaf = first_unit_leaf(&node)
        .ok_or_else(|| "composition must contain at least one unit view".to_string())?;
    let mut root = (*first_leaf.spec).clone();
    root.kind = crate::ir::ChartKind::VegaComposition(Box::new(node.clone()));
    root.series.clear();
    root.categories.clear();
    root.width = width;
    root.height = height;
    root.title = composition_node_title(&node).map(str::to_owned);
    // Composition backgrounds are retained on their own IR nodes and painted by the
    // composition layout, so child leaf backgrounds cannot accidentally cover the root.
    root.theme.background = None;
    crate::guard::validate_vega_composition(&root, limits)?;
    Ok(root)
}

fn first_unit_leaf(
    node: &crate::ir::VegaCompositionNode,
) -> Option<&crate::ir::VegaCompositionLeaf> {
    match node {
        crate::ir::VegaCompositionNode::Unit(leaf) => Some(leaf),
        crate::ir::VegaCompositionNode::Layer(layer) => {
            layer.children.iter().find_map(first_unit_leaf)
        }
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => {
            concat.children.iter().find_map(first_unit_leaf)
        }
    }
}

fn composition_node_title(node: &crate::ir::VegaCompositionNode) -> Option<&str> {
    match node {
        crate::ir::VegaCompositionNode::Unit(_) => None,
        crate::ir::VegaCompositionNode::Layer(layer) => layer.title.as_deref(),
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => concat.title.as_deref(),
    }
}

fn parse_resolved_node(
    node: ExpandedCompositionNode,
    scales: &ResolvedScaleInput,
    strict: bool,
    limits: &crate::guard::InputLimits,
) -> Result<crate::ir::VegaCompositionNode, String> {
    use crate::ir::{VegaCompositionNode, VegaConcatNode, VegaLayerNode};
    match node {
        ExpandedCompositionNode::Unit(unit) => {
            let mut effective = unit.effective_spec;
            validate_explicit_dimension(effective.get("width"), &unit.path, "width", limits)?;
            validate_explicit_dimension(effective.get("height"), &unit.path, "height", limits)?;
            let leaf_scales = scales
                .leaf_scales
                .get(&unit.path)
                .cloned()
                .unwrap_or_default();
            let overrides = VegaUnitScaleOverrides::from_domains(&leaf_scales);
            let spec = super::vegalite::parse_unit_value_with_overrides(
                &mut effective,
                strict,
                limits,
                &overrides,
            )
            .map_err(|error| prefix_path(&unit.path, error))?;
            validate_parsed_dimensions(&spec, &unit.path, limits)?;
            let mut resolved_scales = parsed_leaf_domains(&spec);
            resolved_scales.color = leaf_scales.color;
            resolved_scales.size = leaf_scales.size;
            Ok(VegaCompositionNode::Unit(Box::new(
                crate::ir::VegaCompositionLeaf {
                    path: unit.path,
                    spec: Box::new(spec),
                    scales: resolved_scales,
                },
            )))
        }
        ExpandedCompositionNode::Layer(container) => {
            validate_explicit_dimension(
                container.effective_width.as_ref(),
                &container.path,
                "width",
                limits,
            )?;
            validate_explicit_dimension(
                container.effective_height.as_ref(),
                &container.path,
                "height",
                limits,
            )?;
            let children = parse_resolved_children(container.children, scales, strict, limits)?;
            validate_layer_children(&children, &container.path)?;
            let first = children.first().ok_or_else(|| {
                format!("{}.layer must be non-empty", path_or_root(&container.path))
            })?;
            let (view_width, view_height) = composition_node_dimensions(first);
            for child in &children[1..] {
                let (child_width, child_height) = composition_node_dimensions(child);
                if child_width != view_width || child_height != view_height {
                    return Err(format!(
                        "{}layer children must resolve to equal dimensions (expected {view_width}×{view_height}, got {child_width}×{child_height})",
                        path_or_root(&container.path)
                    ));
                }
            }
            let resolve = node_resolution(scales, &container.path)?;
            let title = composition_title(&container.raw_spec, &container.path, limits)?;
            let title_height = title
                .as_ref()
                .map(|_| crate::layout::common::TITLE_BAND)
                .unwrap_or(0.0);
            let independent_axes = children.len().saturating_sub(1) as f64;
            let width = view_width
                + if resolve.y_axis == crate::ir::VegaResolutionMode::Independent {
                    independent_axes * 36.0
                } else {
                    0.0
                };
            let height = view_height
                + if resolve.x_axis == crate::ir::VegaResolutionMode::Independent {
                    independent_axes * 36.0
                } else {
                    0.0
                }
                + title_height;
            check_composition_dimension(width, &container.path, "width", limits)?;
            check_composition_dimension(height, &container.path, "height", limits)?;
            Ok(VegaCompositionNode::Layer(Box::new(VegaLayerNode {
                path: container.path.clone(),
                children,
                width,
                height,
                view_width,
                view_height,
                title,
                background: composition_background(&container.raw_spec, &container.path)?,
                resolve,
            })))
        }
        ExpandedCompositionNode::HConcat(container) => {
            validate_explicit_dimension(
                container.effective_width.as_ref(),
                &container.path,
                "width",
                limits,
            )?;
            validate_explicit_dimension(
                container.effective_height.as_ref(),
                &container.path,
                "height",
                limits,
            )?;
            let children = parse_resolved_children(container.children, scales, strict, limits)?;
            let spacing = composition_spacing(&container.raw_spec, &container.path, limits)?;
            let width = children
                .iter()
                .map(composition_node_dimensions)
                .map(|(width, _)| width)
                .sum::<f64>()
                + spacing * children.len().saturating_sub(1) as f64;
            let content_height = children
                .iter()
                .map(composition_node_dimensions)
                .map(|(_, height)| height)
                .fold(0.0, f64::max);
            let title = composition_title(&container.raw_spec, &container.path, limits)?;
            let height = content_height
                + title
                    .as_ref()
                    .map(|_| crate::layout::common::TITLE_BAND)
                    .unwrap_or(0.0);
            check_composition_dimension(width, &container.path, "derived width", limits)?;
            check_composition_dimension(height, &container.path, "derived height", limits)?;
            let resolve = node_resolution(scales, &container.path)?;
            Ok(VegaCompositionNode::HConcat(Box::new(VegaConcatNode {
                path: container.path.clone(),
                children,
                width,
                height,
                spacing,
                title,
                background: composition_background(&container.raw_spec, &container.path)?,
                resolve,
            })))
        }
        ExpandedCompositionNode::VConcat(container) => {
            validate_explicit_dimension(
                container.effective_width.as_ref(),
                &container.path,
                "width",
                limits,
            )?;
            validate_explicit_dimension(
                container.effective_height.as_ref(),
                &container.path,
                "height",
                limits,
            )?;
            let children = parse_resolved_children(container.children, scales, strict, limits)?;
            let spacing = composition_spacing(&container.raw_spec, &container.path, limits)?;
            let width = children
                .iter()
                .map(composition_node_dimensions)
                .map(|(width, _)| width)
                .fold(0.0, f64::max);
            let content_height = children
                .iter()
                .map(composition_node_dimensions)
                .map(|(_, height)| height)
                .sum::<f64>()
                + spacing * children.len().saturating_sub(1) as f64;
            let title = composition_title(&container.raw_spec, &container.path, limits)?;
            let height = content_height
                + title
                    .as_ref()
                    .map(|_| crate::layout::common::TITLE_BAND)
                    .unwrap_or(0.0);
            check_composition_dimension(width, &container.path, "derived width", limits)?;
            check_composition_dimension(height, &container.path, "derived height", limits)?;
            let resolve = node_resolution(scales, &container.path)?;
            Ok(VegaCompositionNode::VConcat(Box::new(VegaConcatNode {
                path: container.path.clone(),
                children,
                width,
                height,
                spacing,
                title,
                background: composition_background(&container.raw_spec, &container.path)?,
                resolve,
            })))
        }
    }
}

fn validate_layer_children(
    children: &[crate::ir::VegaCompositionNode],
    path: &str,
) -> Result<(), String> {
    fn check_node(node: &crate::ir::VegaCompositionNode, path: &str) -> Result<(), String> {
        match node {
            crate::ir::VegaCompositionNode::Unit(leaf) => {
                if matches!(
                    &leaf.spec.kind,
                    crate::ir::ChartKind::Bar { .. }
                        | crate::ir::ChartKind::Line { .. }
                        | crate::ir::ChartKind::Trail
                        | crate::ir::ChartKind::Scatter
                        | crate::ir::ChartKind::Bubble
                        | crate::ir::ChartKind::Square
                        | crate::ir::ChartKind::VegaRect { .. }
                        | crate::ir::ChartKind::VegaImage(_)
                        | crate::ir::ChartKind::ErrorMark(_)
                        | crate::ir::ChartKind::VegaBoxPlot(_)
                ) {
                    Ok(())
                } else {
                    Err(format!(
                        "{path} uses a mark that cannot share a Cartesian layer frame"
                    ))
                }
            }
            crate::ir::VegaCompositionNode::Layer(layer) => {
                validate_layer_children(&layer.children, &layer.path)
            }
            crate::ir::VegaCompositionNode::HConcat(_)
            | crate::ir::VegaCompositionNode::VConcat(_) => Err(format!(
                "{path} concat node cannot be nested inside a layer"
            )),
        }
    }

    for (index, child) in children.iter().enumerate() {
        let child_path = if path.is_empty() {
            format!("layer[{index}]")
        } else {
            format!("{path}.layer[{index}]")
        };
        check_node(child, &child_path)?;
    }
    Ok(())
}

fn parse_resolved_children(
    children: Vec<ExpandedCompositionNode>,
    scales: &ResolvedScaleInput,
    strict: bool,
    limits: &crate::guard::InputLimits,
) -> Result<Vec<crate::ir::VegaCompositionNode>, String> {
    children
        .into_iter()
        .map(|child| parse_resolved_node(child, scales, strict, limits))
        .collect()
}

fn composition_node_dimensions(node: &crate::ir::VegaCompositionNode) -> (f64, f64) {
    match node {
        crate::ir::VegaCompositionNode::Unit(leaf) => (leaf.spec.width, leaf.spec.height),
        crate::ir::VegaCompositionNode::Layer(layer) => (layer.width, layer.height),
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => (concat.width, concat.height),
    }
}

fn node_resolution(
    scales: &ResolvedScaleInput,
    path: &str,
) -> Result<crate::ir::VegaCompositionResolve, String> {
    scales.node_resolve.get(path).copied().ok_or_else(|| {
        format!(
            "{}resolved composition settings are missing",
            path_or_root(path)
        )
    })
}

fn composition_title(
    spec: &Value,
    path: &str,
    limits: &crate::guard::InputLimits,
) -> Result<Option<String>, String> {
    let title = match spec.get("title") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok((!text.is_empty()).then(|| text.clone())),
        Some(Value::Object(object)) => match object.get("text") {
            Some(Value::String(text)) => Ok((!text.is_empty()).then(|| text.clone())),
            _ => Err(format!(
                "{}.title.text must be a string",
                path_or_root(path)
            )),
        },
        Some(_) => Err(format!(
            "{}.title must be a string or object",
            path_or_root(path)
        )),
    }?;
    if title
        .as_ref()
        .is_some_and(|title| title.len() > limits.max_label_bytes)
    {
        return Err(format!(
            "{}.title exceeds max_label_bytes ({})",
            path_or_root(path),
            limits.max_label_bytes
        ));
    }
    Ok(title)
}

fn composition_background(spec: &Value, path: &str) -> Result<Option<crate::ir::Color>, String> {
    match spec.get("background") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(color)) => crate::color::parse_color(color)
            .map(Some)
            .ok_or_else(|| format!("{}.background must be a valid color", path_or_root(path))),
        Some(_) => Err(format!(
            "{}.background must be a color string",
            path_or_root(path)
        )),
    }
}

fn composition_spacing(
    spec: &Value,
    path: &str,
    limits: &crate::guard::InputLimits,
) -> Result<f64, String> {
    let spacing = match spec.get("spacing") {
        None | Some(Value::Null) => 20.0,
        Some(Value::Number(number)) => number
            .as_f64()
            .ok_or_else(|| format!("{}.spacing must be finite", path_or_root(path)))?,
        Some(_) => return Err(format!("{}.spacing must be a number", path_or_root(path))),
    };
    if !spacing.is_finite() || spacing < 0.0 || spacing > limits.max_dimension_px {
        return Err(format!(
            "{}.spacing must be finite, non-negative, and at most {} px",
            path_or_root(path),
            limits.max_dimension_px
        ));
    }
    Ok(spacing)
}

fn validate_parsed_dimensions(
    spec: &crate::ir::ChartSpec,
    path: &str,
    limits: &crate::guard::InputLimits,
) -> Result<(), String> {
    check_composition_dimension(spec.width, path, "width", limits)?;
    check_composition_dimension(spec.height, path, "height", limits)
}

fn validate_explicit_dimension(
    value: Option<&Value>,
    path: &str,
    label: &str,
    limits: &crate::guard::InputLimits,
) -> Result<(), String> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(()),
        Some(Value::Number(number)) => {
            let value = number.as_f64().ok_or_else(|| {
                format!("{}.{} must be a finite number", path_or_root(path), label)
            })?;
            check_composition_dimension(value, path, label, limits)
        }
        Some(_) => Err(format!(
            "{}.{} must be a finite number",
            path_or_root(path),
            label
        )),
    }
}

fn check_composition_dimension(
    value: f64,
    path: &str,
    label: &str,
    limits: &crate::guard::InputLimits,
) -> Result<(), String> {
    if !value.is_finite() || value < limits.min_dimension_px || value > limits.max_dimension_px {
        return Err(format!(
            "{}.{} must be between {} and {} px",
            path_or_root(path),
            label,
            limits.min_dimension_px,
            limits.max_dimension_px
        ));
    }
    Ok(())
}

fn parsed_leaf_domains(spec: &crate::ir::ChartSpec) -> crate::ir::VegaLeafScaleDomains {
    use crate::ir::{ChartKind, ErrorMarkOrient, VegaBoxPlotOrient, XPositions};
    let mut domains = crate::ir::VegaLeafScaleDomains::default();
    if let XPositions::Temporal { unix_millis } = &spec.x_positions {
        domains.x = temporal_domain(unix_millis.iter().copied());
    }
    match &spec.kind {
        ChartKind::VegaRect {
            x_labels, y_labels, ..
        } => {
            domains.x = category_domain(x_labels.clone());
            domains.y = category_domain(y_labels.clone());
        }
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square => {
            domains.x = numeric_domain(
                spec.series
                    .iter()
                    .flat_map(|series| series.points.iter().map(|point| point.x)),
            );
            domains.y = numeric_domain(
                spec.series
                    .iter()
                    .flat_map(|series| series.points.iter().map(|point| point.y)),
            );
        }
        ChartKind::VegaBoxPlot(data) => {
            let values = data.groups.iter().flat_map(|group| {
                std::iter::once(group.summary.data_min)
                    .chain(std::iter::once(group.summary.data_max))
                    .chain(group.summary.outliers.iter().copied())
            });
            if data.orient == VegaBoxPlotOrient::Vertical {
                domains.x = category_domain(data.categories.clone());
                domains.y = numeric_domain(values);
            } else {
                domains.x = numeric_domain(values);
                domains.y = category_domain(data.categories.clone());
            }
        }
        ChartKind::ErrorMark(data) => {
            let position_domain = error_position_domain(
                data.ranges.iter().map(|range| range.position),
                &spec.categories,
            );
            let measure_domain = numeric_domain(
                data.ranges
                    .iter()
                    .flat_map(|range| [range.lower, range.center, range.upper]),
            );
            if data.orient == ErrorMarkOrient::Vertical {
                domains.x = position_domain;
                domains.y = measure_domain;
            } else {
                domains.x = measure_domain;
                domains.y = position_domain;
            }
        }
        ChartKind::Bar { value_stacked, .. } => {
            domains.x = category_domain(spec.categories.clone());
            domains.y =
                series_value_domain(&spec.series, *value_stacked, spec.y_axis.begin_at_zero);
        }
        ChartKind::Line { stacked, .. } => {
            if domains.x.is_none() {
                domains.x = category_domain(spec.categories.clone());
            }
            domains.y = series_value_domain(&spec.series, *stacked, spec.y_axis.begin_at_zero);
        }
        ChartKind::Trail => {
            if domains.x.is_none() {
                domains.x = category_domain(spec.categories.clone());
            }
            domains.y = series_value_domain(&spec.series, false, spec.y_axis.begin_at_zero);
        }
        _ => {}
    }
    domains
}

fn category_domain(values: Vec<String>) -> Option<crate::ir::VegaScaleDomain> {
    (!values.is_empty()).then_some(crate::ir::VegaScaleDomain::Categories(values))
}

fn numeric_domain(values: impl IntoIterator<Item = f64>) -> Option<crate::ir::VegaScaleDomain> {
    let mut values = values.into_iter().filter(|value| value.is_finite());
    let first = values.next()?;
    let (min, max) = values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    });
    Some(crate::ir::VegaScaleDomain::Numeric { min, max })
}

fn temporal_domain(values: impl IntoIterator<Item = i64>) -> Option<crate::ir::VegaScaleDomain> {
    let mut values = values.into_iter();
    let first = values.next()?;
    let (min_millis, max_millis) = values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    });
    Some(crate::ir::VegaScaleDomain::Temporal {
        min_millis,
        max_millis,
    })
}

fn error_position_domain(
    positions: impl IntoIterator<Item = crate::ir::ErrorPosition>,
    categories: &[String],
) -> Option<crate::ir::VegaScaleDomain> {
    use crate::ir::ErrorPosition;
    let positions = positions.into_iter().collect::<Vec<_>>();
    if positions
        .iter()
        .any(|position| matches!(position, ErrorPosition::Temporal(_)))
    {
        return temporal_domain(positions.iter().filter_map(|position| match position {
            ErrorPosition::Temporal(value) => Some(*value),
            _ => None,
        }));
    }
    if positions
        .iter()
        .any(|position| matches!(position, ErrorPosition::Quantitative(_)))
    {
        return numeric_domain(positions.iter().filter_map(|position| match position {
            ErrorPosition::Quantitative(value) => Some(*value),
            _ => None,
        }));
    }
    let category_count = positions
        .iter()
        .filter_map(|position| match position {
            ErrorPosition::Category(index) => Some(*index),
            _ => None,
        })
        .max()
        .map_or(0, |index| index + 1);
    category_domain(categories.iter().take(category_count).cloned().collect())
}

fn series_value_domain(
    series: &[crate::ir::Series],
    stacked: bool,
    include_zero: bool,
) -> Option<crate::ir::VegaScaleDomain> {
    let domain = if stacked {
        let point_count = series
            .iter()
            .map(|series| series.values.len())
            .max()
            .unwrap_or(0);
        let mut values = Vec::with_capacity(point_count.saturating_mul(2));
        for index in 0..point_count {
            let mut positive = 0.0;
            let mut negative = 0.0;
            for item in series {
                let value = item.values.get(index).copied().unwrap_or(0.0);
                if value >= 0.0 {
                    positive += value;
                } else {
                    negative += value;
                }
            }
            values.extend([positive, negative]);
        }
        numeric_domain(values)
    } else {
        numeric_domain(
            series
                .iter()
                .flat_map(|series| series.values.iter().copied()),
        )
    };
    match (domain, include_zero) {
        (Some(crate::ir::VegaScaleDomain::Numeric { min, max }), true) => {
            Some(crate::ir::VegaScaleDomain::Numeric {
                min: min.min(0.0),
                max: max.max(0.0),
            })
        }
        (domain, _) => domain,
    }
}

fn finalize_position_domains(node: &mut crate::ir::VegaCompositionNode) -> Result<(), String> {
    match node {
        crate::ir::VegaCompositionNode::Unit(_) => Ok(()),
        crate::ir::VegaCompositionNode::Layer(layer) => {
            for child in &mut layer.children {
                finalize_position_domains(child)?;
            }
            apply_shared_position_domains(&mut layer.children, &layer.resolve, &layer.path)
        }
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => {
            for child in &mut concat.children {
                finalize_position_domains(child)?;
            }
            apply_shared_position_domains(&mut concat.children, &concat.resolve, &concat.path)
        }
    }
}

fn apply_shared_position_domains(
    children: &mut [crate::ir::VegaCompositionNode],
    resolve: &crate::ir::VegaCompositionResolve,
    path: &str,
) -> Result<(), String> {
    for channel in ["x", "y"] {
        if scale_mode(resolve, channel) != crate::ir::VegaResolutionMode::Shared {
            continue;
        }
        let mut domain = None;
        for child in children.iter() {
            if let Some(child_domain) = ir_node_domain(child, channel)? {
                domain = Some(union_scale_domains(
                    domain,
                    child_domain,
                    &format!("{}.resolve.scale.{channel}", path_or_root(path)),
                )?);
            }
        }
        if let Some(domain) = domain {
            for child in children.iter_mut() {
                apply_ir_domain(child, channel, &domain);
            }
        }
    }
    Ok(())
}

fn ir_node_domain(
    node: &crate::ir::VegaCompositionNode,
    channel: &str,
) -> Result<Option<crate::ir::VegaScaleDomain>, String> {
    match node {
        crate::ir::VegaCompositionNode::Unit(leaf) => Ok(match channel {
            "x" => leaf.scales.x.clone(),
            "y" => leaf.scales.y.clone(),
            _ => None,
        }),
        crate::ir::VegaCompositionNode::Layer(layer) => {
            ir_composition_domain(&layer.children, &layer.resolve, &layer.path, channel)
        }
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => {
            ir_composition_domain(&concat.children, &concat.resolve, &concat.path, channel)
        }
    }
}

fn ir_composition_domain(
    children: &[crate::ir::VegaCompositionNode],
    resolve: &crate::ir::VegaCompositionResolve,
    path: &str,
    channel: &str,
) -> Result<Option<crate::ir::VegaScaleDomain>, String> {
    if scale_mode(resolve, channel) == crate::ir::VegaResolutionMode::Independent {
        return Ok(None);
    }
    let mut domain = None;
    for child in children {
        if let Some(child_domain) = ir_node_domain(child, channel)? {
            domain = Some(union_scale_domains(
                domain,
                child_domain,
                &format!("{}.resolve.scale.{channel}", path_or_root(path)),
            )?);
        }
    }
    Ok(domain)
}

fn apply_ir_domain(
    node: &mut crate::ir::VegaCompositionNode,
    channel: &str,
    domain: &crate::ir::VegaScaleDomain,
) {
    match node {
        crate::ir::VegaCompositionNode::Unit(leaf) => match channel {
            "x" => leaf.scales.x = Some(domain.clone()),
            "y" => leaf.scales.y = Some(domain.clone()),
            _ => {}
        },
        crate::ir::VegaCompositionNode::Layer(layer) => {
            if scale_mode(&layer.resolve, channel) == crate::ir::VegaResolutionMode::Shared {
                for child in &mut layer.children {
                    apply_ir_domain(child, channel, domain);
                }
            }
        }
        crate::ir::VegaCompositionNode::HConcat(concat)
        | crate::ir::VegaCompositionNode::VConcat(concat) => {
            if scale_mode(&concat.resolve, channel) == crate::ir::VegaResolutionMode::Shared {
                for child in &mut concat.children {
                    apply_ir_domain(child, channel, domain);
                }
            }
        }
    }
}

fn prefix_path(path: &str, error: String) -> String {
    if path.is_empty() {
        error
    } else if let Some(key) = error.strip_prefix("unknown key: ") {
        format!("unknown key: {path}.{key}")
    } else {
        format!("{path}: {error}")
    }
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
            if !effective_object.contains_key(key)
                && let Some(value) = inherited_value
            {
                effective_object.insert(key.into(), value.clone());
            }
        }
        if strict
            && !matches!(
                effective_object
                    .get("mark")
                    .and_then(Value::as_str)
                    .or_else(|| effective_object
                        .get("mark")
                        .and_then(Value::as_object)
                        .and_then(|mark| mark.get("type"))?
                        .as_str()),
                Some("boxplot" | "image")
            )
        {
            super::vegalite::check_unknown_value(&effective)
                .map_err(|error| prefix_path(path, error))?;
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
    let mut height = object
        .get("height")
        .cloned()
        .or_else(|| inherited.height.clone());
    if composition_title_has_text(object.get("title")) {
        let outer_height = height.as_ref().and_then(Value::as_f64).unwrap_or(450.0);
        height = Some(Value::from(
            outer_height - crate::layout::common::TITLE_BAND,
        ));
    }
    Ok(VegaInheritedSpec {
        data,
        encoding,
        width: object
            .get("width")
            .cloned()
            .or_else(|| inherited.width.clone()),
        height,
        resolve: object
            .get("resolve")
            .cloned()
            .or_else(|| inherited.resolve.clone()),
    })
}

fn composition_title_has_text(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty()),
        _ => false,
    }
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
    let Some(values) = data
        .and_then(Value::as_object)
        .and_then(|object| object.get("values"))
    else {
        return 0;
    };
    if let Some(array) = values.as_array() {
        return array.len();
    }
    if values
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "FeatureCollection")
    {
        return values
            .get("features")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
    }
    usize::from(values.is_object())
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

    #[test]
    fn composition_shared_scale_unions_categories_in_first_seen_order() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": "a", "y": 1, "group": "z"}, {"x": "b", "y": 2, "group": "a"}]}, "mark": "line", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}, "color": {"field": "group", "type": "nominal"}}},
                {"data": {"values": [{"x": "c", "y": 3, "group": "m"}, {"x": "d", "y": 4, "group": "z"}]}, "mark": "line", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}, "color": {"field": "group", "type": "nominal"}}}
            ]
        });
        let expanded = expand(value, false).expect("valid layer");
        let scales = resolve_raw_color_size_scales(&expanded).expect("shared domains resolve");
        let expected =
            crate::ir::VegaScaleDomain::Categories(vec!["z".into(), "a".into(), "m".into()]);
        assert_eq!(scales.leaf_scales["layer[0]"].color, Some(expected.clone()));
        assert_eq!(scales.leaf_scales["layer[1]"].color, Some(expected));
    }

    #[test]
    fn composition_shared_color_and_size_use_union_domains() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": 1, "y": 2, "size": 2, "color": "red"}]}, "mark": "point", "encoding": {"x": {"field": "x", "type": "quantitative"}, "y": {"field": "y", "type": "quantitative"}, "size": {"field": "size", "type": "quantitative"}, "color": {"field": "color", "type": "nominal"}}},
                {"data": {"values": [{"x": 3, "y": 4, "size": 9, "color": "blue"}]}, "mark": "point", "encoding": {"x": {"field": "x", "type": "quantitative"}, "y": {"field": "y", "type": "quantitative"}, "size": {"field": "size", "type": "quantitative"}, "color": {"field": "color", "type": "nominal"}}}
            ]
        });
        let expanded = expand(value, false).expect("valid layer");
        let scales = resolve_raw_color_size_scales(&expanded).expect("shared domains resolve");
        for leaf in ["layer[0]", "layer[1]"] {
            assert_eq!(
                scales.leaf_scales[leaf].size,
                Some(crate::ir::VegaScaleDomain::Numeric { min: 2.0, max: 9.0 })
            );
        }
        assert_eq!(
            scales.leaf_scales["layer[0]"].color,
            Some(crate::ir::VegaScaleDomain::Categories(vec![
                "red".into(),
                "blue".into()
            ]))
        );
        assert_eq!(
            scales.leaf_scales["layer[1]"].color,
            Some(crate::ir::VegaScaleDomain::Categories(vec![
                "red".into(),
                "blue".into()
            ]))
        );
    }

    #[test]
    fn composition_shared_quantitative_color_unions_numeric_extents() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": "a", "y": "row", "value": 1.5}]}, "mark": "rect", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "nominal"}, "color": {"field": "value", "type": "quantitative"}}},
                {"data": {"values": [{"x": "b", "y": "row", "value": 17.0}]}, "mark": "rect", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "nominal"}, "color": {"field": "value", "type": "quantitative"}}}
            ]
        });
        let expanded = expand(value, false).expect("valid layer");
        let scales = resolve_raw_color_size_scales(&expanded).expect("shared domains resolve");
        let expected = crate::ir::VegaScaleDomain::Numeric {
            min: 1.5,
            max: 17.0,
        };
        assert_eq!(scales.leaf_scales["layer[0]"].color, Some(expected.clone()));
        assert_eq!(scales.leaf_scales["layer[1]"].color, Some(expected));
    }

    #[test]
    fn composition_shared_scale_rejects_incompatible_channel_types() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": "a", "y": "row", "color": "red"}]}, "mark": "rect", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "nominal"}, "color": {"field": "color", "type": "nominal"}}},
                {"data": {"values": [{"x": "b", "y": "row", "color": 1}]}, "mark": "rect", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "nominal"}, "color": {"field": "color", "type": "quantitative"}}}
            ]
        });
        let expanded = expand(value, false).expect("shapes expand before scale validation");
        let error = resolve_raw_color_size_scales(&expanded)
            .expect_err("incompatible scale types must fail");
        assert!(error.contains("root.resolve.scale.color"), "got {error:?}");
    }

    #[test]
    fn composition_independent_scales_keep_per_child_domains() {
        let value = json!({
            "resolve": {"scale": {"color": "independent"}},
            "layer": [
                {"data": {"values": [{"x": "a", "y": 1, "group": "left"}]}, "mark": "line", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}, "color": {"field": "group"}}},
                {"data": {"values": [{"x": "b", "y": 2, "group": "right"}]}, "mark": "line", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}, "color": {"field": "group"}}}
            ]
        });
        let expanded = expand(value, false).expect("valid layer");
        let scales = resolve_raw_color_size_scales(&expanded).expect("independent domains resolve");
        assert_eq!(
            scales.leaf_scales["layer[0]"].color,
            Some(crate::ir::VegaScaleDomain::Categories(vec!["left".into()]))
        );
        assert_eq!(
            scales.leaf_scales["layer[1]"].color,
            Some(crate::ir::VegaScaleDomain::Categories(vec!["right".into()]))
        );
    }

    #[test]
    fn composition_resolve_inherits_per_channel() {
        let value = json!({
            "resolve": {"scale": {"color": "independent"}},
            "layer": [
                {"layer": [{"mark": "line"}], "resolve": {"scale": {"size": "independent"}}},
                {"mark": "line"}
            ]
        });
        let expanded = expand(value, false).expect("valid nested layer");
        let scales =
            resolve_raw_color_size_scales(&expanded).expect("resolution inheritance resolves");
        let root = scales.node_resolve[""];
        let nested = scales.node_resolve["layer[0]"];
        assert_eq!(root.color_scale, crate::ir::VegaResolutionMode::Independent);
        assert_eq!(
            nested.color_scale,
            crate::ir::VegaResolutionMode::Independent
        );
        assert_eq!(
            nested.size_scale,
            crate::ir::VegaResolutionMode::Independent
        );
        assert_eq!(nested.x_scale, crate::ir::VegaResolutionMode::Shared);
    }

    #[test]
    fn composition_resolve_rejects_unsupported_guides_and_channels() {
        let cases = [
            (
                json!({"hconcat": [{"mark": "bar"}], "resolve": {"axis": {"x": "shared"}}}),
                "resolve.axis shared is not supported for concat",
            ),
            (
                json!({"layer": [{"mark": "bar"}], "resolve": {"scale": {"shape": "shared"}}}),
                "resolve.scale.shape is not supported",
            ),
            (
                json!({"layer": [{"mark": "bar"}], "resolve": {"scale": {"x": "independent"}, "axis": {"x": "shared"}}}),
                "cannot be shared with an independent scale",
            ),
        ];
        for (value, expected) in cases {
            let expanded = expand(value, false).expect("node shape is valid");
            let error = resolve_raw_color_size_scales(&expanded)
                .expect_err("unsupported resolution should fail");
            assert!(
                error.contains(expected),
                "expected {expected:?}, got {error:?}"
            );
        }
    }

    #[test]
    fn strict_nested_unknown_key_error_contains_leaf_path() {
        let value = json!({
            "layer": [{"mark": "bar", "data": {"values": [{"x": "a", "y": 1}]}, "encoding": {"x": {"field": "x"}, "y": {"field": "y"}}, "typo": true}]
        });
        let error = expand(value, true).expect_err("strict parser rejects unknown leaf key");
        assert!(error.contains("layer[0].typo"), "got {error:?}");
    }

    #[test]
    fn composition_leaf_parser_applies_shared_color_and_size_domains() {
        let mut value = json!({
            "data": {"values": [
                {"x": 1, "y": 2, "group": "blue", "size": 10},
                {"x": 2, "y": 3, "group": "red", "size": 10}
            ]},
            "mark": "point",
            "encoding": {
                "x": {"field": "x", "type": "quantitative"},
                "y": {"field": "y", "type": "quantitative"},
                "color": {"field": "group", "type": "nominal"},
                "size": {"field": "size", "type": "quantitative"}
            }
        });
        let overrides = VegaUnitScaleOverrides {
            color_categories: Some(vec!["red".into(), "blue".into()]),
            color_numeric_domain: None,
            size_numeric_domain: Some((0.0, 10.0)),
        };
        let parsed = crate::frontend::vegalite::parse_unit_value_with_overrides(
            &mut value,
            false,
            &crate::guard::InputLimits::default(),
            &overrides,
        )
        .expect("unit parser accepts resolved scale domains");
        let expected_red = crate::palette::VEGALITE_PALETTE[0];
        let expected_blue = crate::palette::VEGALITE_PALETTE[1];
        assert_eq!(parsed.series[0].name, "blue");
        assert_eq!(parsed.series[0].fill[0], expected_blue);
        assert_eq!(parsed.series[1].name, "red");
        assert_eq!(parsed.series[1].fill[0], expected_red);
        let expected_radius = (361.0 / std::f64::consts::PI).sqrt();
        assert!((parsed.series[0].points[0].r.unwrap() - expected_radius).abs() < 1e-12);
    }

    #[test]
    fn composition_leaf_rect_uses_shared_quantitative_color_domain() {
        let mut value = json!({
            "data": {"values": [{"x": "a", "y": "row", "value": 10.0}]},
            "mark": "rect",
            "encoding": {
                "x": {"field": "x", "type": "nominal"},
                "y": {"field": "y", "type": "nominal"},
                "color": {"field": "value", "type": "quantitative"}
            }
        });
        let overrides = VegaUnitScaleOverrides {
            color_categories: None,
            color_numeric_domain: Some((0.0, 20.0)),
            size_numeric_domain: None,
        };
        let parsed = crate::frontend::vegalite::parse_unit_value_with_overrides(
            &mut value,
            false,
            &crate::guard::InputLimits::default(),
            &overrides,
        )
        .expect("resolved rect scale is accepted");
        let crate::ir::ChartKind::VegaRect { cells, .. } = parsed.kind else {
            panic!("rect mark should parse as VegaRect")
        };
        assert_eq!(
            cells[0][0],
            Some(crate::ir::Color {
                r: 166,
                g: 188,
                b: 212,
                a: 1.0,
            })
        );
    }

    #[test]
    fn composition_parser_builds_resolved_layer_leaf_domains() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": "a", "y": 1, "group": "red"}]}, "mark": "line", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "quantitative"}, "color": {"field": "group", "type": "nominal"}}},
                {"data": {"values": [{"x": "b", "y": 5, "group": "blue"}]}, "mark": "line", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "y", "type": "quantitative"}, "color": {"field": "group", "type": "nominal"}}}
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::Layer(layer) = parsed else {
            panic!("layer root is preserved in composition IR")
        };
        let expected_color =
            crate::ir::VegaScaleDomain::Categories(vec!["red".into(), "blue".into()]);
        let expected_x = crate::ir::VegaScaleDomain::Categories(vec!["a".into(), "b".into()]);
        for child in &layer.children {
            let crate::ir::VegaCompositionNode::Unit(leaf) = child else {
                panic!("layer children are parsed units")
            };
            assert_eq!(leaf.scales.color, Some(expected_color.clone()));
            assert_eq!(leaf.scales.x, Some(expected_x.clone()));
            assert_eq!(
                leaf.scales.y,
                Some(crate::ir::VegaScaleDomain::Numeric { min: 0.0, max: 5.0 })
            );
        }
    }

    #[test]
    fn composition_parser_unions_temporal_position_domains() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"date": "2025-01-01T00:00:00Z", "value": 1}]}, "mark": "line", "encoding": {"x": {"field": "date", "type": "temporal"}, "y": {"field": "value", "type": "quantitative"}}},
                {"data": {"values": [{"date": "2025-01-04T00:00:00Z", "value": 3}]}, "mark": "line", "encoding": {"x": {"field": "date", "type": "temporal"}, "y": {"field": "value", "type": "quantitative"}}}
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::Layer(layer) = parsed else {
            panic!("layer root is preserved in composition IR")
        };
        let expected = crate::ir::VegaScaleDomain::Temporal {
            min_millis: 1_735_689_600_000,
            max_millis: 1_735_948_800_000,
        };
        for child in &layer.children {
            let crate::ir::VegaCompositionNode::Unit(leaf) = child else {
                panic!("layer children are parsed units")
            };
            assert_eq!(leaf.scales.x, Some(expected.clone()));
        }
    }

    #[test]
    fn composition_parser_unions_error_mark_measurement_ranges() {
        let value = json!({
            "layer": [
                {"data": {"values": [{"x": "a", "low": 2, "high": 4}]}, "mark": "errorbar", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "low", "type": "quantitative"}, "y2": {"field": "high", "type": "quantitative"}}},
                {"data": {"values": [{"x": "b", "low": 10, "high": 12}]}, "mark": "errorbar", "encoding": {"x": {"field": "x", "type": "nominal"}, "y": {"field": "low", "type": "quantitative"}, "y2": {"field": "high", "type": "quantitative"}}}
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::Layer(layer) = parsed else {
            panic!("layer root is preserved in composition IR")
        };
        for child in &layer.children {
            let crate::ir::VegaCompositionNode::Unit(leaf) = child else {
                panic!("layer children are parsed units")
            };
            assert_eq!(
                leaf.scales.y,
                Some(crate::ir::VegaScaleDomain::Numeric {
                    min: 2.0,
                    max: 12.0
                })
            );
            assert_eq!(
                leaf.scales.x,
                Some(crate::ir::VegaScaleDomain::Categories(vec![
                    "a".into(),
                    "b".into()
                ]))
            );
        }
    }

    #[test]
    fn composition_parser_derives_concat_dimensions_and_spacing() {
        let value = json!({
            "data": {"values": [{"x": "a", "y": 1}]},
            "spacing": 30,
            "hconcat": [
                {"mark": "bar", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}}},
                {"mark": "bar", "encoding": {"x": {"field": "x"}, "y": {"field": "y"}}}
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::HConcat(concat) = parsed else {
            panic!("hconcat root is preserved in composition IR")
        };
        assert_eq!(
            (concat.width, concat.height, concat.spacing),
            (1630.0, 450.0, 30.0)
        );
    }

    #[test]
    fn composition_parser_shares_geoshape_color_categories_across_concat() {
        let geometry = json!({
            "type": "Polygon",
            "coordinates": [[[0, 0], [1, 0], [1, 1], [0, 0]]]
        });
        let value = json!({
            "hconcat": [
                {
                    "data": {"values": {"type": "FeatureCollection", "features": [
                        {"type": "Feature", "properties": {"shade": "blue"}, "geometry": geometry}
                    ]}},
                    "mark": "geoshape",
                    "encoding": {"color": {"field": "shade", "type": "nominal"}}
                },
                {
                    "data": {"values": {"type": "FeatureCollection", "features": [
                        {"type": "Feature", "properties": {"shade": "red"}, "geometry": geometry}
                    ]}},
                    "mark": "geoshape",
                    "encoding": {"color": {"field": "shade", "type": "nominal"}}
                }
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::HConcat(concat) = parsed else {
            panic!("hconcat root is preserved in composition IR")
        };
        for (index, child) in concat.children.iter().enumerate() {
            let crate::ir::VegaCompositionNode::Unit(leaf) = child else {
                panic!("concat children are parsed units")
            };
            let crate::ir::ChartKind::GeoShape { data } = &leaf.spec.kind else {
                panic!("geoshape leaf retains its GeoShape data")
            };
            assert_eq!(
                leaf.scales.color,
                Some(crate::ir::VegaScaleDomain::Categories(vec![
                    "blue".into(),
                    "red".into()
                ]))
            );
            assert_eq!(
                data.features[0].fill,
                Some(crate::palette::VEGALITE_PALETTE[index])
            );
        }
    }

    #[test]
    fn composition_parser_shares_boxplot_color_categories_across_concat() {
        let value = json!({
            "hconcat": [
                {
                    "data": {"values": [{"category": "A", "group": "blue", "value": 1}]},
                    "mark": "boxplot",
                    "encoding": {
                        "x": {"field": "category", "type": "nominal"},
                        "y": {"field": "value", "type": "quantitative"},
                        "color": {"field": "group", "type": "nominal"}
                    }
                },
                {
                    "data": {"values": [{"category": "B", "group": "red", "value": 2}]},
                    "mark": "boxplot",
                    "encoding": {
                        "x": {"field": "category", "type": "nominal"},
                        "y": {"field": "value", "type": "quantitative"},
                        "color": {"field": "group", "type": "nominal"}
                    }
                }
            ]
        });
        let parsed = parse_test_composition(value);
        let crate::ir::VegaCompositionNode::HConcat(concat) = parsed else {
            panic!("hconcat root is preserved in composition IR")
        };
        for (index, child) in concat.children.iter().enumerate() {
            let crate::ir::VegaCompositionNode::Unit(leaf) = child else {
                panic!("concat children are parsed units")
            };
            let crate::ir::ChartKind::VegaBoxPlot(data) = &leaf.spec.kind else {
                panic!("boxplot leaf retains its statistics")
            };
            assert_eq!(
                data.groups[0].color,
                crate::palette::VEGALITE_PALETTE[index]
            );
        }
    }

    fn parse_test_composition(value: Value) -> crate::ir::VegaCompositionNode {
        let limits = crate::guard::InputLimits::default();
        preflight_composition(&value, &limits).expect("input is within guards");
        let expanded = expand(value, false).expect("input expands");
        let scales = resolve_raw_color_size_scales(&expanded).expect("scales resolve");
        parse_resolved_composition(expanded, &scales, false, &limits).expect("units parse")
    }
}
