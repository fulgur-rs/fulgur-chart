//! Parser for Vega-Lite's bounded Cartesian `tick` mark subset.

use super::*;
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

const OPACITY_SCALE_RANGE: (f64, f64) = (0.3, 0.8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionKind {
    Category,
    Quantitative,
    Temporal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CategoryValueType {
    String,
    Number,
    Boolean,
}

#[derive(Default)]
struct CategoryDomain {
    labels: Vec<String>,
    indexes: HashMap<String, usize>,
    value_types: HashMap<String, CategoryValueType>,
}

impl CategoryDomain {
    fn index(
        &mut self,
        value: &Value,
        label: String,
        row: usize,
        channel: &str,
    ) -> Result<usize, String> {
        let value_type = match value {
            Value::String(_) => CategoryValueType::String,
            Value::Number(_) => CategoryValueType::Number,
            Value::Bool(_) => CategoryValueType::Boolean,
            _ => {
                return Err(format!(
                    "tick data.values[{row}].{channel} must be a string, number, or boolean"
                ));
            }
        };
        if self
            .value_types
            .get(&label)
            .is_some_and(|existing| *existing != value_type)
        {
            return Err(format!(
                "tick data.values[{row}].{channel} category {label:?} has conflicting JSON value types"
            ));
        }
        self.value_types.insert(label.clone(), value_type);
        if let Some(index) = self.indexes.get(&label) {
            return Ok(*index);
        }
        let index = self.labels.len();
        self.labels.push(label.clone());
        self.indexes.insert(label, index);
        Ok(index)
    }
}

#[derive(Clone, Debug)]
enum ChannelSource {
    Field(String),
    Value(Value),
}

struct PositionChannel {
    field: String,
    kind: PositionKind,
    title: Option<String>,
}

pub(super) fn parse_tick_spec(
    top: &Map<String, Value>,
    limits: &crate::guard::InputLimits,
    scale_overrides: &super::super::vegalite_composition::VegaUnitScaleOverrides,
) -> Result<ChartSpec, String> {
    check_unknown_keys(top)?;
    preflight_rows(top, limits)?;
    preflight_category_limits(top, limits)?;
    validate_view_config(top)?;

    let mark = top.get("mark").and_then(Value::as_object);
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "tick mark requires an encoding object".to_string())?;
    let records = super::parse_data_values(top.get("data"))?;
    let x_channel = position_channel(encoding, "x", &records)?;
    let y_channel = position_channel(encoding, "y", &records)?;
    if x_channel.is_none() && y_channel.is_none() {
        return Err("tick mark requires encoding.x or encoding.y".into());
    }

    let theme = make_theme(top)?;
    let orient = parse_orient(mark)?;
    let mark_color = mark
        .and_then(|mark| mark.get("color"))
        .filter(|value| !value.is_null())
        .map(|value| parse_color_value(value, "mark.color"))
        .transpose()?
        .unwrap_or_else(|| palette_color_for(&theme.palette, 0));
    let mark_opacity = mark
        .and_then(|mark| mark.get("opacity"))
        .filter(|value| !value.is_null())
        .map(|value| parse_opacity(value, "mark.opacity"))
        .transpose()?
        .unwrap_or(1.0);
    let mark_size = mark
        .and_then(|mark| mark.get("size"))
        .filter(|value| !value.is_null())
        .map(|value| parse_nonnegative(value, "mark.size", limits))
        .transpose()?;
    let band_size = config_number(top, "bandSize", limits)?;
    let thickness = config_number(top, "thickness", limits)?.unwrap_or(1.0);

    let color_source = parse_style_source(encoding, "color")?;
    let size_source = parse_style_source(encoding, "size")?;
    let opacity_source = parse_style_source(encoding, "opacity")?;
    validate_style_source_type(encoding, "color", color_source.as_ref())?;
    validate_style_source_type(encoding, "size", size_source.as_ref())?;
    validate_style_source_type(encoding, "opacity", opacity_source.as_ref())?;

    let color_categories = category_domain_for_style(&records, color_source.as_ref(), "color")?;
    let color_domain = scale_overrides
        .color_categories
        .as_deref()
        .unwrap_or(&color_categories.labels);
    if color_source
        .as_ref()
        .is_some_and(|source| matches!(source, ChannelSource::Field(_)))
        && color_domain.len() > limits.max_series
    {
        return Err(format!(
            "tick mark series count {} exceeds max_series limit {}",
            color_domain.len(),
            limits.max_series
        ));
    }
    let color_indexes = color_domain
        .iter()
        .enumerate()
        .map(|(index, category)| (category.as_str(), index))
        .collect::<HashMap<_, _>>();

    let size_field = field_source(size_source.as_ref());
    let size_domain = scale_overrides
        .size_numeric_domain
        .or_else(|| size_field.and_then(|field| numeric_domain(&records, field)))
        .unwrap_or((0.0, 1.0));
    let size_domain = (size_domain.0.min(0.0), size_domain.1.max(0.0));
    let opacity_field = field_source(opacity_source.as_ref());
    let opacity_domain = scale_overrides
        .opacity_numeric_domain
        .or_else(|| opacity_field.and_then(|field| numeric_domain(&records, field)))
        .unwrap_or((0.0, 1.0));

    let color_constant = channel_value(color_source.as_ref())
        .map(|value| parse_color_value(value, "encoding.color.value"))
        .transpose()?;
    let size_constant = channel_value(size_source.as_ref())
        .map(|value| parse_nonnegative(value, "encoding.size.value", limits))
        .transpose()?;
    let opacity_constant = channel_value(opacity_source.as_ref())
        .map(|value| parse_opacity(value, "encoding.opacity.value"))
        .transpose()?;

    let mut x_categories = CategoryDomain::default();
    let mut y_categories = CategoryDomain::default();
    let mut marks = Vec::with_capacity(records.len());
    for (row, record) in records.iter().enumerate() {
        let x = if let Some(channel) = &x_channel {
            position(record, channel, &mut x_categories, row, "x")?
        } else {
            VegaTickPosition::Center
        };
        let y = if let Some(channel) = &y_channel {
            position(record, channel, &mut y_categories, row, "y")?
        } else {
            VegaTickPosition::Center
        };

        let mut fill = if let Some(color) = color_constant {
            color
        } else if let Some(field) = field_source(color_source.as_ref()) {
            let label = super::field_category(record, Some(field));
            let index = color_indexes.get(label.as_str()).copied().unwrap_or(0);
            palette_color_for(&theme.palette, index)
        } else {
            mark_color
        };
        let opacity = if let Some(opacity) = opacity_constant {
            opacity
        } else if let Some(field) = opacity_field {
            let value = finite_numeric_field(record, field, "opacity", row)?;
            map_domain(value, opacity_domain, OPACITY_SCALE_RANGE)
        } else {
            mark_opacity
        };
        fill.a = (fill.a * opacity as f32).clamp(0.0, 1.0);

        let size = if let Some(value) = size_constant {
            VegaTickSize::Pixels(value)
        } else if let Some(field) = size_field {
            let value = finite_numeric_field(record, field, "size", row)?;
            VegaTickSize::Scaled(map_domain(value, size_domain, (0.0, 1.0)))
        } else if let Some(value) = mark_size {
            VegaTickSize::Pixels(value)
        } else {
            VegaTickSize::Default
        };
        marks.push(VegaTickMark { x, y, size, fill });
    }

    let category_count = x_categories
        .labels
        .len()
        .saturating_add(y_categories.labels.len());
    if category_count > limits.max_categories {
        return Err(format!(
            "tick mark category count {category_count} exceeds max_categories limit {}",
            limits.max_categories
        ));
    }
    for label in x_categories
        .labels
        .iter()
        .chain(&y_categories.labels)
        .chain(&color_categories.labels)
    {
        if label.len() > limits.max_label_bytes {
            return Err(format!(
                "tick mark category label length {} exceeds max_label_bytes limit {}",
                label.len(),
                limits.max_label_bytes
            ));
        }
    }

    let grid = super::temporal_axis_grid(top, theme.grid_color, theme.text_color)?;
    let x_axis = make_axis(x_channel.as_ref(), grid.clone());
    let y_axis = make_axis(y_channel.as_ref(), grid);
    let series = if field_source(color_source.as_ref()).is_some() {
        color_domain
            .iter()
            .enumerate()
            .map(|(index, label)| {
                make_series(label.clone(), palette_color_for(&theme.palette, index))
            })
            .collect()
    } else {
        Vec::new()
    };
    let legend_title = field_source(color_source.as_ref()).cloned();
    let x_positions = temporal_positions(&marks, true, x_channel.as_ref());
    let y_positions = temporal_positions(&marks, false, y_channel.as_ref());
    let categories = if !x_categories.labels.is_empty() {
        x_categories.labels.clone()
    } else {
        y_categories.labels.clone()
    };
    let title = parse_title(top.get("title"));
    let width = dimension(top.get("width"), "width")?.unwrap_or(800.0);
    let height = dimension(top.get("height"), "height")?.unwrap_or(450.0);
    validate_schema(top)?;

    Ok(ChartSpec {
        kind: ChartKind::VegaTick(Box::new(VegaTickData {
            x_categories: x_categories.labels,
            y_categories: y_categories.labels,
            marks,
            orient,
            band_size,
            thickness,
        })),
        series,
        categories,
        x_positions,
        y_positions,
        x_axis,
        y_axis,
        legend: if legend_title.is_some() {
            LegendPos::Top
        } else {
            LegendPos::None
        },
        legend_options: LegendOptions::default(),
        legend_title,
        vega_size_legend: None,
        title,
        chartjs_title: None,
        chartjs_subtitle: None,
        width,
        height,
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

fn check_unknown_keys(top: &Map<String, Value>) -> Result<(), String> {
    super::check_object(
        top,
        &[
            "mark",
            "data",
            "encoding",
            "$schema",
            "width",
            "height",
            "title",
            "background",
            "config",
        ],
        "",
    )?;
    if super::read_mark_name(top) != Some("tick") {
        return Err("tick mark must be \"tick\" or an object with type \"tick\"".into());
    }
    if let Some(mark) = top.get("mark").and_then(Value::as_object) {
        super::check_object(
            mark,
            &["type", "orient", "color", "opacity", "size"],
            "mark",
        )?;
    }
    if let Some(data) = top.get("data").and_then(Value::as_object) {
        if data.contains_key("url") {
            return Err("tick mark does not support URL data; provide inline data.values".into());
        }
        super::check_object(data, &["values"], "data")?;
    }
    if let Some(config) = top.get("config").and_then(Value::as_object) {
        super::check_object(config, &["view", "axis", "tick"], "config")?;
        if let Some(view) = config.get("view").and_then(Value::as_object) {
            super::check_object(view, &["stroke"], "config.view")?;
        }
        if let Some(axis) = config.get("axis").and_then(Value::as_object) {
            super::check_object(axis, &["grid", "gridOpacity"], "config.axis")?;
        }
        if let Some(tick) = config.get("tick").and_then(Value::as_object) {
            super::check_object(tick, &["bandSize", "thickness"], "config.tick")?;
        }
    }
    if top.contains_key("transform") {
        return Err("tick mark does not support transform".into());
    }
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "tick mark requires an encoding object".to_string())?;
    super::check_object(
        encoding,
        &["x", "y", "color", "size", "opacity"],
        "encoding",
    )?;
    for key in ["x", "y"] {
        if let Some(channel) = encoding.get(key).and_then(Value::as_object) {
            super::check_object(
                channel,
                &["field", "type", "title"],
                &format!("encoding.{key}"),
            )?;
        }
    }
    for key in ["color", "size", "opacity"] {
        if let Some(channel) = encoding.get(key).and_then(Value::as_object) {
            super::check_object(
                channel,
                &["field", "type", "value"],
                &format!("encoding.{key}"),
            )?;
        }
    }
    Ok(())
}

fn preflight_rows(
    top: &Map<String, Value>,
    limits: &crate::guard::InputLimits,
) -> Result<(), String> {
    let values = top
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
        .and_then(Value::as_array)
        .ok_or_else(|| "tick mark requires inline data.values".to_string())?;
    for (limit, name) in [
        (limits.max_total_data_points, "max_total_data_points"),
        (
            limits.max_categorical_primitives,
            "max_categorical_primitives",
        ),
    ] {
        if values.len() > limit {
            return Err(format!(
                "tick mark count {} exceeds {name} limit {limit} (pre-allocation)",
                values.len()
            ));
        }
    }
    Ok(())
}

fn preflight_category_limits(
    top: &Map<String, Value>,
    limits: &crate::guard::InputLimits,
) -> Result<(), String> {
    let values = top
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
        .and_then(Value::as_array)
        .ok_or_else(|| "tick mark requires inline data.values".to_string())?;
    let Some(encoding) = top.get("encoding").and_then(Value::as_object) else {
        return Ok(());
    };

    let x_field = preflight_category_position_field(values, encoding, "x");
    let y_field = preflight_category_position_field(values, encoding, "y");
    let color_field = encoding
        .get("color")
        .and_then(Value::as_object)
        .filter(|channel| !channel.contains_key("value"))
        .and_then(|channel| channel.get("field"))
        .and_then(Value::as_str);

    let mut x_categories = HashSet::<Cow<'_, str>>::new();
    let mut y_categories = HashSet::<Cow<'_, str>>::new();
    let mut color_categories = HashSet::<Cow<'_, str>>::new();
    for value in values {
        let Some(record) = value.as_object() else {
            continue;
        };
        if let Some(field) = x_field {
            if let Some(value) = record.get(field).filter(|value| !value.is_null()) {
                if let Some(label) = category_label_for_preflight(value) {
                    check_preflight_label_length(label.as_ref(), limits, "x")?;
                    if x_categories.insert(label)
                        && x_categories.len().saturating_add(y_categories.len())
                            > limits.max_categories
                    {
                        return Err(format!(
                            "tick mark category count exceeds max_categories limit {} (pre-allocation)",
                            limits.max_categories
                        ));
                    }
                }
            }
        }
        if let Some(field) = y_field {
            if let Some(value) = record.get(field).filter(|value| !value.is_null()) {
                if let Some(label) = category_label_for_preflight(value) {
                    check_preflight_label_length(label.as_ref(), limits, "y")?;
                    if y_categories.insert(label)
                        && x_categories.len().saturating_add(y_categories.len())
                            > limits.max_categories
                    {
                        return Err(format!(
                            "tick mark category count exceeds max_categories limit {} (pre-allocation)",
                            limits.max_categories
                        ));
                    }
                }
            }
        }

        if let Some(field) = color_field
            && let Some(value) = record.get(field).filter(|value| !value.is_null())
            && let Some(label) = category_label_for_preflight(value)
        {
            check_preflight_label_length(label.as_ref(), limits, "color")?;
            if color_categories.insert(label) && color_categories.len() > limits.max_series {
                return Err(format!(
                    "tick mark series count exceeds max_series limit {} (pre-allocation)",
                    limits.max_series
                ));
            }
        }
    }
    Ok(())
}

fn preflight_category_position_field<'a>(
    values: &'a [Value],
    encoding: &'a Map<String, Value>,
    channel: &str,
) -> Option<&'a str> {
    let object = encoding.get(channel)?.as_object()?;
    let field = object.get("field")?.as_str()?;
    match object.get("type") {
        Some(Value::String(kind)) => {
            matches!(kind.as_str(), "nominal" | "ordinal").then_some(field)
        }
        Some(Value::Null) | None => values
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|record| record.get(field))
            .filter(|value| !value.is_null())
            .find_map(|value| match value {
                Value::String(_) | Value::Bool(_) => Some(true),
                Value::Number(_) => Some(false),
                _ => None,
            })
            .and_then(|is_category| is_category.then_some(field)),
        Some(_) => None,
    }
}

fn category_label_for_preflight(value: &Value) -> Option<Cow<'_, str>> {
    match value {
        Value::String(label) => Some(Cow::Borrowed(label)),
        Value::Number(number) => Some(Cow::Owned(number.to_string())),
        Value::Bool(value) => Some(Cow::Borrowed(if *value { "true" } else { "false" })),
        _ => None,
    }
}

fn check_preflight_label_length(
    label: &str,
    limits: &crate::guard::InputLimits,
    channel: &str,
) -> Result<(), String> {
    if label.len() > limits.max_label_bytes {
        return Err(format!(
            "tick mark {channel} category label length {} exceeds max_label_bytes limit {} (pre-allocation)",
            label.len(),
            limits.max_label_bytes
        ));
    }
    Ok(())
}

fn validate_schema(top: &Map<String, Value>) -> Result<(), String> {
    let mut validation_top = top
        .iter()
        .filter(|(key, _)| key.as_str() != "data")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<String, Value>>();
    validation_top.insert("data".into(), serde_json::json!({"values": []}));
    serde_json::from_value::<crate::schema::vegalite::VegaLiteSpec>(Value::Object(validation_top))
        .map(|_| ())
        .map_err(|error| format!("invalid tick mark spec: {error}"))
}

fn validate_view_config(top: &Map<String, Value>) -> Result<(), String> {
    if let Some(view) = top
        .get("config")
        .and_then(Value::as_object)
        .and_then(|config| config.get("view"))
        .and_then(Value::as_object)
        && view.get("stroke").is_some_and(|stroke| !stroke.is_null())
    {
        return Err("config.view.stroke only accepts null".into());
    }
    Ok(())
}

fn position_channel(
    encoding: &Map<String, Value>,
    channel: &str,
    records: &[Map<String, Value>],
) -> Result<Option<PositionChannel>, String> {
    let Some(value) = encoding.get(channel).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("encoding.{channel} must be an object"))?;
    let field = object
        .get("field")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("encoding.{channel}.field must be a string"))?;
    let kind = position_kind(records, object.get("type"), field, channel)?;
    let title = match object.get("title") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err(format!("encoding.{channel}.title must be a string")),
    };
    Ok(Some(PositionChannel {
        field: field.to_owned(),
        kind,
        title,
    }))
}

fn position_kind(
    records: &[Map<String, Value>],
    field_type: Option<&Value>,
    field: &str,
    channel: &str,
) -> Result<PositionKind, String> {
    match field_type {
        Some(Value::String(value)) => match value.as_str() {
            "nominal" | "ordinal" => Ok(PositionKind::Category),
            "quantitative" => Ok(PositionKind::Quantitative),
            "temporal" => Ok(PositionKind::Temporal),
            _ => Err(format!(
                "encoding.{channel}.type must be quantitative, temporal, nominal, or ordinal"
            )),
        },
        Some(Value::Null) | None => records
            .iter()
            .filter_map(|record| record.get(field).filter(|value| !value.is_null()))
            .find_map(|value| match value {
                Value::Number(_) => Some(PositionKind::Quantitative),
                Value::String(_) | Value::Bool(_) => Some(PositionKind::Category),
                _ => None,
            })
            .ok_or_else(|| format!("encoding.{channel} position type cannot be inferred")),
        Some(_) => Err(format!("encoding.{channel}.type must be a string")),
    }
}

fn position(
    record: &Map<String, Value>,
    channel: &PositionChannel,
    domain: &mut CategoryDomain,
    row: usize,
    name: &str,
) -> Result<VegaTickPosition, String> {
    let value = record
        .get(&channel.field)
        .filter(|value| !value.is_null())
        .ok_or_else(|| {
            format!(
                "tick data.values[{row}].{} is missing or null",
                channel.field
            )
        })?;
    match channel.kind {
        PositionKind::Category => Ok(VegaTickPosition::Category(domain.index(
            value,
            super::field_category(record, Some(&channel.field)),
            row,
            name,
        )?)),
        PositionKind::Quantitative => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(VegaTickPosition::Quantitative)
            .ok_or_else(|| {
                format!(
                    "tick data.values[{row}].{} must be a finite number",
                    channel.field
                )
            }),
        PositionKind::Temporal => {
            let raw = value.as_str().ok_or_else(|| {
                format!(
                    "tick data.values[{row}].{} must be an RFC 3339 timestamp",
                    channel.field
                )
            })?;
            crate::temporal::parse_rfc3339_millis(&channel.field, raw)
                .map(VegaTickPosition::Temporal)
                .map_err(|error| format!("tick data.values[{row}].{}: {error}", channel.field))
        }
    }
}

fn parse_style_source(
    encoding: &Map<String, Value>,
    channel: &str,
) -> Result<Option<ChannelSource>, String> {
    let Some(value) = encoding.get(channel).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("encoding.{channel} must be an object"))?;
    let field = object.get("field");
    let field = match field {
        Some(Value::String(field)) => Some(field.clone()),
        Some(_) => return Err(format!("encoding.{channel}.field must be a string")),
        None => None,
    };
    let has_value = object.contains_key("value");
    if field.is_some() == has_value {
        return Err(format!(
            "encoding.{channel} must specify exactly one of field or value"
        ));
    }
    if let Some(field_type) = object.get("type").filter(|value| !value.is_null()) {
        if !field_type.is_string() {
            return Err(format!("encoding.{channel}.type must be a string"));
        }
    }
    Ok(field.map_or_else(
        || Some(ChannelSource::Value(object["value"].clone())),
        |field| Some(ChannelSource::Field(field)),
    ))
}

fn validate_style_source_type(
    encoding: &Map<String, Value>,
    channel: &str,
    source: Option<&ChannelSource>,
) -> Result<(), String> {
    let Some(ChannelSource::Field(_)) = source else {
        return Ok(());
    };
    let field_type = super::channel_type(encoding, channel);
    let valid = match channel {
        "color" => field_type.is_none_or(|kind| matches!(kind, "nominal" | "ordinal")),
        "size" | "opacity" => field_type.is_none_or(|kind| kind == "quantitative"),
        _ => false,
    };
    if !valid {
        let expected = if channel == "color" {
            "nominal or ordinal"
        } else {
            "quantitative"
        };
        return Err(format!("encoding.{channel}.type must be {expected}"));
    }
    Ok(())
}

fn category_domain_for_style(
    records: &[Map<String, Value>],
    source: Option<&ChannelSource>,
    channel: &str,
) -> Result<CategoryDomain, String> {
    let Some(ChannelSource::Field(field)) = source else {
        return Ok(CategoryDomain::default());
    };
    let mut domain = CategoryDomain::default();
    for (row, record) in records.iter().enumerate() {
        let value = record
            .get(field)
            .filter(|value| !value.is_null())
            .ok_or_else(|| format!("tick data.values[{row}].{field} for {channel} is missing"))?;
        domain.index(
            value,
            super::field_category(record, Some(field)),
            row,
            channel,
        )?;
    }
    Ok(domain)
}

fn field_source(source: Option<&ChannelSource>) -> Option<&String> {
    match source {
        Some(ChannelSource::Field(field)) => Some(field),
        _ => None,
    }
}

fn channel_value(source: Option<&ChannelSource>) -> Option<&Value> {
    match source {
        Some(ChannelSource::Value(value)) => Some(value),
        _ => None,
    }
}

fn finite_numeric_field(
    record: &Map<String, Value>,
    field: &str,
    channel: &str,
    row: usize,
) -> Result<f64, String> {
    record
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            format!("tick data.values[{row}].{field} for encoding.{channel} must be finite numeric")
        })
}

fn numeric_domain(records: &[Map<String, Value>], field: &str) -> Option<(f64, f64)> {
    let mut values = records
        .iter()
        .filter_map(|record| record.get(field).and_then(Value::as_f64))
        .filter(|value| value.is_finite());
    let first = values.next()?;
    Some(values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

fn map_domain(value: f64, domain: (f64, f64), range: (f64, f64)) -> f64 {
    let (min, max) = domain;
    let fraction = if min == max {
        0.5
    } else {
        let scale = min.abs().max(max.abs()).max(1.0);
        ((value / scale - min / scale) / (max / scale - min / scale)).clamp(0.0, 1.0)
    };
    range.0 + fraction * (range.1 - range.0)
}

fn parse_orient(mark: Option<&Map<String, Value>>) -> Result<VegaTickOrient, String> {
    match mark
        .and_then(|mark| mark.get("orient"))
        .filter(|value| !value.is_null())
    {
        None => Ok(VegaTickOrient::Horizontal),
        Some(Value::String(value)) if value == "horizontal" => Ok(VegaTickOrient::Horizontal),
        Some(Value::String(value)) if value == "vertical" => Ok(VegaTickOrient::Vertical),
        Some(_) => Err("mark.orient must be \"horizontal\" or \"vertical\"".into()),
    }
}

fn config_number(
    top: &Map<String, Value>,
    key: &str,
    limits: &crate::guard::InputLimits,
) -> Result<Option<f64>, String> {
    top.get("config")
        .and_then(Value::as_object)
        .and_then(|config| config.get("tick"))
        .and_then(Value::as_object)
        .and_then(|tick| tick.get(key))
        .filter(|value| !value.is_null())
        .map(|value| parse_nonnegative(value, &format!("config.tick.{key}"), limits))
        .transpose()
}

fn parse_nonnegative(
    value: &Value,
    path: &str,
    limits: &crate::guard::InputLimits,
) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= limits.max_dimension_px)
        .ok_or_else(|| format!("{path} must be a finite number between 0 and max_dimension_px"))
}

fn parse_opacity(value: &Value, path: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| format!("{path} must be between 0 and 1"))
}

fn parse_color_value(value: &Value, path: &str) -> Result<Color, String> {
    value
        .as_str()
        .and_then(parse_color)
        .ok_or_else(|| format!("{path} must be a valid CSS color"))
}

fn palette_color_for(palette: &[Color], index: usize) -> Color {
    palette
        .get(index % palette.len().max(1))
        .copied()
        .unwrap_or(Color {
            r: 76,
            g: 120,
            b: 168,
            a: 1.0,
        })
}

fn make_series(name: String, color: Color) -> Series {
    Series {
        name,
        values: Vec::new(),
        points: Vec::new(),
        fill: vec![color],
        stroke: vec![color],
        stroke_width: 1.0,
        area: false,
        area_fill: None,
        interpolation: LineInterpolation::Linear,
        span_gaps: false,
        step_mode: None,
        line_style: None,
        series_type: SeriesType::Line,
        stack: None,
        bar_geometry: None,
        point_radius: None,
        trail_widths: None,
        violin_samples: Vec::new(),
        box_points: Vec::new(),
        tree: Vec::new(),
        links: Vec::new(),
    }
}

fn make_axis(channel: Option<&PositionChannel>, grid: AxisGrid) -> AxisSpec {
    let title = channel.map(|channel| AxisTitle {
        text: channel
            .title
            .clone()
            .unwrap_or_else(|| channel.field.clone()),
        color: None,
        font_size: None,
        align: AxisTitleAlign::Center,
    });
    let temporal = channel.is_some_and(|channel| channel.kind == PositionKind::Temporal);
    AxisSpec {
        title,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: false,
        grid,
        border: AxisBorder::default(),
        scale_kind: if temporal {
            ScaleKind::Time
        } else {
            ScaleKind::Linear
        },
        time: temporal.then(crate::ir::TimeOptions::default),
        ticks: crate::ir::AxisTickOptions::default(),
    }
}

fn make_theme(top: &Map<String, Value>) -> Result<crate::ir::Theme, String> {
    let mut theme = vegalite_theme();
    if let Some(background) = top.get("background").filter(|value| !value.is_null()) {
        theme.background = Some(parse_color_value(background, "background")?);
    }
    Ok(theme)
}

fn parse_title(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn dimension(value: Option<&Value>, name: &str) -> Result<Option<f64>, String> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(Some)
            .ok_or_else(|| format!("{name} must be a positive finite number")),
    }
}

fn temporal_positions(
    marks: &[VegaTickMark],
    x_axis: bool,
    channel: Option<&PositionChannel>,
) -> XPositions {
    if !channel.is_some_and(|channel| channel.kind == PositionKind::Temporal) {
        return XPositions::Category;
    }
    let mut values = marks
        .iter()
        .filter_map(|mark| match if x_axis { mark.x } else { mark.y } {
            VegaTickPosition::Temporal(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    XPositions::Temporal {
        unix_millis: values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_scaling_handles_equal_and_extreme_domains() {
        assert_eq!(map_domain(4.0, (4.0, 4.0), (0.0, 1.0)), 0.5);
        assert_eq!(
            map_domain(-f64::MAX, (-f64::MAX, f64::MAX), (0.0, 1.0)),
            0.0
        );
        assert_eq!(map_domain(f64::MAX, (-f64::MAX, f64::MAX), (0.0, 1.0)), 1.0);
    }
}
