//! Parsing for the bounded Vega-Lite text mark subset.

use super::*;
use serde_json::{Map, Value};

const TEXT_SIZE_RANGE: (f64, f64) = (8.0, 40.0);
const TEXT_OPACITY_RANGE: (f64, f64) = (0.3, 0.8);
const DEFAULT_TEXT_SIZE: f64 = 11.0;

pub(super) fn parse_text_spec(
    top: &Map<String, Value>,
    strict: bool,
    limits: &crate::guard::InputLimits,
    scale_overrides: &super::super::vegalite_composition::VegaUnitScaleOverrides,
) -> Result<ChartSpec, String> {
    check_text_spec_keys(top, strict)?;

    let mark = top
        .get("mark")
        .ok_or_else(|| "text mark is required".to_string())?;
    let mark_object = match mark {
        Value::String(value) if value == "text" => None,
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("text") => {
            Some(object)
        }
        _ => return Err("text mark must be \"text\" or an object with type \"text\"".into()),
    };

    let data = top
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "text mark requires inline data.values".to_string())?;
    if data.contains_key("url") {
        return Err("text mark data.url is unsupported; provide inline data.values".into());
    }
    if data.contains_key("format") {
        return Err("text mark data.format is unsupported".into());
    }
    for key in data.keys() {
        if key != "values" {
            return Err(format!("text mark does not support data.{key}"));
        }
    }
    let row_count = data
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "text mark requires inline data.values".to_string())?
        .len();
    if row_count > limits.max_total_data_points {
        return Err(format!(
            "text mark point count {row_count} exceeds max_total_data_points limit {}",
            limits.max_total_data_points
        ));
    }
    if row_count > limits.max_categorical_primitives {
        return Err(format!(
            "text mark count {row_count} exceeds max_categorical_primitives limit {}",
            limits.max_categorical_primitives
        ));
    }
    let font_attribute_bytes =
        preflight_font_family_attribute_bytes(Some(mark), row_count, limits)?;
    let records = super::parse_data_values(top.get("data"))?;

    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "text mark requires an encoding object".to_string())?;
    let x_field = parse_position_field(encoding.get("x"), "x")?;
    let y_field = parse_position_field(encoding.get("y"), "y")?;
    let text_source = parse_text_source(encoding.get("text"), mark_object)?;
    let label_bytes = total_label_bytes_for_records(&records, &text_source, limits)?;
    let expanded_text_bytes = label_bytes.saturating_add(font_attribute_bytes);
    if expanded_text_bytes > limits.max_total_text_bytes {
        return Err(total_text_and_font_bytes_error(
            expanded_text_bytes,
            limits.max_total_text_bytes,
        ));
    }

    let points = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let x = finite_field(record, &x_field, "x", index)?;
            let y = finite_field(record, &y_field, "y", index)?;
            let text = match &text_source {
                TextSource::Field(field) => scalar_text(
                    record.get(field),
                    &format!("text mark data.values[{index}].{field}"),
                )?,
                TextSource::Value(text) => text.clone(),
            };
            if text.contains(['\n', '\r']) {
                return Err(format!(
                    "text mark data.values[{index}] is multiline; multiline text is unsupported"
                ));
            }
            if text.len() > limits.max_label_bytes {
                return Err(format!(
                    "text mark label length {} exceeds max_label_bytes limit {} at data.values[{index}]",
                    text.len(), limits.max_label_bytes
                ));
            }
            Ok((Point { x, y, r: None }, text))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut theme = vegalite_theme();
    if let Some(background) = top.get("background").filter(|value| !value.is_null()) {
        theme.background = Some(parse_css_color(background, "background")?);
    }

    let mark_color = mark_object
        .and_then(|mark| mark.get("color"))
        .filter(|value| !value.is_null())
        .map(|value| parse_css_color(value, "mark.color"))
        .transpose()?
        .unwrap_or(theme.text_color);
    let mark_opacity = mark_object
        .and_then(|mark| mark.get("opacity"))
        .filter(|value| !value.is_null())
        .map(|value| parse_opacity_value(value, "mark.opacity"))
        .transpose()?
        .unwrap_or(1.0);
    let mark_size = mark_object
        .and_then(|mark| mark.get("fontSize"))
        .filter(|value| !value.is_null())
        .map(|value| parse_positive_number(value, "mark.fontSize", limits))
        .transpose()?
        .unwrap_or(DEFAULT_TEXT_SIZE);
    let font_family = mark_object
        .and_then(|mark| mark.get("font"))
        .filter(|value| !value.is_null())
        .map(|value| parse_nonempty_string(value, "mark.font"))
        .transpose()?;
    let font_weight = mark_object
        .and_then(|mark| mark.get("fontWeight"))
        .filter(|value| !value.is_null())
        .map(parse_font_weight)
        .transpose()?;
    let font_style = mark_object
        .and_then(|mark| mark.get("fontStyle"))
        .filter(|value| !value.is_null())
        .map(parse_font_style)
        .transpose()?;
    let align = mark_object
        .and_then(|mark| mark.get("align"))
        .filter(|value| !value.is_null())
        .map(parse_align)
        .transpose()?
        .unwrap_or(VegaTextAlign::Center);
    let baseline = mark_object
        .and_then(|mark| mark.get("baseline"))
        .filter(|value| !value.is_null())
        .map(parse_baseline)
        .transpose()?
        .unwrap_or(TextBaseline::Middle);
    let angle = mark_object
        .and_then(|mark| mark.get("angle"))
        .filter(|value| !value.is_null())
        .map(|value| parse_finite_number(value, "mark.angle"))
        .transpose()?
        .map(|angle| angle.rem_euclid(360.0));
    let dx = mark_object
        .and_then(|mark| mark.get("dx"))
        .filter(|value| !value.is_null())
        .map(|value| parse_finite_number(value, "mark.dx"))
        .transpose()?
        .unwrap_or(0.0);
    let dy = mark_object
        .and_then(|mark| mark.get("dy"))
        .filter(|value| !value.is_null())
        .map(|value| parse_finite_number(value, "mark.dy"))
        .transpose()?
        .unwrap_or(0.0);

    let color_field = parse_channel_field(encoding.get("color"), "color", &records)?;
    let color_value = parse_channel_value(encoding.get("color"), "color")?;
    if color_field.is_some() && color_value.is_some() {
        return Err("text mark encoding.color must specify field or value, not both".into());
    }
    let categories = color_field
        .as_deref()
        .map(|field| {
            scale_overrides
                .color_categories
                .clone()
                .unwrap_or_else(|| super::distinct_categories(&records, Some(field)))
        })
        .unwrap_or_default();
    let size_field = parse_quantitative_channel_field(encoding.get("size"), "size", &records)?;
    let opacity_field =
        parse_quantitative_channel_field(encoding.get("opacity"), "opacity", &records)?;

    let size_values = numeric_field_values(&records, size_field.as_deref(), "size")?;
    let size_domain = scale_overrides
        .size_numeric_domain
        .or_else(|| numeric_domain(&size_values))
        .unwrap_or((0.0, 1.0));
    let opacity_values = numeric_field_values(&records, opacity_field.as_deref(), "opacity")?;
    let opacity_domain = scale_overrides
        .opacity_numeric_domain
        .or_else(|| numeric_domain(&opacity_values))
        .unwrap_or((0.0, 1.0));

    let encoding_size_value = parse_channel_number_value(encoding.get("size"), "size")?;
    let encoding_opacity_value = parse_channel_number_value(encoding.get("opacity"), "opacity")?;
    let color_constant = color_value
        .map(|value| {
            parse_color(value).ok_or_else(|| {
                "text mark encoding.color.value must be a valid CSS color".to_string()
            })
        })
        .transpose()?;

    let marks = points
        .into_iter()
        .enumerate()
        .map(|(index, (point, text))| {
            let mut fill = if let Some(color) = color_constant {
                color
            } else if let Some(field) = color_field.as_deref() {
                let category = super::field_category(&records[index], Some(field));
                let color_index = categories
                    .iter()
                    .position(|candidate| candidate == &category)
                    .unwrap_or(0);
                theme
                    .palette
                    .get(color_index % theme.palette.len().max(1))
                    .copied()
                    .unwrap_or(theme.text_color)
            } else {
                mark_color
            };
            let opacity = if let Some(value) = encoding_opacity_value {
                parse_opacity_value(value, "encoding.opacity.value")?
            } else if let Some(field) = opacity_field.as_deref() {
                map_domain(
                    finite_field(&records[index], field, "opacity", index)?,
                    opacity_domain,
                    TEXT_OPACITY_RANGE,
                )
            } else {
                mark_opacity
            };
            fill.a = (fill.a * opacity as f32).clamp(0.0, 1.0);

            let size = if let Some(value) = encoding_size_value {
                parse_positive_number(value, "encoding.size.value", limits)?
            } else if let Some(field) = size_field.as_deref() {
                let value = finite_field(&records[index], field, "size", index)?;
                map_domain(value, size_domain, TEXT_SIZE_RANGE)
            } else {
                mark_size
            };

            Ok(VegaTextMark {
                point,
                text,
                fill,
                size,
                align,
                baseline,
                angle,
                dx,
                dy,
                font_family: font_family.clone(),
                font_weight: font_weight.clone(),
                font_style: font_style.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let title = match top.get("title") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    };
    let width = parse_chart_dimension(top.get("width"), "width", 800.0, limits)?;
    let height = parse_chart_dimension(top.get("height"), "height", 450.0, limits)?;

    Ok(ChartSpec {
        kind: ChartKind::VegaText(Box::new(VegaTextData { marks })),
        series: Vec::new(),
        categories: Vec::new(),
        x_positions: XPositions::Category,
        y_positions: XPositions::Category,
        x_axis: text_axis_spec(),
        y_axis: text_axis_spec(),
        legend: LegendPos::None,
        legend_options: LegendOptions::default(),
        legend_title: None,
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

enum TextSource {
    Field(String),
    Value(String),
}

fn total_label_bytes_for_records(
    records: &[Map<String, Value>],
    source: &TextSource,
    limits: &crate::guard::InputLimits,
) -> Result<usize, String> {
    let mut total = 0usize;
    match source {
        TextSource::Value(text) => {
            if text.len() > limits.max_label_bytes {
                return Err(format!(
                    "text mark label length {} exceeds max_label_bytes limit {}",
                    text.len(),
                    limits.max_label_bytes
                ));
            }
            total = text.len().saturating_mul(records.len());
        }
        TextSource::Field(field) => {
            for (index, record) in records.iter().enumerate() {
                let path = format!("text mark data.values[{index}].{field}");
                let bytes = scalar_text_byte_len(record.get(field), &path)?;
                if bytes > limits.max_label_bytes {
                    return Err(format!(
                        "text mark label length {bytes} exceeds max_label_bytes limit {} at data.values[{index}]",
                        limits.max_label_bytes
                    ));
                }
                total = total.saturating_add(bytes);
                if total > limits.max_total_text_bytes {
                    return Err(total_text_bytes_error(total, limits.max_total_text_bytes));
                }
            }
        }
    }
    if total > limits.max_total_text_bytes {
        return Err(total_text_bytes_error(total, limits.max_total_text_bytes));
    }
    Ok(total)
}

pub(super) fn preflight_font_family_attribute_bytes(
    mark: Option<&Value>,
    mark_count: usize,
    limits: &crate::guard::InputLimits,
) -> Result<usize, String> {
    let font_value = mark
        .and_then(Value::as_object)
        .and_then(|mark| mark.get("font"))
        .filter(|value| !value.is_null());
    let (font_family, is_explicit) = match font_value {
        None => (crate::font::DEFAULT_SVG_FONT_FAMILY, false),
        Some(Value::String(font_family)) if !font_family.is_empty() => (font_family.as_str(), true),
        Some(Value::String(_)) => (crate::font::DEFAULT_SVG_FONT_FAMILY, false),
        // Invalid font values are reported by the normal parser before any font copy is made.
        Some(_) => return Ok(0),
    };
    if is_explicit && font_family.len() > limits.max_label_bytes {
        return Err(format!(
            "mark.font length {} bytes exceeds max_label_bytes limit {}",
            font_family.len(),
            limits.max_label_bytes
        ));
    }
    Ok(crate::svg::xml_escape_attr_len(font_family).saturating_mul(mark_count))
}

fn total_text_and_font_bytes_error(total: usize, limit: usize) -> String {
    format!(
        "text mark label and font-family SVG attribute bytes {total} exceeds max_total_text_bytes limit {limit}"
    )
}

fn scalar_text_byte_len(value: Option<&Value>, path: &str) -> Result<usize, String> {
    match value {
        Some(Value::String(text)) => Ok(text.len()),
        Some(Value::Number(number)) => Ok(number.to_string().len()),
        Some(Value::Bool(value)) => Ok(value.to_string().len()),
        _ => Err(format!(
            "{path} must be a non-null string, number, or boolean"
        )),
    }
}

fn total_text_bytes_error(total: usize, limit: usize) -> String {
    format!("text mark label bytes {total} exceeds max_total_text_bytes limit {limit}")
}

/// Count the expanded labels for a raw text unit before composition parsing clones them into IR.
pub(super) fn preflight_label_bytes(
    data: Option<&Value>,
    encoding: Option<&Value>,
    mark: Option<&Value>,
    limits: &crate::guard::InputLimits,
) -> Result<usize, String> {
    let source = parse_text_source(
        encoding
            .and_then(Value::as_object)
            .and_then(|encoding| encoding.get("text")),
        mark.and_then(Value::as_object),
    )?;
    let records = data
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
        .and_then(Value::as_array);
    let count = records.map_or(0, Vec::len);
    let total = match &source {
        TextSource::Value(text) => {
            if text.len() > limits.max_label_bytes {
                return Err(format!(
                    "text mark label length {} exceeds max_label_bytes limit {}",
                    text.len(),
                    limits.max_label_bytes
                ));
            }
            text.len().saturating_mul(count)
        }
        TextSource::Field(field) => {
            let mut total = 0usize;
            if let Some(records) = records {
                for (index, record) in records.iter().enumerate() {
                    let value = record.as_object().and_then(|record| record.get(field));
                    let bytes = scalar_text_byte_len(
                        value,
                        &format!("text mark data.values[{index}].{field}"),
                    )?;
                    if bytes > limits.max_label_bytes {
                        return Err(format!(
                            "text mark label length {bytes} exceeds max_label_bytes limit {} at data.values[{index}]",
                            limits.max_label_bytes
                        ));
                    }
                    total = total.saturating_add(bytes);
                    if total > limits.max_total_text_bytes {
                        return Err(total_text_bytes_error(total, limits.max_total_text_bytes));
                    }
                }
            }
            total
        }
    };
    if total > limits.max_total_text_bytes {
        return Err(total_text_bytes_error(total, limits.max_total_text_bytes));
    }
    Ok(total)
}

fn parse_text_source(
    channel: Option<&Value>,
    mark: Option<&Map<String, Value>>,
) -> Result<TextSource, String> {
    let channel = channel.filter(|value| !value.is_null());
    let mark_text = mark
        .and_then(|mark| mark.get("text"))
        .filter(|value| !value.is_null());
    if channel.is_some() && mark_text.is_some() {
        return Err("text mark must use exactly one text source".into());
    }
    if let Some(mark_text) = mark_text {
        let text = mark_text
            .as_str()
            .ok_or_else(|| "text mark mark.text must be a literal string".to_string())?;
        if text.contains(['\n', '\r']) {
            return Err("text mark mark.text is multiline; multiline text is unsupported".into());
        }
        return Ok(TextSource::Value(text.to_owned()));
    }
    let channel = channel.ok_or_else(|| "text mark requires one text source".to_string())?;
    let channel = channel
        .as_object()
        .ok_or_else(|| "text mark encoding.text must be an object".to_string())?;
    check_channel_keys(channel, "text", &["field", "value", "type"])?;
    match (channel.get("field"), channel.get("value")) {
        (Some(Value::String(field)), None) if !field.is_empty() => {
            Ok(TextSource::Field(field.clone()))
        }
        (None, Some(value)) => Ok(TextSource::Value(scalar_text(
            Some(value),
            "text mark encoding.text.value",
        )?)),
        (Some(_), Some(_)) => {
            Err("text mark encoding.text must specify field or value, not both".into())
        }
        _ => Err("text mark encoding.text requires field or value".into()),
    }
}

fn parse_position_field(channel: Option<&Value>, name: &str) -> Result<String, String> {
    let channel = channel
        .filter(|value| !value.is_null())
        .and_then(Value::as_object)
        .ok_or_else(|| format!("text mark encoding.{name} must be an object"))?;
    check_channel_keys(channel, name, &["field", "type"])?;
    let field = channel
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("text mark encoding.{name}.field is required"))?;
    if let Some(field_type) = channel.get("type").filter(|value| !value.is_null())
        && field_type.as_str() != Some("quantitative")
    {
        return Err(format!(
            "text mark encoding.{name}.type must be \"quantitative\""
        ));
    }
    Ok(field.to_owned())
}

fn parse_channel_field(
    channel: Option<&Value>,
    name: &str,
    records: &[Map<String, Value>],
) -> Result<Option<String>, String> {
    let Some(channel) = channel.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let channel = channel
        .as_object()
        .ok_or_else(|| format!("text mark encoding.{name} must be an object"))?;
    check_channel_keys(channel, name, &["field", "value", "type"])?;
    let has_field = channel.get("field").is_some_and(|value| !value.is_null());
    let has_value = channel.get("value").is_some_and(|value| !value.is_null());
    if has_field && has_value {
        return Err(format!(
            "text mark encoding.{name} must specify field or value, not both"
        ));
    }
    if has_value {
        return Ok(None);
    }
    let field = channel
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("text mark encoding.{name} requires field or value"))?;
    if let Some(field_type) = channel.get("type").filter(|value| !value.is_null())
        && !matches!(field_type.as_str(), Some("nominal" | "ordinal"))
    {
        return Err(format!(
            "text mark encoding.{name}.type must be \"nominal\" or \"ordinal\""
        ));
    }
    for (index, record) in records.iter().enumerate() {
        match record.get(field) {
            Some(Value::String(_) | Value::Number(_) | Value::Bool(_)) => {}
            _ => {
                return Err(format!(
                    "text mark data.values[{index}].{field} must be a non-null categorical value"
                ));
            }
        }
    }
    Ok(Some(field.to_owned()))
}

fn parse_quantitative_channel_field(
    channel: Option<&Value>,
    name: &str,
    records: &[Map<String, Value>],
) -> Result<Option<String>, String> {
    let Some(channel) = channel.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let channel = channel
        .as_object()
        .ok_or_else(|| format!("text mark encoding.{name} must be an object"))?;
    check_channel_keys(channel, name, &["field", "value", "type"])?;
    let has_field = channel.get("field").is_some_and(|value| !value.is_null());
    let has_value = channel.get("value").is_some_and(|value| !value.is_null());
    if has_field && has_value {
        return Err(format!(
            "text mark encoding.{name} must specify field or value, not both"
        ));
    }
    if has_value {
        return Ok(None);
    }
    let field = channel
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("text mark encoding.{name} requires field or value"))?;
    if let Some(field_type) = channel.get("type").filter(|value| !value.is_null())
        && field_type.as_str() != Some("quantitative")
    {
        return Err(format!(
            "text mark encoding.{name}.type must be \"quantitative\""
        ));
    }
    let _ = numeric_field_values(records, Some(field), name)?;
    Ok(Some(field.to_owned()))
}

fn parse_channel_value<'a>(
    channel: Option<&'a Value>,
    name: &str,
) -> Result<Option<&'a str>, String> {
    let Some(channel) = channel.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let channel = channel
        .as_object()
        .ok_or_else(|| format!("text mark encoding.{name} must be an object"))?;
    if let Some(value) = channel.get("value").filter(|value| !value.is_null()) {
        return value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("text mark encoding.{name}.value must be a string"));
    }
    Ok(None)
}

fn parse_channel_number_value<'a>(
    channel: Option<&'a Value>,
    name: &str,
) -> Result<Option<&'a Value>, String> {
    let Some(channel) = channel.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let channel = channel
        .as_object()
        .ok_or_else(|| format!("text mark encoding.{name} must be an object"))?;
    if let Some(value) = channel.get("value").filter(|value| !value.is_null()) {
        return Ok(Some(value));
    }
    Ok(None)
}

fn finite_field(
    record: &Map<String, Value>,
    field: &str,
    channel: &str,
    index: usize,
) -> Result<f64, String> {
    record
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            format!("text mark data.values[{index}].{field} for {channel} must be finite")
        })
}

fn scalar_text(value: Option<&Value>, path: &str) -> Result<String, String> {
    match value {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Number(number)) => Ok(number.to_string()),
        Some(Value::Bool(value)) => Ok(value.to_string()),
        _ => Err(format!(
            "{path} must be a non-null string, number, or boolean"
        )),
    }
}

fn numeric_field_values(
    records: &[Map<String, Value>],
    field: Option<&str>,
    name: &str,
) -> Result<Vec<f64>, String> {
    let Some(field) = field else {
        return Ok(Vec::new());
    };
    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            record
                .get(field)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("text mark data.values[{index}].{field} for {name} must be finite")
                })
        })
        .collect()
}

fn numeric_domain(values: &[f64]) -> Option<(f64, f64)> {
    let (&first, tail) = values.split_first()?;
    Some(tail.iter().fold((first, first), |(min, max), value| {
        (min.min(*value), max.max(*value))
    }))
}

fn map_domain(value: f64, domain: (f64, f64), range: (f64, f64)) -> f64 {
    let (min, max) = domain;
    if min == max {
        return (range.0 + range.1) / 2.0;
    }
    let fraction = ((value - min) / (max - min)).clamp(0.0, 1.0);
    range.0 + fraction * (range.1 - range.0)
}

fn parse_css_color(value: &Value, path: &str) -> Result<Color, String> {
    value
        .as_str()
        .and_then(parse_color)
        .ok_or_else(|| format!("text mark {path} must be a valid CSS color"))
}

fn parse_opacity_value(value: &Value, path: &str) -> Result<f64, String> {
    let value = parse_finite_number(value, path)?;
    parse_opacity_number(value, path)
}

fn parse_opacity_number(value: f64, path: &str) -> Result<f64, String> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!("text mark {path} must be between 0 and 1"))
    }
}

fn parse_positive_number(
    value: &Value,
    path: &str,
    limits: &crate::guard::InputLimits,
) -> Result<f64, String> {
    let value = parse_finite_number(value, path)?;
    if value > 0.0 && value <= limits.max_dimension_px {
        Ok(value)
    } else {
        Err(format!(
            "text mark {path} must be positive and at most {}",
            limits.max_dimension_px
        ))
    }
}

fn parse_finite_number(value: &Value, path: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("text mark {path} must be a finite number"))
}

fn parse_nonempty_string(value: &Value, path: &str) -> Result<String, String> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("text mark {path} must be a non-empty string"))
}

fn parse_font_weight(value: &Value) -> Result<String, String> {
    let weight = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => return Err("text mark fontWeight must be a string or number".into()),
    };
    if matches!(weight.as_str(), "normal" | "bold" | "bolder" | "lighter")
        || weight
            .parse::<u16>()
            .is_ok_and(|value| (100..=900).contains(&value) && value % 100 == 0)
    {
        Ok(weight)
    } else {
        Err("text mark fontWeight must be normal, bold, bolder, lighter, or 100..900 by 100".into())
    }
}

fn parse_font_style(value: &Value) -> Result<String, String> {
    match value.as_str() {
        Some(style @ ("normal" | "italic" | "oblique")) => Ok(style.to_owned()),
        _ => Err("text mark fontStyle must be normal, italic, or oblique".into()),
    }
}

fn parse_align(value: &Value) -> Result<VegaTextAlign, String> {
    match value.as_str() {
        Some("left") => Ok(VegaTextAlign::Left),
        Some("center") => Ok(VegaTextAlign::Center),
        Some("right") => Ok(VegaTextAlign::Right),
        _ => Err("text mark align must be left, center, or right".into()),
    }
}

fn parse_baseline(value: &Value) -> Result<TextBaseline, String> {
    match value.as_str() {
        Some("top") => Ok(TextBaseline::Top),
        Some("middle") => Ok(TextBaseline::Middle),
        Some("bottom") => Ok(TextBaseline::Bottom),
        _ => Err("text mark baseline must be top, middle, or bottom".into()),
    }
}

fn parse_chart_dimension(
    value: Option<&Value>,
    name: &str,
    default: f64,
    limits: &crate::guard::InputLimits,
) -> Result<f64, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(default);
    };
    let value = parse_finite_number(value, name)?;
    if value > 0.0 && value <= limits.max_dimension_px {
        Ok(value)
    } else {
        Err(format!(
            "text mark {name} must be positive and at most {}",
            limits.max_dimension_px
        ))
    }
}

fn check_text_spec_keys(top: &Map<String, Value>, _strict: bool) -> Result<(), String> {
    for key in [
        "transform",
        "projection",
        "facet",
        "repeat",
        "layer",
        "hconcat",
        "vconcat",
    ] {
        if top.contains_key(key) {
            return Err(format!("text mark does not support {key}"));
        }
    }
    let top_allowed = [
        "mark",
        "data",
        "encoding",
        "$schema",
        "width",
        "height",
        "title",
        "background",
    ];
    for key in top.keys() {
        if !top_allowed.contains(&key.as_str()) {
            return Err(format!("text mark does not support top-level {key}"));
        }
    }

    if let Some(mark) = top.get("mark").and_then(Value::as_object) {
        check_channel_keys(
            mark,
            "mark",
            &[
                "type",
                "text",
                "color",
                "opacity",
                "font",
                "fontSize",
                "fontWeight",
                "fontStyle",
                "align",
                "baseline",
                "angle",
                "dx",
                "dy",
            ],
        )?;
    }
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "text mark requires an encoding object".to_string())?;
    for channel in encoding.keys() {
        if !matches!(
            channel.as_str(),
            "x" | "y" | "text" | "color" | "size" | "opacity"
        ) {
            return Err(format!("text mark does not support encoding.{channel}"));
        }
    }
    for channel in ["x", "y"] {
        if let Some(value) = encoding.get(channel).filter(|value| !value.is_null()) {
            let object = value
                .as_object()
                .ok_or_else(|| format!("text mark encoding.{channel} must be an object"))?;
            check_channel_keys(object, channel, &["field", "type"])?;
        }
    }
    for channel in ["text", "color", "size", "opacity"] {
        if let Some(value) = encoding.get(channel).filter(|value| !value.is_null()) {
            let object = value
                .as_object()
                .ok_or_else(|| format!("text mark encoding.{channel} must be an object"))?;
            check_channel_keys(object, channel, &["field", "value", "type"])?;
        }
    }
    Ok(())
}

fn check_channel_keys(
    object: &Map<String, Value>,
    path: &str,
    allowed: &[&str],
) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        let path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        return Err(format!("text mark does not support {path}"));
    }
    Ok(())
}

fn text_axis_spec() -> AxisSpec {
    AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: false,
        grid: AxisGrid::default(),
        border: AxisBorder::default(),
        scale_kind: ScaleKind::Linear,
        time: None,
        ticks: AxisTickOptions::default(),
    }
}
