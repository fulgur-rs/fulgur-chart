//! Parser for Vega-Lite's composite error marks.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeasureAxis {
    X,
    Y,
}

impl MeasureAxis {
    fn other(self) -> Self {
        match self {
            Self::X => Self::Y,
            Self::Y => Self::X,
        }
    }

    fn orient(self) -> ErrorMarkOrient {
        match self {
            Self::X => ErrorMarkOrient::Horizontal,
            Self::Y => ErrorMarkOrient::Vertical,
        }
    }

    fn channel(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }

    fn end_channel(self) -> &'static str {
        match self {
            Self::X => "x2",
            Self::Y => "y2",
        }
    }

    fn error_channel(self) -> &'static str {
        match self {
            Self::X => "xError",
            Self::Y => "yError",
        }
    }

    fn error2_channel(self) -> &'static str {
        match self {
            Self::X => "xError2",
            Self::Y => "yError2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChannelKind {
    Category,
    Quantitative,
    Temporal,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum PositionKey {
    FullAxis,
    Category(usize),
    Quantitative(u64),
    Temporal(i64),
}

impl PositionKey {
    fn from(position: ErrorPosition) -> Self {
        match position {
            ErrorPosition::FullAxis => Self::FullAxis,
            ErrorPosition::Category(index) => Self::Category(index),
            ErrorPosition::Quantitative(value) => Self::Quantitative(if value == 0.0 {
                0.0_f64.to_bits()
            } else {
                value.to_bits()
            }),
            ErrorPosition::Temporal(millis) => Self::Temporal(millis),
        }
    }

    fn position(self) -> ErrorPosition {
        match self {
            Self::FullAxis => ErrorPosition::FullAxis,
            Self::Category(index) => ErrorPosition::Category(index),
            Self::Quantitative(bits) => ErrorPosition::Quantitative(f64::from_bits(bits)),
            Self::Temporal(millis) => ErrorPosition::Temporal(millis),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct GroupKey {
    position: PositionKey,
    color: Option<String>,
    detail: Option<String>,
}

#[derive(Default)]
struct PositionDomain {
    categories: Vec<String>,
    category_indexes: HashMap<String, usize>,
    temporal_labels: Vec<String>,
    temporal_values: Vec<i64>,
    temporal_indexes: HashMap<i64, usize>,
}

impl PositionDomain {
    fn parse(
        &mut self,
        record: &Map<String, Value>,
        field: Option<&str>,
        kind: Option<ChannelKind>,
    ) -> Result<ErrorPosition, String> {
        let Some(field) = field else {
            return Ok(ErrorPosition::FullAxis);
        };
        let value = record
            .get(field)
            .filter(|value| !value.is_null())
            .ok_or_else(|| format!("field {field} is missing or null"))?;
        match kind.unwrap_or(ChannelKind::Category) {
            ChannelKind::Category => {
                let label = match value {
                    Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                        field_category(record, Some(field))
                    }
                    _ => return Err(format!("field {field} must be a category value")),
                };
                let index = if let Some(index) = self.category_indexes.get(&label) {
                    *index
                } else {
                    let index = self.categories.len();
                    self.categories.push(label.clone());
                    self.category_indexes.insert(label, index);
                    index
                };
                Ok(ErrorPosition::Category(index))
            }
            ChannelKind::Quantitative => {
                let number = value
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| format!("field {field} must be a finite number"))?;
                Ok(ErrorPosition::Quantitative(number))
            }
            ChannelKind::Temporal => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| format!("field {field} must be a timestamp string"))?;
                let millis = parse_rfc3339_millis(field, raw)?;
                if !self.temporal_indexes.contains_key(&millis) {
                    let index = self.temporal_values.len();
                    self.temporal_values.push(millis);
                    self.temporal_labels.push(raw.to_owned());
                    self.temporal_indexes.insert(millis, index);
                }
                Ok(ErrorPosition::Temporal(millis))
            }
        }
    }

    fn chart_positions(
        &self,
        measure_axis: MeasureAxis,
        independent_kind: Option<ChannelKind>,
    ) -> (Vec<String>, XPositions, XPositions) {
        let position = if independent_kind == Some(ChannelKind::Temporal) {
            XPositions::Temporal {
                unix_millis: self.temporal_values.clone(),
            }
        } else {
            XPositions::Category
        };
        let labels = match independent_kind {
            Some(ChannelKind::Category) => self.categories.clone(),
            Some(ChannelKind::Temporal) => self.temporal_labels.clone(),
            Some(ChannelKind::Quantitative) | None => Vec::new(),
        };
        match measure_axis.other() {
            MeasureAxis::X => (labels, position, XPositions::Category),
            MeasureAxis::Y => (labels, XPositions::Category, position),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Raw,
    LowerUpper(MeasureAxis),
    CenterError(MeasureAxis),
}

fn mark_value<'a>(mark: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a Value> {
    mark.and_then(|mark| mark.get(key))
        .filter(|value| !value.is_null())
}

fn channel_is_present(encoding: &Map<String, Value>, key: &str) -> bool {
    encoding.get(key).is_some_and(|value| !value.is_null())
}

struct RangeGrouping<'a> {
    independent_field: Option<&'a str>,
    independent_kind: Option<ChannelKind>,
    color_field: Option<&'a str>,
    detail_field: Option<&'a str>,
}

pub(super) fn check_unknown_keys(top: &Map<String, Value>) -> Result<(), String> {
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
    if let Some(data) = top.get("data").and_then(Value::as_object) {
        super::check_object(data, &["values"], "data")?;
    }
    let mark_name = super::read_mark_name(top);
    if let Some(mark) = top.get("mark").and_then(Value::as_object) {
        let allowed: &[&str] = match mark_name {
            Some("errorbar") => &[
                "type", "extent", "orient", "color", "opacity", "clip", "rule", "ticks",
            ],
            Some("errorband") => &[
                "type",
                "extent",
                "orient",
                "color",
                "opacity",
                "clip",
                "band",
                "borders",
                "interpolate",
                "tension",
            ],
            _ => &["type"],
        };
        super::check_object(mark, allowed, "mark")?;
        for name in ["rule", "ticks", "band", "borders"] {
            if let Some(style) = mark.get(name).and_then(Value::as_object) {
                let allowed: &[&str] = if matches!(name, "rule" | "ticks") {
                    &[
                        "color",
                        "fill",
                        "stroke",
                        "strokeWidth",
                        "opacity",
                        "size",
                        "strokeDash",
                    ]
                } else {
                    &[
                        "color",
                        "fill",
                        "stroke",
                        "strokeWidth",
                        "opacity",
                        "strokeDash",
                    ]
                };
                super::check_object(style, allowed, &format!("mark.{name}"))?;
            }
        }
    }
    if let Some(encoding) = top.get("encoding").and_then(Value::as_object) {
        super::check_object(
            encoding,
            &[
                "x", "y", "x2", "y2", "xError", "xError2", "yError", "yError2", "color", "detail",
                "opacity",
            ],
            "encoding",
        )?;
        for name in [
            "x", "y", "x2", "y2", "xError", "xError2", "yError", "yError2", "color", "detail",
            "opacity",
        ] {
            if let Some(channel) = encoding.get(name).and_then(Value::as_object) {
                let allowed: &[&str] = match name {
                    "color" => &["field", "type", "value"],
                    "opacity" => &["value"],
                    _ => &["field", "type"],
                };
                super::check_object(channel, allowed, &format!("encoding.{name}"))?;
            }
        }
    }
    Ok(())
}

pub(super) fn parse_error_mark_spec(
    top: &mut Map<String, Value>,
    limits: &crate::guard::InputLimits,
) -> Result<ChartSpec, String> {
    check_unknown_keys(top)?;
    let input_values = top
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("values"))
        .and_then(Value::as_array)
        .ok_or_else(|| "data.values (inline array) is required for error marks".to_string())?;
    if input_values.len() > limits.max_total_data_points {
        return Err(format!(
            "error mark data point count {} exceeds max_total_data_points limit {} (pre-allocation)",
            input_values.len(),
            limits.max_total_data_points
        ));
    }
    validate_schema(top)?;
    let data = top
        .remove("data")
        .ok_or_else(|| "data must be an object for error marks".to_string())?;
    let values = match data {
        Value::Object(mut data) => match data.remove("values") {
            Some(Value::Array(values)) => values,
            _ => return Err("data.values must be an array for error marks".into()),
        },
        _ => return Err("data must be an object for error marks".into()),
    };
    let records = values
        .into_iter()
        .map(|value| match value {
            Value::Object(record) => Ok(record),
            _ => Err("data.values entries must be objects for error marks".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mark_name = read_mark_name(top).ok_or_else(|| "mark.type is required".to_string())?;
    let kind = match mark_name {
        "errorbar" => ErrorMarkKind::ErrorBar,
        "errorband" => ErrorMarkKind::ErrorBand,
        _ => return Err(format!("unsupported error mark {mark_name}")),
    };
    let mark = top.get("mark").and_then(Value::as_object);
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "encoding is required for error marks".to_string())?;
    if records.is_empty() {
        return Err("error mark data.values must contain at least one record".to_string());
    }

    let x_field = channel_field(encoding, "x");
    let y_field = channel_field(encoding, "y");
    let extent_value = mark_value(mark, "extent");
    let has_x2 = channel_is_present(encoding, "x2");
    let has_y2 = channel_is_present(encoding, "y2");
    let has_x_error = channel_is_present(encoding, "xError");
    let has_x_error2 = channel_is_present(encoding, "xError2");
    let has_y_error = channel_is_present(encoding, "yError");
    let has_y_error2 = channel_is_present(encoding, "yError2");
    let has_range_channels =
        has_x2 || has_y2 || has_x_error || has_x_error2 || has_y_error || has_y_error2;
    let mode = if !has_range_channels {
        InputMode::Raw
    } else {
        if extent_value.is_some() {
            return Err("mark.extent cannot be combined with pre-aggregated error channels".into());
        }
        if has_x2 && has_y2 {
            return Err("error marks may define a range on only one measure axis".into());
        }
        if (has_x2 || has_y2) && (has_x_error || has_x_error2 || has_y_error || has_y_error2) {
            return Err("lower/upper channels cannot be mixed with center/error channels".into());
        }
        if has_x_error2 && !has_x_error || has_y_error2 && !has_y_error {
            return Err("error2 requires the matching error channel".into());
        }
        if has_x_error || has_x_error2 {
            if has_y_error || has_y_error2 {
                return Err("error marks may define errors on only one measure axis".into());
            }
            InputMode::CenterError(MeasureAxis::X)
        } else if has_y_error || has_y_error2 {
            InputMode::CenterError(MeasureAxis::Y)
        } else if has_x2 {
            InputMode::LowerUpper(MeasureAxis::X)
        } else if has_y2 {
            InputMode::LowerUpper(MeasureAxis::Y)
        } else {
            return Err("error mark range channels are incomplete".into());
        }
    };
    let explicit_orient = parse_orient(mark)?;
    let measure_axis = match mode {
        InputMode::Raw => infer_measure_axis(
            &records,
            encoding,
            x_field.as_deref(),
            y_field.as_deref(),
            explicit_orient,
        )?,
        InputMode::LowerUpper(axis) | InputMode::CenterError(axis) => {
            if explicit_orient.is_some_and(|orient| orient != axis) {
                return Err("mark.orient conflicts with the error range axis".into());
            }
            axis
        }
    };
    let measure_field = match measure_axis {
        MeasureAxis::X => x_field.as_deref(),
        MeasureAxis::Y => y_field.as_deref(),
    }
    .ok_or_else(|| {
        format!(
            "encoding.{}.field is required for the measured axis",
            measure_axis.channel()
        )
    })?;
    if channel_type(encoding, measure_axis.channel()).is_some_and(|ty| ty != "quantitative") {
        return Err(format!(
            "encoding.{}.type must be quantitative",
            measure_axis.channel()
        ));
    }
    validate_numeric_finite(&records, measure_field)?;
    if let InputMode::LowerUpper(axis) = mode {
        let end_field = channel_field(encoding, axis.end_channel())
            .ok_or_else(|| format!("encoding.{}.field is required", axis.end_channel()))?;
        validate_numeric_finite(&records, &end_field)?;
    }
    if let InputMode::CenterError(axis) = mode {
        if channel_field(encoding, axis.error_channel()).is_none() {
            return Err(format!(
                "encoding.{}.field is required",
                axis.error_channel()
            ));
        }
        for channel in [axis.error_channel(), axis.error2_channel()] {
            if let Some(field) = channel_field(encoding, channel) {
                validate_numeric_finite(&records, &field)?;
            }
        }
    }

    let independent_axis = measure_axis.other();
    let independent_field = match independent_axis {
        MeasureAxis::X => x_field.as_deref(),
        MeasureAxis::Y => y_field.as_deref(),
    };
    let independent_kind = independent_field
        .map(|field| channel_kind(&records, encoding, independent_axis, field))
        .transpose()?;
    if let (Some(field), Some(channel_kind)) = (independent_field, independent_kind) {
        validate_independent_field(&records, field, channel_kind)?;
    }

    let (color_field, color_value) = parse_color_channel(encoding)?;
    if let Some(field) = color_field.as_deref() {
        validate_category(&records, field)?;
    }
    let detail_field = channel_field(encoding, "detail");
    if channel_type(encoding, "detail").is_some_and(|ty| ty != "nominal" && ty != "ordinal") {
        return Err("encoding.detail.type must be nominal or ordinal".into());
    }
    if let Some(field) = detail_field.as_deref() {
        validate_category(&records, field)?;
    }

    let extent = parse_extent(extent_value)?;
    if mode == InputMode::Raw && extent == super::super::vegalite_error::ErrorExtent::Ci {
        // Each record contributes one sample to exactly one group, so this preflights the
        // total work across every color/detail/position group before bootstrap allocation.
        super::super::vegalite_error::validate_bootstrap_budget(records.len())?;
    }
    let mut domain = PositionDomain::default();
    let grouping = RangeGrouping {
        independent_field,
        independent_kind,
        color_field: color_field.as_deref(),
        detail_field: detail_field.as_deref(),
    };
    let ranges = match mode {
        InputMode::Raw => {
            parse_raw_ranges(&records, measure_field, &grouping, extent, &mut domain)?
        }
        InputMode::LowerUpper(axis) => {
            parse_lower_upper_ranges(&records, encoding, axis, &grouping, &mut domain)?
        }
        InputMode::CenterError(axis) => {
            parse_center_error_ranges(&records, encoding, axis, &grouping, &mut domain)?
        }
    };
    if ranges.is_empty() {
        return Err("error mark data has no ranges".into());
    }
    if kind == ErrorMarkKind::ErrorBand {
        if independent_field.is_none()
            && (mark_value(mark, "interpolate").is_some() || mark_value(mark, "tension").is_some())
        {
            return Err(
                "interpolate and tension are unsupported for one-dimensional errorbands".into(),
            );
        }
        if mode != InputMode::Raw {
            validate_unique_band_positions(&ranges)?;
        }
    }

    let color_names = color_field
        .as_deref()
        .map(|field| distinct_categories(&records, Some(field)))
        .unwrap_or_else(|| vec![String::new()]);
    if color_names.len() > limits.max_series {
        return Err(format!(
            "error mark series count {} exceeds max_series limit {}",
            color_names.len(),
            limits.max_series
        ));
    }
    if domain.categories.len() > limits.max_categories {
        return Err(format!(
            "error mark category count {} exceeds max_categories limit {}",
            domain.categories.len(),
            limits.max_categories
        ));
    }
    if ranges.len() > limits.max_categorical_primitives {
        return Err(format!(
            "error mark range count {} exceeds max_categorical_primitives limit {}",
            ranges.len(),
            limits.max_categorical_primitives
        ));
    }

    let mark_color = parse_mark_color(mark)?;
    let series_colors = if color_field.is_some() {
        (0..color_names.len())
            .map(|index| palette_pick(VEGALITE_PALETTE, index))
            .collect::<Vec<_>>()
    } else {
        vec![color_value.unwrap_or(mark_color)]
    };
    let mut series = color_names
        .iter()
        .zip(&series_colors)
        .map(|(name, color)| make_series(name.clone(), *color))
        .collect::<Vec<_>>();
    for range in &ranges {
        if let Some(series) = series.get_mut(range.series_index) {
            series.values.push(range.center);
        }
    }

    let opacity = parse_mark_opacity(mark)? * parse_encoding_opacity(encoding)?.unwrap_or(1.0);
    let style = parse_style(kind, mark, opacity)?;
    let mut theme = vegalite_theme();
    if let Some(background) = top.get("background").filter(|value| !value.is_null()) {
        theme.background = Some(
            background
                .as_str()
                .and_then(parse_color)
                .ok_or_else(|| "background must be a valid color".to_string())?,
        );
    }

    let (categories, x_positions, y_positions) =
        domain.chart_positions(measure_axis, independent_kind);
    let configured_grid = super::temporal_axis_grid(top, theme.grid_color, theme.text_color)?;
    let grid = AxisGrid {
        display: configured_grid.display,
        color: configured_grid.color.or(Some(theme.grid_color)),
        ..AxisGrid::default()
    };
    let x_axis = make_axis(
        x_field.as_deref(),
        channel_kind_optional(&records, encoding, MeasureAxis::X, x_field.as_deref())?,
        grid.clone(),
    );
    let y_axis = make_axis(
        y_field.as_deref(),
        channel_kind_optional(&records, encoding, MeasureAxis::Y, y_field.as_deref())?,
        grid,
    );
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
    let title = match top.get("title") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    };
    let data = ErrorMarkData {
        kind,
        orient: measure_axis.orient(),
        ranges,
        style,
    };
    Ok(ChartSpec {
        kind: ChartKind::ErrorMark(Box::new(data)),
        series,
        categories,
        x_positions,
        y_positions,
        x_axis,
        y_axis,
        legend: if color_field.is_some() {
            LegendPos::Top
        } else {
            LegendPos::None
        },
        legend_options: crate::ir::LegendOptions::default(),
        legend_title: color_field,
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

fn validate_schema(top: &Map<String, Value>) -> Result<(), String> {
    // Validate the typed schema with empty inline data so a large data.values array is not
    // cloned just to check mark/channel/style definitions. The actual records are moved below.
    let mut validation_top = top
        .iter()
        .filter(|(key, _)| key.as_str() != "data")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<String, Value>>();
    validation_top.insert("data".into(), serde_json::json!({"values": []}));
    serde_json::from_value::<crate::schema::vegalite::VegaLiteSpec>(Value::Object(validation_top))
        .map(|_| ())
        .map_err(|error| format!("invalid error mark spec: {error}"))
}

fn parse_orient(mark: Option<&Map<String, Value>>) -> Result<Option<MeasureAxis>, String> {
    match mark_value(mark, "orient").and_then(Value::as_str) {
        Some("horizontal") => Ok(Some(MeasureAxis::X)),
        Some("vertical") => Ok(Some(MeasureAxis::Y)),
        None => Ok(None),
        Some(value) => Err(format!("unsupported error mark orient {value:?}")),
    }
}

fn infer_measure_axis(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    x_field: Option<&str>,
    y_field: Option<&str>,
    orient: Option<MeasureAxis>,
) -> Result<MeasureAxis, String> {
    if let Some(axis) = orient {
        let field = match axis {
            MeasureAxis::X => x_field,
            MeasureAxis::Y => y_field,
        }
        .ok_or_else(|| {
            format!(
                "encoding.{}.field is required by mark.orient",
                axis.channel()
            )
        })?;
        if channel_kind(records, encoding, axis, field)? != ChannelKind::Quantitative {
            return Err(format!(
                "encoding.{}.field must be quantitative",
                axis.channel()
            ));
        }
        return Ok(axis);
    }
    let x_kind = x_field
        .map(|field| channel_kind(records, encoding, MeasureAxis::X, field))
        .transpose()?;
    let y_kind = y_field
        .map(|field| channel_kind(records, encoding, MeasureAxis::Y, field))
        .transpose()?;
    match (x_kind, y_kind) {
        (Some(ChannelKind::Quantitative), Some(ChannelKind::Quantitative)) => {
            Err("both x and y are quantitative; set mark.orient to select the measured axis".into())
        }
        (Some(ChannelKind::Quantitative), Some(_) | None) => Ok(MeasureAxis::X),
        (Some(_) | None, Some(ChannelKind::Quantitative)) => Ok(MeasureAxis::Y),
        (None, None) => Err("error marks require an x or y quantitative field".into()),
        _ => Err("error marks require one quantitative measured axis".into()),
    }
}

fn channel_kind(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    axis: MeasureAxis,
    field: &str,
) -> Result<ChannelKind, String> {
    if let Some(hint) = channel_type(encoding, axis.channel()) {
        return match hint {
            "quantitative" => Ok(ChannelKind::Quantitative),
            "temporal" => Ok(ChannelKind::Temporal),
            "nominal" | "ordinal" => Ok(ChannelKind::Category),
            _ => Err(format!("encoding.{}.type is unsupported", axis.channel())),
        };
    }
    let first = records
        .iter()
        .find_map(|record| record.get(field).filter(|value| !value.is_null()))
        .ok_or_else(|| format!("field {field} is missing or null"))?;
    match first {
        Value::Number(_) => Ok(ChannelKind::Quantitative),
        Value::String(_) | Value::Bool(_) => Ok(ChannelKind::Category),
        _ => Err(format!("field {field} has an unsupported value type")),
    }
}

fn channel_kind_optional(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    axis: MeasureAxis,
    field: Option<&str>,
) -> Result<Option<ChannelKind>, String> {
    field
        .map(|field| channel_kind(records, encoding, axis, field))
        .transpose()
}

fn validate_numeric_finite(records: &[Map<String, Value>], field: &str) -> Result<(), String> {
    for record in records {
        number(record, field)?;
    }
    Ok(())
}

fn validate_independent_field(
    records: &[Map<String, Value>],
    field: &str,
    kind: ChannelKind,
) -> Result<(), String> {
    let mut domain = PositionDomain::default();
    for record in records {
        domain.parse(record, Some(field), Some(kind))?;
    }
    Ok(())
}

fn parse_extent(
    value: Option<&Value>,
) -> Result<super::super::vegalite_error::ErrorExtent, String> {
    use super::super::vegalite_error::ErrorExtent;
    match value.and_then(Value::as_str).unwrap_or("stderr") {
        "stderr" => Ok(ErrorExtent::Stderr),
        "stdev" => Ok(ErrorExtent::Stdev),
        "ci" => Ok(ErrorExtent::Ci),
        "iqr" => Ok(ErrorExtent::Iqr),
        value => Err(format!("unsupported error mark extent {value:?}")),
    }
}

fn parse_color_channel(
    encoding: &Map<String, Value>,
) -> Result<(Option<String>, Option<Color>), String> {
    let Some(value) = encoding.get("color").filter(|value| !value.is_null()) else {
        return Ok((None, None));
    };
    let channel = value
        .as_object()
        .ok_or_else(|| "encoding.color must be an object".to_string())?;
    if let Some(field) = channel.get("field").and_then(Value::as_str) {
        if channel_type(encoding, "color") == Some("quantitative") {
            return Err("encoding.color.type must be nominal or ordinal".into());
        }
        return Ok((Some(field.to_owned()), None));
    }
    let color = channel
        .get("value")
        .and_then(Value::as_str)
        .and_then(parse_color)
        .ok_or_else(|| "encoding.color.value must be a valid color".to_string())?;
    Ok((None, Some(color)))
}

fn parse_encoding_opacity(encoding: &Map<String, Value>) -> Result<Option<f64>, String> {
    let Some(value) = encoding.get("opacity").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    value
        .as_object()
        .and_then(|channel| channel.get("value"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .map(Some)
        .ok_or_else(|| "encoding.opacity must define a value from 0 to 1".into())
}

fn parse_mark_color(mark: Option<&Map<String, Value>>) -> Result<Color, String> {
    match mark_value(mark, "color") {
        None => parse_color("#4682b4").ok_or_else(|| "invalid default error mark color".into()),
        Some(Value::String(value)) => {
            parse_color(value).ok_or_else(|| "mark.color must be a valid color".into())
        }
        Some(_) => Err("mark.color must be a color string".into()),
    }
}

fn parse_mark_opacity(mark: Option<&Map<String, Value>>) -> Result<f64, String> {
    let Some(value) = mark_value(mark, "opacity") else {
        return Ok(1.0);
    };
    value
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| "mark.opacity must be between 0 and 1".into())
}

fn parse_style(
    kind: ErrorMarkKind,
    mark: Option<&Map<String, Value>>,
    opacity: f64,
) -> Result<ErrorMarkStyle, String> {
    let clip = mark_value(mark, "clip")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let hidden = || ErrorPartStyle {
        visible: false,
        fill: None,
        stroke: None,
        stroke_width: None,
        opacity: None,
        size: None,
        stroke_dash: Vec::new(),
    };
    let (rule, ticks, band, borders) = match kind {
        ErrorMarkKind::ErrorBar => (
            parse_part(mark_value(mark, "rule"), true, None, true)?,
            parse_part(mark_value(mark, "ticks"), false, None, true)?,
            hidden(),
            hidden(),
        ),
        ErrorMarkKind::ErrorBand => (
            hidden(),
            hidden(),
            parse_part(mark_value(mark, "band"), true, Some(0.3), false)?,
            parse_part(mark_value(mark, "borders"), false, None, false)?,
        ),
    };
    let interpolation = match mark_value(mark, "interpolate")
        .and_then(Value::as_str)
        .unwrap_or("linear")
    {
        "linear" => ErrorBandInterpolation::Linear,
        "linear-closed" => ErrorBandInterpolation::LinearClosed,
        "step" => ErrorBandInterpolation::Step,
        "step-before" => ErrorBandInterpolation::StepBefore,
        "step-after" => ErrorBandInterpolation::StepAfter,
        "basis" => ErrorBandInterpolation::Basis,
        "basis-open" => ErrorBandInterpolation::BasisOpen,
        "basis-closed" => ErrorBandInterpolation::BasisClosed,
        "cardinal" => ErrorBandInterpolation::Cardinal,
        "cardinal-open" => ErrorBandInterpolation::CardinalOpen,
        "cardinal-closed" => ErrorBandInterpolation::CardinalClosed,
        "bundle" => ErrorBandInterpolation::Bundle,
        "monotone" => ErrorBandInterpolation::Monotone,
        value => return Err(format!("unsupported errorband interpolation {value:?}")),
    };
    let tension = mark_value(mark, "tension")
        .and_then(Value::as_f64)
        .unwrap_or(0.5);
    if !tension.is_finite() || !(0.0..=1.0).contains(&tension) {
        return Err("errorband tension must be between 0 and 1".into());
    }
    Ok(ErrorMarkStyle {
        opacity,
        clip,
        rule,
        ticks,
        band,
        borders,
        interpolation,
        tension,
    })
}

fn parse_part(
    value: Option<&Value>,
    default_visible: bool,
    default_opacity: Option<f64>,
    allow_size: bool,
) -> Result<ErrorPartStyle, String> {
    let empty = || ErrorPartStyle {
        visible: default_visible,
        fill: None,
        stroke: None,
        stroke_width: None,
        opacity: default_opacity,
        size: None,
        stroke_dash: Vec::new(),
    };
    let Some(value) = value else {
        return Ok(empty());
    };
    if let Some(visible) = value.as_bool() {
        return Ok(ErrorPartStyle { visible, ..empty() });
    }
    let style = value
        .as_object()
        .ok_or_else(|| "error mark part must be a boolean or style object".to_string())?;
    let color = |key: &str| -> Result<Option<Color>, String> {
        style
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .and_then(parse_color)
                    .ok_or_else(|| format!("error mark part {key} must be a valid color"))
            })
            .transpose()
    };
    let color_value = color("color")?;
    let fill = color("fill")?.or(color_value);
    let stroke = color("stroke")?.or(color_value);
    let stroke_width = optional_nonnegative(style, "strokeWidth")?;
    let opacity = optional_bounded(style, "opacity", 0.0, 1.0)?.or(default_opacity);
    let size = if allow_size {
        optional_nonnegative(style, "size")?
    } else {
        None
    };
    let stroke_dash = match style.get("strokeDash") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_f64()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| {
                        "error mark strokeDash values must be finite and nonnegative".to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("error mark strokeDash must be an array".into()),
    };
    Ok(ErrorPartStyle {
        visible: true,
        fill,
        stroke,
        stroke_width,
        opacity,
        size,
        stroke_dash,
    })
}

fn optional_nonnegative(object: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| format!("error mark {key} must be finite and nonnegative"))
        })
        .transpose()
}

fn optional_bounded(
    object: &Map<String, Value>,
    key: &str,
    min: f64,
    max: f64,
) -> Result<Option<f64>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && (min..=max).contains(value))
                .ok_or_else(|| format!("error mark {key} must be between {min} and {max}"))
        })
        .transpose()
}

fn parse_raw_ranges(
    records: &[Map<String, Value>],
    measure_field: &str,
    grouping: &RangeGrouping<'_>,
    extent: super::super::vegalite_error::ErrorExtent,
    domain: &mut PositionDomain,
) -> Result<Vec<ErrorRangePoint>, String> {
    let mut groups = Vec::<(GroupKey, Vec<f64>)>::new();
    let mut indexes = HashMap::<GroupKey, usize>::new();
    for record in records {
        let position = domain.parse(
            record,
            grouping.independent_field,
            grouping.independent_kind,
        )?;
        let color = grouping
            .color_field
            .map(|field| field_category(record, Some(field)));
        let detail = grouping
            .detail_field
            .map(|field| field_category(record, Some(field)));
        let key = GroupKey {
            position: PositionKey::from(position),
            color,
            detail,
        };
        let value = number(record, measure_field)?;
        let index = if let Some(index) = indexes.get(&key) {
            *index
        } else {
            let index = groups.len();
            groups.push((key.clone(), Vec::new()));
            indexes.insert(key, index);
            index
        };
        groups[index].1.push(value);
    }
    let color_indexes = color_indexes(records, grouping.color_field);
    groups
        .into_iter()
        .map(|(key, samples)| {
            let bytes = serde_json::to_vec(&(
                format!("{:?}", key.position),
                key.color.as_deref(),
                key.detail.as_deref(),
            ))
            .unwrap_or_default();
            let seed = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
            let summary = super::super::vegalite_error::summarize(&samples, extent, seed)?;
            let series_index = key
                .color
                .as_ref()
                .and_then(|color| color_indexes.get(color))
                .copied()
                .unwrap_or(0);
            Ok(ErrorRangePoint {
                series_index,
                detail: key.detail,
                position: key.position.position(),
                center: summary.center,
                lower: summary.lower,
                upper: summary.upper,
            })
        })
        .collect()
}

fn parse_lower_upper_ranges(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    axis: MeasureAxis,
    grouping: &RangeGrouping<'_>,
    domain: &mut PositionDomain,
) -> Result<Vec<ErrorRangePoint>, String> {
    let low_field = channel_field(encoding, axis.channel())
        .ok_or_else(|| format!("encoding.{}.field is required", axis.channel()))?;
    let high_field = channel_field(encoding, axis.end_channel())
        .ok_or_else(|| format!("encoding.{}.field is required", axis.end_channel()))?;
    let color_indexes = color_indexes(records, grouping.color_field);
    let mut ranges = Vec::with_capacity(records.len());
    for record in records {
        let position = domain.parse(
            record,
            grouping.independent_field,
            grouping.independent_kind,
        )?;
        let lower = number(record, &low_field)?;
        let upper = number(record, &high_field)?;
        let (center, lower, upper) = checked_endpoints(lower, upper)?;
        let color = grouping
            .color_field
            .map(|field| field_category(record, Some(field)));
        let detail = grouping
            .detail_field
            .map(|field| field_category(record, Some(field)));
        let series_index = color
            .as_ref()
            .and_then(|color| color_indexes.get(color))
            .copied()
            .unwrap_or(0);
        ranges.push(ErrorRangePoint {
            series_index,
            detail,
            position,
            center,
            lower,
            upper,
        });
    }
    Ok(ranges)
}

fn parse_center_error_ranges(
    records: &[Map<String, Value>],
    encoding: &Map<String, Value>,
    axis: MeasureAxis,
    grouping: &RangeGrouping<'_>,
    domain: &mut PositionDomain,
) -> Result<Vec<ErrorRangePoint>, String> {
    let center_field = channel_field(encoding, axis.channel())
        .ok_or_else(|| format!("encoding.{}.field is required", axis.channel()))?;
    let upper_field = channel_field(encoding, axis.error_channel())
        .ok_or_else(|| format!("encoding.{}.field is required", axis.error_channel()))?;
    let lower_field = channel_field(encoding, axis.error2_channel());
    let color_indexes = color_indexes(records, grouping.color_field);
    let mut ranges = Vec::with_capacity(records.len());
    for record in records {
        let position = domain.parse(
            record,
            grouping.independent_field,
            grouping.independent_kind,
        )?;
        let center = number(record, &center_field)?;
        let upper_offset = number(record, &upper_field)?;
        let lower_offset = match lower_field.as_deref() {
            Some(field) => number(record, field)?,
            None => -upper_offset,
        };
        if upper_offset < 0.0 || lower_offset > 0.0 {
            return Err("error offsets require error >= 0 and error2 <= 0".into());
        }
        let lower = center + lower_offset;
        let upper = center + upper_offset;
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err("error offsets produce non-finite or reversed endpoints".into());
        }
        let color = grouping
            .color_field
            .map(|field| field_category(record, Some(field)));
        let detail = grouping
            .detail_field
            .map(|field| field_category(record, Some(field)));
        let series_index = color
            .as_ref()
            .and_then(|color| color_indexes.get(color))
            .copied()
            .unwrap_or(0);
        ranges.push(ErrorRangePoint {
            series_index,
            detail,
            position,
            center,
            lower,
            upper,
        });
    }
    Ok(ranges)
}

fn checked_endpoints(lower: f64, upper: f64) -> Result<(f64, f64, f64), String> {
    if lower > upper {
        return Err("error mark lower endpoint exceeds upper endpoint".into());
    }
    let center = lower * 0.5 + upper * 0.5;
    if !center.is_finite() {
        return Err("error mark range center must be finite".into());
    }
    Ok((center, lower, upper))
}

fn number(record: &Map<String, Value>, field: &str) -> Result<f64, String> {
    record
        .get(field)
        .filter(|value| !value.is_null())
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("field {field} must be a finite number"))
}

fn color_indexes(records: &[Map<String, Value>], field: Option<&str>) -> HashMap<String, usize> {
    field
        .map(|field| {
            distinct_categories(records, Some(field))
                .into_iter()
                .enumerate()
                .map(|(index, value)| (value, index))
                .collect()
        })
        .unwrap_or_default()
}

fn validate_unique_band_positions(ranges: &[ErrorRangePoint]) -> Result<(), String> {
    let mut seen = HashSet::<(usize, Option<String>, PositionKey)>::new();
    for range in ranges {
        if !seen.insert((
            range.series_index,
            range.detail.clone(),
            PositionKey::from(range.position),
        )) {
            return Err("errorband has duplicate independent positions in one series".into());
        }
    }
    Ok(())
}

fn make_series(name: String, color: Color) -> Series {
    Series {
        name,
        values: Vec::new(),
        points: vec![],
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
        violin_samples: vec![],
        box_points: vec![],
        tree: vec![],
        links: vec![],
    }
}

fn make_axis(field: Option<&str>, kind: Option<ChannelKind>, grid: AxisGrid) -> AxisSpec {
    let temporal = kind == Some(ChannelKind::Temporal);
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
        ticks: AxisTickOptions::default(),
    }
}
