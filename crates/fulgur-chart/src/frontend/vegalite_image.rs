//! Parser for Vega-Lite image marks. The core retains image URLs for SVG and never fetches them.

use super::*;

const MAX_IMAGE_DIMENSION: f64 = 32_768.0;

pub(super) fn parse_image_spec(
    top: &Map<String, Value>,
    strict: bool,
    limits: &crate::guard::InputLimits,
) -> Result<ChartSpec, String> {
    if strict {
        check_unknown_keys(top)?;
    }

    let mark = top
        .get("mark")
        .ok_or_else(|| "image mark is required".to_string())?;
    let mark_object = mark
        .as_object()
        .ok_or_else(|| "image mark requires an object with width and height".to_string())?;
    let width = parse_mark_dimension(mark_object, "width", limits)?;
    let height = parse_mark_dimension(mark_object, "height", limits)?;

    let data = top
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "image mark requires inline data.values".to_string())?;
    if data.contains_key("url") {
        return Err("image mark data.url is unsupported; provide inline data.values".to_string());
    }
    let value_count = data
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "image mark requires inline data.values".to_string())?
        .len();
    if value_count > limits.max_total_data_points {
        return Err(format!(
            "image mark point count {value_count} exceeds max_total_data_points limit {}",
            limits.max_total_data_points
        ));
    }
    if value_count > limits.max_categorical_primitives {
        return Err(format!(
            "image mark count {value_count} exceeds max_categorical_primitives limit {}",
            limits.max_categorical_primitives
        ));
    }
    let records = parse_data_values(top.get("data"))?;

    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "image mark requires an encoding object".to_string())?;
    let x_field = parse_position_channel(encoding.get("x"), "x")?;
    let y_field = parse_position_channel(encoding.get("y"), "y")?;
    validate_numeric(&records, &x_field)?;
    validate_numeric(&records, &y_field)?;
    let points = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let x = record
                .get(&x_field)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("image mark data.values[{index}].{x_field} must be finite")
                })?;
            let y = record
                .get(&y_field)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("image mark data.values[{index}].{y_field} must be finite")
                })?;
            Ok(Point { x, y, r: None })
        })
        .collect::<Result<Vec<_>, String>>()?;

    if mark_object.contains_key("url") {
        return Err("mark.url is unsupported; use encoding.url.value instead".to_string());
    }
    let url_source = parse_url_source(encoding.get("url"))?;
    let urls = match url_source {
        UrlSource::Field(field) => records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let href = record.get(&field).and_then(Value::as_str).ok_or_else(|| {
                    format!("image mark data.values[{index}].{field} must be a URL string")
                })?;
                validate_image_url(href, limits.max_label_bytes)?;
                Ok(std::sync::Arc::<str>::from(href))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(VegaImageUrls::PerPoint)?,
        UrlSource::Value(href) => {
            validate_image_url(&href, limits.max_label_bytes)?;
            VegaImageUrls::Constant(std::sync::Arc::<str>::from(href))
        }
    };

    let mut theme = vegalite_theme();
    if let Some(background) = top.get("background").filter(|value| !value.is_null()) {
        let color = background
            .as_str()
            .and_then(parse_color)
            .ok_or_else(|| "background must be a valid color".to_string())?;
        theme.background = Some(color);
    }

    let width_canvas = chart_dimension(top.get("width"), 800.0);
    let height_canvas = chart_dimension(top.get("height"), 450.0);
    let title = match top.get("title") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    };
    let series = Series {
        name: String::new(),
        values: Vec::new(),
        points,
        fill: Vec::new(),
        stroke: Vec::new(),
        stroke_width: 0.0,
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
    };

    Ok(ChartSpec {
        kind: ChartKind::VegaImage(Box::new(VegaImageData {
            urls,
            width,
            height,
        })),
        series: vec![series],
        categories: Vec::new(),
        x_positions: XPositions::Category,
        y_positions: XPositions::Category,
        x_axis: image_axis_spec(),
        y_axis: image_axis_spec(),
        legend: LegendPos::None,
        legend_options: LegendOptions::default(),
        legend_title: None,
        title,
        chartjs_title: None,
        chartjs_subtitle: None,
        width: width_canvas,
        height: height_canvas,
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
        radial_axis: None,
    })
}

enum UrlSource {
    Field(String),
    Value(String),
}

fn parse_url_source(channel: Option<&Value>) -> Result<UrlSource, String> {
    let channel = channel
        .filter(|value| !value.is_null())
        .ok_or_else(|| "image mark requires encoding.url".to_string())?
        .as_object()
        .ok_or_else(|| "encoding.url must be an object".to_string())?;
    match (channel.get("field"), channel.get("value")) {
        (Some(Value::String(field)), None) if !field.is_empty() => {
            if let Some(field_type) = channel.get("type").filter(|value| !value.is_null())
                && field_type.as_str() != Some("nominal")
                && field_type.as_str() != Some("ordinal")
            {
                return Err("encoding.url.type must be nominal or ordinal".to_string());
            }
            Ok(UrlSource::Field(field.clone()))
        }
        (None, Some(Value::String(value))) => {
            if channel.contains_key("type") {
                return Err("encoding.url.type cannot be combined with encoding.url.value".into());
            }
            Ok(UrlSource::Value(value.clone()))
        }
        (Some(_), None) => Err("encoding.url.field must be a non-empty string".into()),
        (None, Some(_)) => Err("encoding.url.value must be a string".into()),
        (Some(_), Some(_)) => Err("encoding.url cannot combine field and value".into()),
        (None, None) => Err("encoding.url requires field or value".into()),
    }
}

fn parse_position_channel(channel: Option<&Value>, name: &str) -> Result<String, String> {
    let channel = channel
        .and_then(Value::as_object)
        .ok_or_else(|| format!("image mark encoding.{name} must be an object"))?;
    let field = channel
        .get("field")
        .and_then(Value::as_str)
        .filter(|field| !field.is_empty())
        .ok_or_else(|| format!("image mark encoding.{name}.field must be a non-empty string"))?;
    if let Some(field_type) = channel.get("type").filter(|value| !value.is_null())
        && field_type.as_str() != Some("quantitative")
    {
        return Err(format!(
            "image mark encoding.{name}.type must be quantitative"
        ));
    }
    Ok(field.to_owned())
}

fn parse_mark_dimension(
    mark: &Map<String, Value>,
    name: &str,
    limits: &crate::guard::InputLimits,
) -> Result<f64, String> {
    let value = mark
        .get(name)
        .and_then(Value::as_f64)
        .filter(|value| {
            value.is_finite()
                && *value > 0.0
                && *value <= MAX_IMAGE_DIMENSION
                && *value <= limits.max_dimension_px
        })
        .ok_or_else(|| {
            format!(
                "image mark width and height must be finite, positive, and at most {} pixels",
                MAX_IMAGE_DIMENSION.min(limits.max_dimension_px)
            )
        })?;
    Ok(value)
}

pub(super) fn validate_image_url(url: &str, max_bytes: usize) -> Result<(), String> {
    let invalid = || "image URL is invalid or uses an unsupported scheme".to_string();
    if url.is_empty()
        || url.len() > max_bytes
        || url.chars().any(char::is_control)
        || url.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(invalid());
    }
    let Some((scheme, remainder)) = url.split_once(':') else {
        return Err(invalid());
    };
    if scheme.eq_ignore_ascii_case("data") {
        let Some((metadata, payload)) = remainder.split_once(',') else {
            return Err(invalid());
        };
        let media_type = metadata.split(';').next().unwrap_or_default();
        let Some((media, subtype)) = media_type.split_once('/') else {
            return Err(invalid());
        };
        let valid_subtype = !subtype.is_empty()
            && subtype
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte));
        if !media.eq_ignore_ascii_case("image") || !valid_subtype || payload.is_empty() {
            return Err(invalid());
        }
        if !url
            .bytes()
            .enumerate()
            .all(|(index, byte)| valid_uri_byte(url.as_bytes(), index, byte))
        {
            return Err(invalid());
        }
        return Ok(());
    }
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return Err(invalid());
    }
    if !remainder.starts_with("//") {
        return Err(invalid());
    }
    // Keep URL validation self-contained so the WASM core does not pull in an IDNA database.
    let authority = remainder
        .strip_prefix("//")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or_default();
    if !valid_http_authority(authority)
        || !url
            .bytes()
            .enumerate()
            .all(|(index, byte)| valid_uri_byte(url.as_bytes(), index, byte))
    {
        return Err(invalid());
    }
    Ok(())
}

fn valid_http_authority(authority: &str) -> bool {
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((ipv6, tail)) = bracketed.split_once(']') else {
            return false;
        };
        if ipv6.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        let port = if tail.is_empty() {
            None
        } else if let Some(port) = tail.strip_prefix(':') {
            Some(port)
        } else {
            return false;
        };
        (ipv6, port)
    } else {
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            Some(_) => return false,
            None => (authority, None),
        };
        (host, port)
    };
    if host.is_empty() {
        return false;
    }
    if !host.contains(':') {
        let hostname = host.strip_suffix('.').unwrap_or(host);
        if hostname.is_empty()
            || hostname.split('.').any(|label| {
                label.is_empty()
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label.chars().all(|character| {
                        character.is_alphanumeric() || matches!(character, '-' | '_')
                    })
            })
        {
            return false;
        }
    }
    port.is_none_or(|port| !port.is_empty() && port.parse::<u16>().is_ok())
}

fn valid_uri_byte(bytes: &[u8], index: usize, byte: u8) -> bool {
    if byte == b'%' {
        return bytes
            .get(index + 1..index + 3)
            .is_some_and(|digits| digits.len() == 2 && digits.iter().all(u8::is_ascii_hexdigit));
    }
    !byte.is_ascii()
        || byte.is_ascii_alphanumeric()
        || b"-._~:/?#[]@!$&'()*+,;=".contains(&byte)
        || byte == b','
}

fn check_unknown_keys(top: &Map<String, Value>) -> Result<(), String> {
    check_keys(
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
        ],
        "",
    )?;
    if let Some(mark) = top.get("mark").and_then(Value::as_object) {
        check_keys(mark, &["type", "width", "height"], "mark")?;
    }
    if let Some(data) = top.get("data").and_then(Value::as_object) {
        check_keys(data, &["values", "url"], "data")?;
    }
    if let Some(encoding) = top.get("encoding").and_then(Value::as_object) {
        check_keys(encoding, &["x", "y", "url"], "encoding")?;
        for axis in ["x", "y"] {
            if let Some(channel) = encoding.get(axis).and_then(Value::as_object) {
                check_keys(channel, &["field", "type"], &format!("encoding.{axis}"))?;
            }
        }
        if let Some(channel) = encoding.get("url").and_then(Value::as_object) {
            check_keys(channel, &["field", "value", "type"], "encoding.url")?;
        }
    }
    serde_json::from_value::<crate::schema::vegalite::VegaLiteSpec>(Value::Object(top.clone()))
        .map_err(|error| format!("invalid Vega-Lite image spec: {error}"))?;
    Ok(())
}

fn check_keys(object: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        let path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        return Err(format!("unknown key: {path}"));
    }
    Ok(())
}

fn image_axis_spec() -> AxisSpec {
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

fn chart_dimension(value: Option<&Value>, default: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}
