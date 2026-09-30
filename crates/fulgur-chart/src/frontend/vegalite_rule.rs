//! Parser for Vega-Lite's Cartesian `rule` mark subset.

use super::*;
use crate::ir::{
    AxisBorder, AxisGrid, AxisSpec, AxisTitle, AxisTitleAlign, ChartKind, ChartSpec, Color,
    Decimation, LegendOptions, LegendPos, ScaleKind, Series, SeriesType, SizeMode, VegaRuleData,
    VegaRulePosition, VegaRuleSegment, XPositions,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

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
        index: usize,
        channel: &str,
    ) -> Result<usize, String> {
        let value_type = match value {
            Value::String(_) => CategoryValueType::String,
            Value::Number(_) => CategoryValueType::Number,
            Value::Bool(_) => CategoryValueType::Boolean,
            _ => unreachable!("category values are validated before indexing"),
        };
        if self
            .value_types
            .get(&label)
            .is_some_and(|existing| *existing != value_type)
        {
            return Err(format!(
                "rule data.values[{index}].{channel} category label {label:?} has conflicting JSON value types"
            ));
        }
        self.value_types.insert(label.clone(), value_type);
        if let Some(index) = self.indexes.get(&label) {
            return Ok(*index);
        }
        let category_index = self.labels.len();
        self.labels.push(label.clone());
        self.indexes.insert(label, category_index);
        Ok(category_index)
    }
}

pub(super) fn parse_rule_spec(
    top: &Map<String, Value>,
    limits: &crate::guard::InputLimits,
    scale_overrides: &super::super::vegalite_composition::VegaUnitScaleOverrides,
) -> Result<ChartSpec, String> {
    check_unknown_keys(top)?;
    preflight_stroke_dash_length(top.get("mark"))?;
    let values = top
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
        .and_then(Value::as_array)
        .ok_or_else(|| "rule mark requires inline data.values".to_string())?;
    if values.len() > limits.max_total_data_points {
        return Err(format!(
            "rule mark point count {} exceeds max_total_data_points limit {} (pre-allocation)",
            values.len(),
            limits.max_total_data_points
        ));
    }
    if values.len() > limits.max_categorical_primitives {
        return Err(format!(
            "rule mark count {} exceeds max_categorical_primitives limit {} (pre-allocation)",
            values.len(),
            limits.max_categorical_primitives
        ));
    }
    validate_schema(top)?;
    validate_temporal_view(top)?;

    let mark = top.get("mark").and_then(Value::as_object);
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "rule mark requires an encoding object".to_string())?;
    let records = super::parse_data_values(top.get("data"))?;
    if records.is_empty() {
        return Err("rule mark data.values must contain at least one record".into());
    }

    let x_field = super::channel_field(encoding, "x");
    let y_field = super::channel_field(encoding, "y");
    let x2_field = super::channel_field(encoding, "x2");
    let y2_field = super::channel_field(encoding, "y2");
    let has_x = channel_present(encoding, "x");
    let has_y = channel_present(encoding, "y");
    let has_x2 = channel_present(encoding, "x2");
    let has_y2 = channel_present(encoding, "y2");
    if !has_x && !has_y {
        return Err("rule mark requires encoding.x or encoding.y".into());
    }
    if has_x2 && !has_x {
        return Err("encoding.x2 requires encoding.x".into());
    }
    if has_y2 && !has_y {
        return Err("encoding.y2 requires encoding.y".into());
    }
    if has_x != has_y && (has_x2 || has_y2) {
        return Err("ranged rule marks require both encoding.x and encoding.y".into());
    }

    let x_kind = x_field
        .as_deref()
        .map(|field| position_kind(&records, encoding, "x", field))
        .transpose()?;
    let y_kind = y_field
        .as_deref()
        .map(|field| position_kind(&records, encoding, "y", field))
        .transpose()?;
    if let (Some(kind), Some(field)) = (x_kind, x2_field.as_deref()) {
        validate_endpoint_kind(&records, encoding, "x2", field, kind)?;
    }
    if let (Some(kind), Some(field)) = (y_kind, y2_field.as_deref()) {
        validate_endpoint_kind(&records, encoding, "y2", field, kind)?;
    }

    let mut x_categories = CategoryDomain::default();
    let mut y_categories = CategoryDomain::default();
    let mut raw_segments = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        let (x1, y1, x2, y2) = match (x_field.as_deref(), y_field.as_deref()) {
            (Some(x_field), None) => {
                let x = position(
                    record,
                    x_field,
                    x_kind.expect("x field has a kind"),
                    &mut x_categories,
                    index,
                    "x",
                )?;
                (
                    x,
                    VegaRulePosition::FullAxisStart,
                    x,
                    VegaRulePosition::FullAxisEnd,
                )
            }
            (None, Some(y_field)) => {
                let y = position(
                    record,
                    y_field,
                    y_kind.expect("y field has a kind"),
                    &mut y_categories,
                    index,
                    "y",
                )?;
                (
                    VegaRulePosition::FullAxisStart,
                    y,
                    VegaRulePosition::FullAxisEnd,
                    y,
                )
            }
            (Some(x_field), Some(y_field)) => {
                let x1 = position(
                    record,
                    x_field,
                    x_kind.expect("x field has a kind"),
                    &mut x_categories,
                    index,
                    "x",
                )?;
                let y1 = position(
                    record,
                    y_field,
                    y_kind.expect("y field has a kind"),
                    &mut y_categories,
                    index,
                    "y",
                )?;
                let x2 = if let Some(field) = x2_field.as_deref() {
                    position(
                        record,
                        field,
                        x_kind.expect("x field has a kind"),
                        &mut x_categories,
                        index,
                        "x2",
                    )?
                } else {
                    x1
                };
                let y2 = if let Some(field) = y2_field.as_deref() {
                    position(
                        record,
                        field,
                        y_kind.expect("y field has a kind"),
                        &mut y_categories,
                        index,
                        "y2",
                    )?
                } else {
                    y1
                };
                (x1, y1, x2, y2)
            }
            (None, None) => unreachable!("the rule position channels were checked"),
        };
        raw_segments.push((x1, y1, x2, y2));
    }

    let (color_field, color_value) = parse_color_channel(encoding)?;
    if color_field.is_some() && color_value.is_some() {
        return Err("encoding.color must specify field or value, not both".into());
    }
    if let Some(field) = color_field.as_deref() {
        super::validate_category(&records, field)?;
    }
    let mut local_color_domain = CategoryDomain::default();
    if let Some(field) = color_field.as_deref() {
        for (index, record) in records.iter().enumerate() {
            let value = record
                .get(field)
                .expect("color fields are validated before building their domain");
            local_color_domain.index(
                value,
                super::field_category(record, Some(field)),
                index,
                "color",
            )?;
        }
    }
    let color_categories = local_color_domain.labels;
    let color_domain = scale_overrides
        .color_categories
        .as_deref()
        .unwrap_or(&color_categories);
    if color_field.is_some() && color_domain.len() > limits.max_series {
        return Err(format!(
            "rule mark series count {} exceeds max_series limit {}",
            color_domain.len(),
            limits.max_series
        ));
    }
    let mut color_indexes = HashMap::new();
    if color_field.is_some() {
        color_indexes.reserve(color_domain.len());
        for (index, category) in color_domain.iter().enumerate() {
            color_indexes.entry(category.as_str()).or_insert(index);
        }
    }
    let mark_color = mark
        .and_then(|mark| mark.get("color"))
        .filter(|value| !value.is_null())
        .map(|value| parse_color_value(value, "mark.color"))
        .transpose()?
        .unwrap_or_else(|| palette_color(0));
    let color_constant = color_value
        .map(|value| {
            super::parse_color(&value)
                .ok_or_else(|| String::from("rule mark encoding.color.value must be a valid color"))
        })
        .transpose()?;
    let opacity = mark
        .and_then(|mark| mark.get("opacity"))
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_f64()
                .ok_or_else(|| String::from("mark.opacity must be a number"))
        })
        .transpose()?
        .unwrap_or(1.0);
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err("mark.opacity must be between 0 and 1".into());
    }
    let stroke_width = mark
        .and_then(|mark| mark.get("strokeWidth"))
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_f64()
                .filter(|width| width.is_finite() && *width >= 0.0)
                .ok_or_else(|| {
                    String::from("mark.strokeWidth must be a finite non-negative number")
                })
        })
        .transpose()?
        .unwrap_or(1.5);
    let stroke_dash = parse_stroke_dash(mark)?;
    let clip = mark
        .and_then(|mark| mark.get("clip"))
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| String::from("mark.clip must be a boolean"))
        })
        .transpose()?
        .unwrap_or(false);

    let mut segments = Vec::with_capacity(raw_segments.len());
    for (index, (x1, y1, x2, y2)) in raw_segments.into_iter().enumerate() {
        let mut color = if let Some(constant) = color_constant {
            constant
        } else if let Some(field) = color_field.as_deref() {
            let label = super::field_category(&records[index], Some(field));
            let color_index = color_indexes.get(label.as_str()).copied().unwrap_or(0);
            palette_color(color_index)
        } else {
            mark_color
        };
        color.a = (color.a * opacity as f32).clamp(0.0, 1.0);
        segments.push(VegaRuleSegment {
            x1,
            y1,
            x2,
            y2,
            color,
        });
    }

    let category_count = x_categories
        .labels
        .len()
        .saturating_add(y_categories.labels.len());
    if category_count > limits.max_categories {
        return Err(format!(
            "rule mark category count {category_count} exceeds max_categories limit {}",
            limits.max_categories
        ));
    }
    for label in x_categories
        .labels
        .iter()
        .chain(&y_categories.labels)
        .chain(&color_categories)
    {
        if label.len() > limits.max_label_bytes {
            return Err(format!(
                "rule mark category label length {} exceeds max_label_bytes limit {}",
                label.len(),
                limits.max_label_bytes
            ));
        }
    }
    let theme = make_theme(top)?;
    let grid = super::temporal_axis_grid(top, theme.grid_color, theme.text_color)?;
    let x_axis = make_axis(x_field.as_deref(), x_kind, grid.clone());
    let y_axis = make_axis(y_field.as_deref(), y_kind, grid);
    let series = if color_field.is_some() {
        color_domain
            .iter()
            .enumerate()
            .map(|(index, label)| make_series(label.clone(), palette_color(index)))
            .collect()
    } else {
        Vec::new()
    };
    let title = parse_title(top.get("title"));
    let width = dimension(top.get("width"), "width")?.unwrap_or(800.0);
    let height = dimension(top.get("height"), "height")?.unwrap_or(450.0);
    let rule_x_positions = x_positions(x_kind, &segments, true);
    let rule_y_positions = x_positions(y_kind, &segments, false);
    let data = VegaRuleData {
        x_categories: x_categories.labels,
        y_categories: y_categories.labels,
        segments,
        stroke_width,
        stroke_dash,
        clip,
    };

    Ok(ChartSpec {
        kind: ChartKind::VegaRule(Box::new(data)),
        series,
        categories: Vec::new(),
        x_positions: rule_x_positions,
        y_positions: rule_y_positions,
        x_axis,
        y_axis,
        legend: if color_field.is_some() {
            LegendPos::Top
        } else {
            LegendPos::None
        },
        legend_options: LegendOptions::default(),
        legend_title: color_field,
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
    let mark_name = super::read_mark_name(top);
    if mark_name != Some("rule") {
        return Err("rule mark must be \"rule\" or an object with type \"rule\"".into());
    }
    if let Some(mark) = top.get("mark").and_then(Value::as_object) {
        super::check_object(
            mark,
            &[
                "type",
                "color",
                "opacity",
                "strokeWidth",
                "strokeDash",
                "clip",
            ],
            "mark",
        )?;
    }
    if let Some(data) = top.get("data").and_then(Value::as_object) {
        super::check_object(data, &["values"], "data")?;
    }
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "rule mark requires an encoding object".to_string())?;
    super::check_object(encoding, &["x", "y", "x2", "y2", "color"], "encoding")?;
    for key in ["x", "y", "x2", "y2"] {
        if let Some(channel) = encoding.get(key).and_then(Value::as_object) {
            super::check_object(channel, &["field", "type"], &format!("encoding.{key}"))?;
        }
    }
    if let Some(color) = encoding.get("color").and_then(Value::as_object) {
        super::check_object(color, &["field", "type", "value"], "encoding.color")?;
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
        .map_err(|error| format!("invalid rule mark spec: {error}"))
}

fn channel_present(encoding: &Map<String, Value>, key: &str) -> bool {
    encoding.get(key).is_some_and(|value| !value.is_null())
}

fn position_kind(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    channel: &str,
    field: &str,
) -> Result<PositionKind, String> {
    let explicit = super::channel_type(encoding, channel);
    let kind = match explicit {
        Some("nominal" | "ordinal") => PositionKind::Category,
        Some("quantitative") => PositionKind::Quantitative,
        Some("temporal") => PositionKind::Temporal,
        Some(value) => return Err(format!("encoding.{channel}.type {value:?} is unsupported")),
        None => records
            .iter()
            .filter_map(|record| record.get(field).filter(|value| !value.is_null()))
            .find_map(|value| match value {
                Value::Number(_) => Some(PositionKind::Quantitative),
                Value::String(_) => Some(PositionKind::Category),
                Value::Bool(_) => Some(PositionKind::Category),
                _ => None,
            })
            .ok_or_else(|| format!("field {field} has no inferable values"))?,
    };
    Ok(kind)
}

fn validate_endpoint_kind(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    channel: &str,
    field: &str,
    expected: PositionKind,
) -> Result<(), String> {
    if super::channel_type(encoding, channel).is_some() {
        let actual = position_kind(records, encoding, channel, field)?;
        if actual != expected {
            return Err(format!(
                "encoding.{channel}.type must match its primary axis"
            ));
        }
    }
    Ok(())
}

fn position(
    record: &Map<String, Value>,
    field: &str,
    kind: PositionKind,
    domain: &mut CategoryDomain,
    index: usize,
    channel: &str,
) -> Result<VegaRulePosition, String> {
    let value = record
        .get(field)
        .filter(|value| !value.is_null())
        .ok_or_else(|| format!("rule data.values[{index}].{field} is missing or null"))?;
    match kind {
        PositionKind::Category => {
            if !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
                return Err(format!(
                    "rule data.values[{index}].{field} must be a category value for {channel}"
                ));
            }
            Ok(VegaRulePosition::Category(domain.index(
                value,
                super::field_category(record, Some(field)),
                index,
                channel,
            )?))
        }
        PositionKind::Quantitative => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(VegaRulePosition::Quantitative)
            .ok_or_else(|| format!("rule data.values[{index}].{field} must be a finite number")),
        PositionKind::Temporal => {
            let raw = value.as_str().ok_or_else(|| {
                format!("rule data.values[{index}].{field} must be an RFC 3339 timestamp")
            })?;
            super::parse_rfc3339_millis(field, raw)
                .map(VegaRulePosition::Temporal)
                .map_err(|error| format!("rule data.values[{index}].{channel}: {error}"))
        }
    }
}

fn parse_color_channel(
    encoding: &Map<String, Value>,
) -> Result<(Option<String>, Option<String>), String> {
    let Some(channel) = encoding
        .get("color")
        .filter(|value| !value.is_null())
        .and_then(Value::as_object)
    else {
        return Ok((None, None));
    };
    let field = channel
        .get("field")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let value = channel
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if field.is_some() == value.is_some() {
        return Err("encoding.color must specify exactly one of field or value".into());
    }
    if field.is_some()
        && super::channel_type(encoding, "color")
            .is_some_and(|kind| kind != "nominal" && kind != "ordinal")
    {
        return Err("encoding.color.type must be nominal or ordinal".into());
    }
    Ok((field, value))
}

fn parse_color_value(value: &Value, path: &str) -> Result<Color, String> {
    value
        .as_str()
        .and_then(parse_color)
        .ok_or_else(|| format!("rule mark {path} must be a valid color"))
}

fn parse_stroke_dash(mark: Option<&Map<String, Value>>) -> Result<Vec<f64>, String> {
    let Some(value) = mark
        .and_then(|mark| mark.get("strokeDash"))
        .filter(|value| !value.is_null())
    else {
        return Ok(Vec::new());
    };
    let pattern = value
        .as_array()
        .ok_or_else(|| "mark.strokeDash must be an array of non-negative numbers".to_string())?;
    if pattern.len() > crate::guard::MAX_BORDER_DASH_ELEMENTS {
        return Err(format!(
            "mark.strokeDash must contain at most {} entries",
            crate::guard::MAX_BORDER_DASH_ELEMENTS
        ));
    }
    pattern
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|dash| dash.is_finite() && *dash >= 0.0)
                .ok_or_else(|| {
                    String::from("mark.strokeDash entries must be finite non-negative numbers")
                })
        })
        .collect()
}

fn preflight_stroke_dash_length(mark: Option<&Value>) -> Result<(), String> {
    let Some(pattern) = mark
        .and_then(Value::as_object)
        .and_then(|mark| mark.get("strokeDash"))
        .filter(|value| !value.is_null())
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    if pattern.len() > crate::guard::MAX_BORDER_DASH_ELEMENTS {
        return Err(format!(
            "mark.strokeDash must contain at most {} entries",
            crate::guard::MAX_BORDER_DASH_ELEMENTS
        ));
    }
    Ok(())
}

fn palette_color(index: usize) -> Color {
    VEGALITE_PALETTE[index % VEGALITE_PALETTE.len()]
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

fn make_axis(field: Option<&str>, kind: Option<PositionKind>, grid: AxisGrid) -> AxisSpec {
    let temporal = kind == Some(PositionKind::Temporal);
    AxisSpec {
        title: field.map(|field| AxisTitle {
            text: field.to_owned(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        }),
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

fn x_positions(
    kind: Option<PositionKind>,
    segments: &[VegaRuleSegment],
    x_axis: bool,
) -> XPositions {
    if kind != Some(PositionKind::Temporal) {
        return XPositions::Category;
    }
    let mut unix_millis = segments
        .iter()
        .flat_map(|segment| {
            if x_axis {
                [segment.x1, segment.x2]
            } else {
                [segment.y1, segment.y2]
            }
        })
        .filter_map(|position| match position {
            VegaRulePosition::Temporal(millis) => Some(millis),
            _ => None,
        })
        .collect::<Vec<_>>();
    unix_millis.sort_unstable();
    unix_millis.dedup();
    XPositions::Temporal { unix_millis }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_color_domain_is_limited_before_series_are_built() {
        let mut top = serde_json::json!({
            "mark": "rule",
            "data": {"values": [{"x": 1, "site": "local"}]},
            "encoding": {
                "x": {"field": "x", "type": "quantitative"},
                "color": {"field": "site", "type": "nominal"}
            }
        });
        let limits = crate::guard::InputLimits {
            max_series: 1,
            ..crate::guard::InputLimits::default()
        };
        let overrides = crate::frontend::vegalite_composition::VegaUnitScaleOverrides {
            color_categories: Some(vec!["one".into(), "two".into()]),
            ..Default::default()
        };

        let error = parse_rule_spec(
            top.as_object_mut().expect("spec object"),
            &limits,
            &overrides,
        )
        .expect_err("the full shared color domain must honor max_series");

        assert!(error.contains("series count 2"), "{error}");
    }
}
