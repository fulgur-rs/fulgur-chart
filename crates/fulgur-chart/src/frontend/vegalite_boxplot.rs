use crate::frontend::vegalite_error::type7_quantile;
use crate::ir::*;
use crate::palette::{VEGALITE_PALETTE, vegalite_theme};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Clone)]
struct PositionField {
    field: String,
    field_type: Option<String>,
    scale_domain: Option<(f64, f64)>,
}

struct GroupBuilder {
    category_index: Option<usize>,
    color_label: Option<String>,
    detail_label: Option<String>,
    color: Color,
    size: Option<f64>,
    opacity: f64,
    values: Vec<f64>,
}

pub(super) fn parse_boxplot_spec(
    top: &mut Map<String, Value>,
    limits: &crate::guard::InputLimits,
) -> Result<ChartSpec, String> {
    if top.contains_key("transform") {
        return Err("boxplot transform is not supported".into());
    }
    if top.contains_key("layer") {
        return Err("boxplot layer is not supported".into());
    }

    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "boxplot encoding must be an object".to_string())?;
    if encoding.contains_key("x2") || encoding.contains_key("y2") {
        return Err("pre-aggregated boxplot summaries are not supported".into());
    }
    validate_boxplot_schema(top)?;

    let data = top
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "boxplot data must be an inline object".to_string())?;
    if data.contains_key("url") {
        return Err("boxplot data.url is not supported; use data.values".into());
    }
    let values = data
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "boxplot data.values must be an inline record array".to_string())?;
    if values.is_empty() {
        return Err("boxplot data.values must not be empty".into());
    }
    if values.len() > limits.max_total_data_points {
        return Err(format!(
            "boxplot point count {} exceeds max_total_data_points limit {} (pre-aggregation)",
            values.len(),
            limits.max_total_data_points
        ));
    }
    let records = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_object()
                .ok_or_else(|| format!("boxplot data.values[{index}] must be a record object"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let x_position = parse_position(encoding, "x")?;
    let y_position = parse_position(encoding, "y")?;
    if x_position.is_none() && y_position.is_none() {
        return Err("boxplot requires one quantitative field in encoding.x or encoding.y".into());
    }
    let x_quantitative = x_position
        .as_ref()
        .is_some_and(|field| is_quantitative(field, &records));
    let y_quantitative = y_position
        .as_ref()
        .is_some_and(|field| is_quantitative(field, &records));
    if x_quantitative == y_quantitative {
        return Err("boxplot requires exactly one quantitative position field".into());
    }
    let (measurement, orient, category) = if x_quantitative {
        (
            x_position.as_ref().expect("quantitative x field"),
            VegaBoxPlotOrient::Horizontal,
            y_position.as_ref(),
        )
    } else {
        (
            y_position.as_ref().expect("quantitative y field"),
            VegaBoxPlotOrient::Vertical,
            x_position.as_ref(),
        )
    };
    if category.is_some_and(|field| is_quantitative(field, &records)) {
        return Err("boxplot categorical position field must not be quantitative".into());
    }
    validate_measurement_field(measurement, &records)?;
    if let Some(field) = category {
        validate_categorical_field(&field.field, &records)?;
        if field.scale_domain.is_some() {
            return Err("boxplot categorical position does not support scale.domain".into());
        }
    }
    let expected_orient = match orient {
        VegaBoxPlotOrient::Horizontal => "horizontal",
        VegaBoxPlotOrient::Vertical => "vertical",
    };
    if let Some(explicit) = mark_value(top, "orient") {
        let explicit = explicit
            .as_str()
            .ok_or_else(|| "boxplot mark.orient must be a string".to_string())?;
        if explicit != expected_orient {
            return Err(format!(
                "boxplot mark.orient {explicit:?} conflicts with the quantitative measurement axis ({expected_orient})"
            ));
        }
    }
    let hard_domain = measurement.scale_domain;

    let color = parse_color_channel(encoding)?;
    if let Some(field) = color.field.as_deref() {
        validate_categorical_field(field, &records)?;
    }
    let detail_field = parse_categorical_channel(encoding, "detail")?;
    if let Some(field) = detail_field.as_deref() {
        validate_categorical_field(field, &records)?;
    }
    let size = parse_numeric_channel(encoding, "size", false)?;
    let opacity = parse_numeric_channel(encoding, "opacity", true)?;
    let mark_size = parse_mark_number(top, "size", false)?
        .flatten()
        .unwrap_or(14.0);
    let mark_opacity = parse_mark_number(top, "opacity", true)?
        .flatten()
        .unwrap_or(1.0);
    let mark_color = parse_mark_color(top)?.unwrap_or(VEGALITE_PALETTE[0]);
    let extent = parse_extent(top)?;
    let style = VegaBoxPlotStyle {
        clip: parse_mark_bool(top, "clip")?.unwrap_or(false),
        opacity: mark_opacity,
        box_part: parse_part_style(top, "box")?,
        median_part: parse_part_style(top, "median")?,
        outliers_part: parse_part_style(top, "outliers")?,
        rule_part: parse_part_style(top, "rule")?,
        ticks_part: parse_part_style(top, "ticks")?,
    };

    let mut categories = Vec::<String>::new();
    let mut category_indices = HashMap::<String, usize>::new();
    let mut color_indices = HashMap::<String, usize>::new();
    let mut group_indices =
        HashMap::<(Option<usize>, Option<String>, Option<String>), usize>::new();
    let mut groups = Vec::<GroupBuilder>::new();
    for record in &records {
        let category_index = if let Some(field) = category {
            let (key, label) = category_key(required_value(record, &field.field)?, &field.field)?;
            let index = *category_indices.entry(key).or_insert_with(|| {
                let index = categories.len();
                categories.push(label);
                index
            });
            if categories.len() > limits.max_categories {
                return Err(format!(
                    "boxplot category count {} exceeds max_categories limit {} (pre-grouping)",
                    categories.len(),
                    limits.max_categories
                ));
            }
            Some(index)
        } else {
            None
        };

        let (color_key, color_label, group_color) = match (&color.field, color.value) {
            (Some(field), _) => {
                let (key, label) = category_key(required_value(record, field)?, field)?;
                let next_index = color_indices.len();
                let index = *color_indices.entry(key.clone()).or_insert(next_index);
                if color_indices.len() > limits.max_categories {
                    return Err(format!(
                        "boxplot color category count {} exceeds max_categories limit {}",
                        color_indices.len(),
                        limits.max_categories
                    ));
                }
                (
                    Some(key),
                    Some(label),
                    VEGALITE_PALETTE[index % VEGALITE_PALETTE.len()],
                )
            }
            (None, Some(value_color)) => (Some("value".to_string()), None, value_color),
            (None, None) => (None, None, mark_color),
        };
        let (detail_key, detail_label) = if let Some(field) = detail_field.as_deref() {
            let (key, label) = category_key(required_value(record, field)?, field)?;
            (Some(key), Some(label))
        } else {
            (None, None)
        };
        let group_key = (category_index, color_key.clone(), detail_key);
        let size_value = numeric_channel_value(&size, record, "size")?.or(Some(mark_size));
        let opacity_value = numeric_channel_value(&opacity, record, "opacity")?.unwrap_or(1.0);
        let measurement_value = numeric_value(
            required_value(record, &measurement.field)?,
            &measurement.field,
        )?;

        let group_index = if let Some(index) = group_indices.get(&group_key) {
            *index
        } else {
            if groups.len() >= limits.max_series {
                return Err(format!(
                    "boxplot group count exceeds max_series limit {} (pre-allocation)",
                    limits.max_series
                ));
            }
            let index = groups.len();
            group_indices.insert(group_key, index);
            groups.push(GroupBuilder {
                category_index,
                color_label,
                detail_label,
                color: group_color,
                size: size_value,
                opacity: opacity_value,
                values: Vec::new(),
            });
            index
        };
        let group = &mut groups[group_index];
        if group.size != size_value {
            return Err(
                "boxplot size must be constant within each position/color/detail group".into(),
            );
        }
        if group.opacity != opacity_value {
            return Err(
                "boxplot opacity must be constant within each position/color/detail group".into(),
            );
        }
        group.values.push(measurement_value);
    }
    if groups.is_empty() {
        return Err("boxplot data has no groups".into());
    }

    let groups = groups
        .into_iter()
        .map(|group| {
            let point_count = group.values.len();
            let summary = summarize_boxplot(&group.values, extent)?;
            Ok(VegaBoxPlotGroup {
                category_index: group.category_index,
                color_label: group.color_label,
                detail_label: group.detail_label,
                color: group.color,
                size: group.size,
                opacity: group.opacity,
                point_count,
                summary,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let estimated_primitives = groups.iter().fold(0usize, |total, group| {
        let fixed = usize::from(style.box_part.visible)
            .saturating_add(usize::from(style.median_part.visible))
            .saturating_add(
                usize::from(style.box_part.visible && style.box_part.stroke.is_some())
                    .saturating_mul(4),
            );
        let endpoints = group.summary.whisker_low.is_some() && group.summary.whisker_high.is_some();
        let whiskers = if endpoints {
            usize::from(style.rule_part.visible)
                .saturating_add(usize::from(style.ticks_part.visible).saturating_mul(2))
        } else {
            0
        };
        let outliers = if style.outliers_part.visible {
            group.summary.outliers.len()
        } else {
            0
        };
        total
            .saturating_add(fixed)
            .saturating_add(whiskers)
            .saturating_add(outliers)
    });
    if estimated_primitives > limits.max_categorical_primitives {
        return Err(format!(
            "boxplot requires up to {estimated_primitives} primitives, exceeding max_categorical_primitives limit {}",
            limits.max_categorical_primitives
        ));
    }

    let mut theme = vegalite_theme();
    if let Some(background) = top.get("background") {
        theme.background = Some(
            background
                .as_str()
                .and_then(crate::color::parse_color)
                .ok_or_else(|| "boxplot background must be a valid color".to_string())?,
        );
    }
    let (width, height) = parse_dimensions(top);
    let title = parse_title(top);
    let has_category = category.is_some();
    let value_axis_grid = AxisGrid {
        display: true,
        color: Some(theme.grid_color),
        ..AxisGrid::default()
    };
    let category_axis_grid = AxisGrid {
        display: false,
        ..AxisGrid::default()
    };
    let value_axis = make_axis(hard_domain, value_axis_grid);
    let category_axis = make_axis(None, category_axis_grid);
    let (x_axis, y_axis) = if orient == VegaBoxPlotOrient::Vertical {
        (category_axis, value_axis)
    } else {
        (value_axis, category_axis)
    };
    let data = VegaBoxPlotData {
        orient,
        categories: categories.clone(),
        groups,
        has_category,
        extent,
        style,
    };
    Ok(ChartSpec {
        kind: ChartKind::VegaBoxPlot(Box::new(data)),
        series: Vec::new(),
        categories,
        x_positions: XPositions::Category,
        y_positions: XPositions::Category,
        x_axis,
        y_axis,
        legend: if color.field.is_some() {
            LegendPos::Top
        } else {
            LegendPos::None
        },
        legend_options: LegendOptions::default(),
        legend_title: color.field,
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

fn validate_boxplot_schema(top: &Map<String, Value>) -> Result<(), String> {
    let mut validation = top
        .iter()
        .filter(|(key, _)| key.as_str() != "data")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<String, Value>>();
    validation.insert("data".into(), serde_json::json!({"values": []}));
    serde_json::from_value::<crate::schema::vegalite::VegaLiteSpec>(Value::Object(validation))
        .map(|_| ())
        .map_err(|error| format!("invalid boxplot spec: {error}"))
}

fn parse_position(
    encoding: &Map<String, Value>,
    channel: &str,
) -> Result<Option<PositionField>, String> {
    let Some(value) = encoding.get(channel) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("boxplot encoding.{channel} must be an object"))?;
    let field = object
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("boxplot encoding.{channel}.field must be a nonempty string"))?
        .to_string();
    let field_type = object
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let scale_domain = match object.get("scale") {
        None => None,
        Some(Value::Object(scale)) => {
            let Some(domain) = scale.get("domain") else {
                return Err("boxplot scale supports only a domain pair".into());
            };
            let pair = domain
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| {
                    "boxplot scale.domain must contain exactly two numbers".to_string()
                })?;
            let low = pair[0]
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    "boxplot scale.domain endpoints must be finite numbers".to_string()
                })?;
            let high = pair[1]
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    "boxplot scale.domain endpoints must be finite numbers".to_string()
                })?;
            if low >= high {
                return Err("boxplot scale.domain must be in ascending order".into());
            }
            if !(high - low).is_finite() {
                return Err("boxplot scale.domain span must be finite".into());
            }
            Some((low, high))
        }
        Some(_) => {
            return Err(format!(
                "boxplot encoding.{channel}.scale must be an object"
            ));
        }
    };
    Ok(Some(PositionField {
        field,
        field_type,
        scale_domain,
    }))
}

fn is_quantitative(field: &PositionField, records: &[&Map<String, Value>]) -> bool {
    match field.field_type.as_deref() {
        Some("quantitative") => true,
        Some("nominal" | "ordinal") => false,
        _ => records
            .iter()
            .all(|record| record.get(&field.field).is_some_and(Value::is_number)),
    }
}

fn validate_measurement_field(
    field: &PositionField,
    records: &[&Map<String, Value>],
) -> Result<(), String> {
    if field
        .field_type
        .as_deref()
        .is_some_and(|ty| ty != "quantitative")
    {
        return Err(format!(
            "boxplot measurement field {:?} must be quantitative",
            field.field
        ));
    }
    for record in records {
        numeric_value(required_value(record, &field.field)?, &field.field)?;
    }
    Ok(())
}

fn validate_categorical_field(field: &str, records: &[&Map<String, Value>]) -> Result<(), String> {
    for record in records {
        category_key(required_value(record, field)?, field)?;
    }
    Ok(())
}

fn required_value<'a>(record: &'a Map<String, Value>, field: &str) -> Result<&'a Value, String> {
    let value = record
        .get(field)
        .ok_or_else(|| format!("boxplot data field {field:?} is missing"))?;
    if value.is_null() {
        return Err(format!("boxplot data field {field:?} must not be null"));
    }
    Ok(value)
}

fn numeric_value(value: &Value, field: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("boxplot field {field:?} must contain finite numbers"))
}

fn category_key(value: &Value, field: &str) -> Result<(String, String), String> {
    match value {
        Value::String(label) => Ok((format!("s:{}:{label}", label.len()), label.clone())),
        Value::Number(number) => {
            let number = number
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| format!("boxplot category field {field:?} must be finite"))?;
            let canonical = if number == 0.0 { 0.0 } else { number };
            Ok((
                format!("n:{:016x}", canonical.to_bits()),
                number.to_string(),
            ))
        }
        Value::Bool(value) => Ok((format!("b:{value}"), value.to_string())),
        _ => Err(format!(
            "boxplot category field {field:?} must contain string, number, or boolean values"
        )),
    }
}

struct ColorChannel {
    field: Option<String>,
    value: Option<Color>,
}

fn parse_color_channel(encoding: &Map<String, Value>) -> Result<ColorChannel, String> {
    match encoding.get("color") {
        None => Ok(ColorChannel {
            field: None,
            value: None,
        }),
        Some(Value::Object(object)) if object.contains_key("field") => {
            let field = object
                .get("field")
                .and_then(Value::as_str)
                .filter(|field| !field.is_empty())
                .ok_or_else(|| "boxplot encoding.color.field must be nonempty".to_string())?;
            if object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "quantitative")
            {
                return Err("boxplot encoding.color.type must be nominal or ordinal".into());
            }
            Ok(ColorChannel {
                field: Some(field.to_string()),
                value: None,
            })
        }
        Some(Value::Object(object)) if object.contains_key("value") => {
            let color = object
                .get("value")
                .and_then(Value::as_str)
                .and_then(crate::color::parse_color)
                .ok_or_else(|| "boxplot encoding.color.value must be a valid color".to_string())?;
            Ok(ColorChannel {
                field: None,
                value: Some(color),
            })
        }
        Some(_) => Err("boxplot encoding.color must specify field or value".into()),
    }
}

fn parse_categorical_channel(
    encoding: &Map<String, Value>,
    channel: &str,
) -> Result<Option<String>, String> {
    let Some(value) = encoding.get(channel) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("boxplot encoding.{channel} must be an object"))?;
    let field = object
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("boxplot encoding.{channel}.field must be nonempty"))?;
    Ok(Some(field.to_string()))
}

enum NumericChannel {
    Field(String),
    Value(f64),
}

fn parse_numeric_channel(
    encoding: &Map<String, Value>,
    channel: &str,
    unit_interval: bool,
) -> Result<Option<NumericChannel>, String> {
    let Some(value) = encoding.get(channel) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| format!("boxplot encoding.{channel} must be an object"))?;
    let valid = |number: f64| {
        number.is_finite()
            && if unit_interval {
                (0.0..=1.0).contains(&number)
            } else {
                number >= 0.0
            }
    };
    if let Some(field) = object.get("field").and_then(Value::as_str) {
        if object
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind != "quantitative")
        {
            return Err(format!(
                "boxplot encoding.{channel}.type must be quantitative"
            ));
        }
        Ok(Some(NumericChannel::Field(field.to_string())))
    } else if let Some(number) = object
        .get("value")
        .and_then(Value::as_f64)
        .filter(|number| valid(*number))
    {
        Ok(Some(NumericChannel::Value(number)))
    } else {
        Err(format!(
            "boxplot encoding.{channel} must have a valid field or value"
        ))
    }
}

fn numeric_channel_value(
    channel: &Option<NumericChannel>,
    record: &Map<String, Value>,
    name: &str,
) -> Result<Option<f64>, String> {
    match channel {
        None => Ok(None),
        Some(NumericChannel::Value(value)) => Ok(Some(*value)),
        Some(NumericChannel::Field(field)) => {
            let value = numeric_value(required_value(record, field)?, field)?;
            let valid = if name == "opacity" {
                (0.0..=1.0).contains(&value)
            } else {
                value >= 0.0
            };
            if !valid {
                return Err(format!(
                    "boxplot encoding.{name} values are outside the supported range"
                ));
            }
            Ok(Some(value))
        }
    }
}

fn mark_value<'a>(top: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    top.get("mark")?.as_object()?.get(key)
}

fn parse_mark_number(
    top: &Map<String, Value>,
    key: &str,
    unit_interval: bool,
) -> Result<Option<Option<f64>>, String> {
    let Some(value) = mark_value(top, key) else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("boxplot mark.{key} must be finite"))?;
    if (unit_interval && !(0.0..=1.0).contains(&number)) || (!unit_interval && number < 0.0) {
        return Err(format!("boxplot mark.{key} is outside the supported range"));
    }
    Ok(Some(Some(number)))
}

fn parse_mark_color(top: &Map<String, Value>) -> Result<Option<Color>, String> {
    let Some(value) = mark_value(top, "color") else {
        return Ok(None);
    };
    value
        .as_str()
        .and_then(crate::color::parse_color)
        .map(Some)
        .ok_or_else(|| "boxplot mark.color must be a valid color".to_string())
}

fn parse_mark_bool(top: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    let Some(value) = mark_value(top, key) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| format!("boxplot mark.{key} must be a boolean"))
}

fn parse_extent(top: &Map<String, Value>) -> Result<VegaBoxPlotExtent, String> {
    match mark_value(top, "extent") {
        None => Ok(VegaBoxPlotExtent::Tukey { coefficient: 1.5 }),
        Some(Value::String(value)) if value == "min-max" => Ok(VegaBoxPlotExtent::MinMax),
        Some(Value::Number(value)) => {
            let coefficient = value
                .as_f64()
                .filter(|number| number.is_finite() && *number >= 0.0)
                .ok_or_else(|| "boxplot extent must be finite and nonnegative".to_string())?;
            Ok(VegaBoxPlotExtent::Tukey { coefficient })
        }
        Some(_) => Err("boxplot extent must be a nonnegative number or \"min-max\"".into()),
    }
}

fn parse_part_style(top: &Map<String, Value>, key: &str) -> Result<VegaBoxPlotPartStyle, String> {
    let mut style = VegaBoxPlotPartStyle {
        visible: key != "ticks",
        fill: None,
        stroke: None,
        stroke_width: None,
        stroke_dash: Vec::new(),
        opacity: None,
        size: None,
    };
    let Some(value) = mark_value(top, key) else {
        return Ok(style);
    };
    match value {
        Value::Bool(visible) => style.visible = *visible,
        Value::Object(object) => {
            style.visible = true;
            let color = object
                .get("color")
                .map(|value| parse_style_color(value, key, "color"))
                .transpose()?;
            if key == "outliers" {
                style.stroke = color;
            } else {
                style.fill = color;
                style.stroke = color;
            }
            if let Some(value) = object.get("fill") {
                style.fill = Some(parse_style_color(value, key, "fill")?);
            }
            if let Some(value) = object.get("stroke") {
                style.stroke = Some(parse_style_color(value, key, "stroke")?);
            }
            style.stroke_width = parse_optional_nonnegative(object, "strokeWidth", key)?;
            style.size = parse_optional_nonnegative(object, "size", key)?;
            if let Some(value) = object.get("strokeDash") {
                let dash = value
                    .as_array()
                    .ok_or_else(|| format!("boxplot {key}.strokeDash must be an array"))?;
                style.stroke_dash = dash
                    .iter()
                    .map(|value| {
                        value
                            .as_f64()
                            .filter(|number| number.is_finite() && *number >= 0.0)
                            .ok_or_else(|| {
                                format!(
                                    "boxplot {key}.strokeDash values must be finite and nonnegative"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            }
            if let Some(value) = object.get("opacity") {
                let opacity = value
                    .as_f64()
                    .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
                    .ok_or_else(|| format!("boxplot {key}.opacity must be within 0..=1"))?;
                style.opacity = Some(opacity);
            }
        }
        _ => {
            return Err(format!(
                "boxplot mark.{key} must be a boolean or style object"
            ));
        }
    }
    Ok(style)
}

fn parse_style_color(value: &Value, part: &str, name: &str) -> Result<Color, String> {
    value
        .as_str()
        .and_then(crate::color::parse_color)
        .ok_or_else(|| format!("boxplot {part}.{name} must be a valid color"))
}

fn parse_optional_nonnegative(
    object: &Map<String, Value>,
    key: &str,
    part: &str,
) -> Result<Option<f64>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_f64()
                .filter(|number| number.is_finite() && *number >= 0.0)
                .ok_or_else(|| format!("boxplot {part}.{key} must be finite and nonnegative"))
        })
        .transpose()
}

fn make_axis(domain: Option<(f64, f64)>, grid: AxisGrid) -> AxisSpec {
    AxisSpec {
        title: None,
        min: domain.map(|(low, _)| low),
        max: domain.map(|(_, high)| high),
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: true,
        grid,
        border: AxisBorder::default(),
        scale_kind: ScaleKind::Linear,
        time: None,
        ticks: AxisTickOptions::default(),
    }
}

fn parse_dimensions(top: &Map<String, Value>) -> (f64, f64) {
    let width = top
        .get("width")
        .and_then(Value::as_f64)
        .filter(|width| width.is_finite() && *width > 0.0)
        .unwrap_or(800.0);
    let height = top
        .get("height")
        .and_then(Value::as_f64)
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(450.0);
    (width, height)
}

fn parse_title(top: &Map<String, Value>) -> Option<String> {
    match top.get("title") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

pub(super) fn summarize_boxplot(
    values: &[f64],
    extent: VegaBoxPlotExtent,
) -> Result<VegaBoxPlotSummary, String> {
    if values.is_empty() {
        return Err("boxplot group must contain at least one value".to_string());
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err("boxplot values must be finite".to_string());
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let q1 = type7_quantile(&sorted, 0.25);
    let median = type7_quantile(&sorted, 0.5);
    let q3 = type7_quantile(&sorted, 0.75);
    if !q1.is_finite() || !median.is_finite() || !q3.is_finite() {
        return Err("boxplot quantiles must be finite".to_string());
    }

    let data_min = sorted[0];
    let data_max = sorted[sorted.len() - 1];
    let (whisker_low, whisker_high, outliers) = match extent {
        VegaBoxPlotExtent::MinMax => (Some(data_min), Some(data_max), Vec::new()),
        VegaBoxPlotExtent::Tukey { coefficient } => {
            if !coefficient.is_finite() || coefficient < 0.0 {
                return Err("boxplot Tukey coefficient must be finite and nonnegative".to_string());
            }
            let iqr = q3 - q1;
            let scaled_iqr = coefficient * iqr;
            let fence_low = q1 - scaled_iqr;
            let fence_high = q3 + scaled_iqr;
            if !iqr.is_finite()
                || !scaled_iqr.is_finite()
                || !fence_low.is_finite()
                || !fence_high.is_finite()
            {
                return Err("boxplot Tukey fences must be finite".to_string());
            }

            let mut in_fence_min = None;
            let mut in_fence_max = None;
            let mut outliers = Vec::new();
            for &value in values {
                if value < fence_low || value > fence_high {
                    outliers.push(value);
                } else {
                    if in_fence_min.is_none_or(|min: f64| value.total_cmp(&min).is_lt()) {
                        in_fence_min = Some(value);
                    }
                    if in_fence_max.is_none_or(|max: f64| value.total_cmp(&max).is_gt()) {
                        in_fence_max = Some(value);
                    }
                }
            }
            (in_fence_min, in_fence_max, outliers)
        }
    };

    Ok(VegaBoxPlotSummary {
        q1,
        median,
        q3,
        whisker_low,
        whisker_high,
        data_min,
        data_max,
        outliers,
    })
}

#[cfg(test)]
mod tests {
    use super::summarize_boxplot;
    use crate::ir::{VegaBoxPlotExtent, VegaBoxPlotSummary};

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_optional_close(actual: Option<f64>, expected: f64) {
        assert_close(actual.expect("expected a whisker endpoint"), expected);
    }

    fn tukey(values: &[f64]) -> VegaBoxPlotSummary {
        summarize_boxplot(values, VegaBoxPlotExtent::Tukey { coefficient: 1.5 })
            .expect("valid Tukey samples should summarize")
    }

    #[test]
    fn boxplot_quantiles_use_type7_interpolation() {
        let summary = tukey(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        assert_close(summary.q1, 2.0);
        assert_close(summary.median, 3.0);
        assert_close(summary.q3, 4.0);
    }

    #[test]
    fn boxplot_tukey_uses_observed_whiskers_and_keeps_outliers() {
        let summary = tukey(&[1.0, 2.0, 3.0, 4.0, 5.0, 100.0]);

        assert_optional_close(summary.whisker_low, 1.0);
        assert_optional_close(summary.whisker_high, 5.0);
        assert_eq!(summary.outliers, [100.0]);
        assert_close(summary.data_min, 1.0);
        assert_close(summary.data_max, 100.0);
    }

    #[test]
    fn boxplot_tukey_zero_omits_whiskers_when_fence_has_no_sample() {
        let summary = summarize_boxplot(&[1.0, 2.0], VegaBoxPlotExtent::Tukey { coefficient: 0.0 })
            .expect("valid zero extent should summarize");

        assert_eq!(summary.whisker_low, None);
        assert_eq!(summary.whisker_high, None);
        assert_eq!(summary.outliers, [1.0, 2.0]);
    }

    #[test]
    fn boxplot_min_max_uses_data_extrema_without_outliers() {
        let summary = summarize_boxplot(&[10.0, 20.0, 30.0], VegaBoxPlotExtent::MinMax)
            .expect("valid min-max samples should summarize");

        assert_optional_close(summary.whisker_low, 10.0);
        assert_optional_close(summary.whisker_high, 30.0);
        assert!(summary.outliers.is_empty());
    }

    #[test]
    fn boxplot_summary_handles_singleton_and_constant_samples() {
        for values in [&[4.0][..], &[4.0, 4.0, 4.0][..]] {
            let summary = tukey(values);
            assert_close(summary.q1, 4.0);
            assert_close(summary.median, 4.0);
            assert_close(summary.q3, 4.0);
            assert_optional_close(summary.whisker_low, 4.0);
            assert_optional_close(summary.whisker_high, 4.0);
            assert!(summary.outliers.is_empty());
        }
    }

    #[test]
    fn boxplot_summary_rejects_empty_nonfinite_and_overflowing_fences() {
        let default_extent = VegaBoxPlotExtent::Tukey { coefficient: 1.5 };
        assert!(summarize_boxplot(&[], default_extent).is_err());
        assert!(summarize_boxplot(&[f64::NAN], default_extent).is_err());
        assert!(summarize_boxplot(&[1.0], VegaBoxPlotExtent::Tukey { coefficient: -1.0 }).is_err());
        assert!(
            summarize_boxplot(
                &[1.0],
                VegaBoxPlotExtent::Tukey {
                    coefficient: f64::INFINITY,
                },
            )
            .is_err()
        );
        assert!(
            summarize_boxplot(
                &[1.0, 2.0, 3.0, 4.0, 5.0],
                VegaBoxPlotExtent::Tukey {
                    coefficient: f64::MAX,
                },
            )
            .is_err()
        );
    }
}
