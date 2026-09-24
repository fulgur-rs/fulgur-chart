use crate::ir::{AxisTickOptions, ScaleKind, TimeOptions, TimeUnit};
use std::fmt::Write as _;
use time::{
    Month, OffsetDateTime, PrimitiveDateTime, UtcOffset, Weekday,
    format_description::well_known::Rfc3339,
};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;
const MILLIS_PER_WEEK: i64 = 7 * MILLIS_PER_DAY;
const APPROX_MILLIS_PER_MONTH: i64 = 30 * MILLIS_PER_DAY;
const APPROX_MILLIS_PER_YEAR: i64 = 365 * MILLIS_PER_DAY;
const MAX_ERROR_FRAGMENT_BYTES: usize = 80;
const MAX_TEMPORAL_TICKS: usize = 1_000;
const MAX_TIME_FORMAT_BYTES: usize = 256;
const MAX_TIME_VALUE_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickInterval {
    unit: TickUnit,
    step: i32,
    approximate_millis: i64,
}

const TICK_INTERVALS: [TickInterval; 19] = [
    TickInterval::new(TickUnit::Second, 1, MILLIS_PER_SECOND),
    TickInterval::new(TickUnit::Second, 5, 5 * MILLIS_PER_SECOND),
    TickInterval::new(TickUnit::Second, 15, 15 * MILLIS_PER_SECOND),
    TickInterval::new(TickUnit::Second, 30, 30 * MILLIS_PER_SECOND),
    TickInterval::new(TickUnit::Minute, 1, MILLIS_PER_MINUTE),
    TickInterval::new(TickUnit::Minute, 5, 5 * MILLIS_PER_MINUTE),
    TickInterval::new(TickUnit::Minute, 15, 15 * MILLIS_PER_MINUTE),
    TickInterval::new(TickUnit::Minute, 30, 30 * MILLIS_PER_MINUTE),
    TickInterval::new(TickUnit::Hour, 1, MILLIS_PER_HOUR),
    TickInterval::new(TickUnit::Hour, 3, 3 * MILLIS_PER_HOUR),
    TickInterval::new(TickUnit::Hour, 6, 6 * MILLIS_PER_HOUR),
    TickInterval::new(TickUnit::Hour, 12, 12 * MILLIS_PER_HOUR),
    TickInterval::new(TickUnit::Day, 1, MILLIS_PER_DAY),
    TickInterval::new(TickUnit::Day, 2, 2 * MILLIS_PER_DAY),
    TickInterval::new(TickUnit::Week, 1, MILLIS_PER_WEEK),
    TickInterval::new(TickUnit::Month, 1, APPROX_MILLIS_PER_MONTH),
    TickInterval::new(TickUnit::Month, 3, 3 * APPROX_MILLIS_PER_MONTH),
    TickInterval::new(TickUnit::Quarter, 1, 3 * APPROX_MILLIS_PER_MONTH),
    TickInterval::new(TickUnit::Year, 1, APPROX_MILLIS_PER_YEAR),
];

impl TickInterval {
    const fn new(unit: TickUnit, step: i32, approximate_millis: i64) -> Self {
        Self {
            unit,
            step,
            approximate_millis,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalTick {
    pub unix_millis: i64,
    pub label: String,
}

/// Pixel projection for a `time` or `timeseries` axis.
#[derive(Clone, Debug)]
pub struct TemporalScale {
    kind: ScaleKind,
    values: Vec<i64>,
    min: i64,
    max: i64,
    pixel_start: f64,
    pixel_end: f64,
}

impl TemporalScale {
    pub fn new(kind: ScaleKind, values: &[i64], pixel_start: f64, pixel_end: f64) -> Self {
        let min = values.iter().copied().min().unwrap_or(0);
        let max = values.iter().copied().max().unwrap_or(min);
        Self::with_domain(kind, values, min, max, pixel_start, pixel_end)
    }

    pub fn with_domain(
        kind: ScaleKind,
        values: &[i64],
        min: i64,
        max: i64,
        pixel_start: f64,
        pixel_end: f64,
    ) -> Self {
        let mut values = values.to_vec();
        if kind == ScaleKind::Timeseries {
            values.push(min);
            values.push(max);
        }
        values.sort_unstable();
        values.dedup();
        Self {
            kind,
            values,
            min,
            max,
            pixel_start,
            pixel_end,
        }
    }

    pub fn map_millis(&self, value: i64) -> f64 {
        let ratio = if self.kind == ScaleKind::Timeseries && self.values.len() > 1 {
            let right = self.values.partition_point(|&timestamp| timestamp < value);
            if right == 0 {
                0.0
            } else if right >= self.values.len() {
                1.0
            } else if self.values[right] == value {
                right as f64 / (self.values.len() - 1) as f64
            } else {
                let left = right - 1;
                let span = i128::from(self.values[right]) - i128::from(self.values[left]);
                let elapsed = i128::from(value) - i128::from(self.values[left]);
                (left as f64 + elapsed as f64 / span as f64) / (self.values.len() - 1) as f64
            }
        } else if self.min == self.max {
            0.5
        } else {
            (i128::from(value) - i128::from(self.min)) as f64
                / (i128::from(self.max) - i128::from(self.min)) as f64
        };
        self.pixel_start + ratio * (self.pixel_end - self.pixel_start)
    }

    pub fn map_value(&self, value: f64) -> f64 {
        if !value.is_finite() || value.abs() > 8.64e15 {
            return f64::NAN;
        }
        self.map_millis(value.trunc() as i64)
    }

    pub fn unmap_pixel(&self, pixel: f64) -> f64 {
        let pixel_span = self.pixel_end - self.pixel_start;
        if pixel_span == 0.0 {
            return self.min as f64;
        }
        let ratio = (pixel - self.pixel_start) / pixel_span;
        if self.kind == ScaleKind::Timeseries && self.values.len() > 1 {
            let position = ratio * (self.values.len() - 1) as f64;
            let left = position.floor() as isize;
            let fraction = position - left as f64;
            let left_index = left.clamp(0, self.values.len() as isize - 2) as usize;
            let low = i128::from(self.values[left_index]);
            let high = i128::from(self.values[left_index + 1]);
            low as f64 + fraction * (high - low) as f64
        } else {
            self.min as f64 + ratio * (self.max as f64 - self.min as f64)
        }
    }
}

/// User-controlled field names and values must not make parse errors unbounded.
/// Truncation is byte-based and preserves UTF-8 boundaries.
pub(crate) fn bounded_error_fragment(raw: &str) -> String {
    if raw.len() <= MAX_ERROR_FRAGMENT_BYTES {
        return raw.to_owned();
    }
    let mut end = MAX_ERROR_FRAGMENT_BYTES;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &raw[..end])
}

pub fn parse_rfc3339_millis(field: &str, raw: &str) -> Result<i64, String> {
    let shown_field = bounded_error_fragment(field);
    if raw.len() > MAX_TIME_VALUE_BYTES {
        return Err(format!(
            "field {shown_field} timestamp exceeds {MAX_TIME_VALUE_BYTES} bytes"
        ));
    }
    let parsed = parse_iso8601_utc(raw).ok_or_else(|| {
        let shown = bounded_error_fragment(raw);
        format!("field {shown_field} contains invalid ISO 8601 timestamp: {shown:?}")
    })?;
    i64::try_from(parsed.unix_timestamp_nanos().div_euclid(1_000_000))
        .map_err(|_| format!("field {shown_field} timestamp is outside the supported range"))
}

/// Validate the supported strftime-style subset for parser or display formats.
pub fn validate_time_format(field: &str, format: &str, display: bool) -> Result<(), String> {
    if format.len() > MAX_TIME_FORMAT_BYTES {
        return Err(format!(
            "{field} format exceeds {MAX_TIME_FORMAT_BYTES} bytes"
        ));
    }
    let bytes = format.as_bytes();
    let mut index = 0;
    let mut has_year = false;
    let mut has_month = false;
    let mut has_day = false;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        index += 1;
        if index >= bytes.len() {
            return Err(format!("{field} format ends with an incomplete directive"));
        }
        let (directive, has_fraction_dot) = match bytes[index] {
            b'.' if bytes.get(index + 1) == Some(&b'f') => {
                index += 2;
                (b'f', true)
            }
            directive => {
                index += 1;
                (directive, false)
            }
        };
        let supported = (directive != b'f' || has_fraction_dot)
            && if display {
                matches!(
                    directive,
                    b'Y' | b'y'
                        | b'm'
                        | b'b'
                        | b'B'
                        | b'd'
                        | b'a'
                        | b'A'
                        | b'H'
                        | b'I'
                        | b'M'
                        | b'S'
                        | b'f'
                        | b'p'
                        | b'z'
                        | b'%'
                )
            } else {
                matches!(
                    directive,
                    b'Y' | b'm' | b'd' | b'H' | b'M' | b'S' | b'f' | b'z' | b'%'
                )
            };
        if !supported {
            return Err(format!("{field} format contains an unsupported directive"));
        }
        has_year |= directive == b'Y';
        has_month |= directive == b'm';
        has_day |= directive == b'd';
    }
    if !display && !(has_year && has_month && has_day) {
        return Err(format!("{field} parser format must include %Y, %m, and %d"));
    }
    Ok(())
}

/// Parse a timestamp string using the supported numeric strftime-style subset.
pub fn parse_custom_format_millis(field: &str, raw: &str, format: &str) -> Result<i64, String> {
    validate_time_format(field, format, false)?;
    let shown_field = bounded_error_fragment(field);
    if raw.len() > MAX_TIME_VALUE_BYTES {
        return Err(format!(
            "field {shown_field} timestamp exceeds {MAX_TIME_VALUE_BYTES} bytes"
        ));
    }

    let input = raw.as_bytes();
    let format_bytes = format.as_bytes();
    let mut input_index = 0;
    let mut format_index = 0;
    let (mut year, mut month, mut day) = (None, None, None);
    let (mut hour, mut minute, mut second) = (0_u32, 0_u32, 0_u32);
    let mut nanosecond = 0_u32;
    let mut offset = UtcOffset::UTC;
    while format_index < format_bytes.len() {
        if format_bytes[format_index] != b'%' {
            if input.get(input_index) != format_bytes.get(format_index) {
                return Err(invalid_custom_timestamp(&shown_field, raw));
            }
            input_index += 1;
            format_index += 1;
            continue;
        }
        format_index += 1;
        let directive = if format_bytes.get(format_index) == Some(&b'.') {
            if format_bytes.get(format_index + 1) != Some(&b'f') {
                return Err(format!(
                    "{shown_field} parser format contains an invalid directive"
                ));
            }
            format_index += 2;
            b'f'
        } else {
            let directive = *format_bytes.get(format_index).ok_or_else(|| {
                format!("{shown_field} parser format ends with an incomplete directive")
            })?;
            format_index += 1;
            directive
        };
        match directive {
            b'Y' => year = Some(read_fixed_digits(input, &mut input_index, 4)),
            b'm' => month = Some(read_fixed_digits(input, &mut input_index, 2)),
            b'd' => day = Some(read_fixed_digits(input, &mut input_index, 2)),
            b'H' => hour = read_fixed_digits(input, &mut input_index, 2).unwrap_or(u32::MAX),
            b'M' => minute = read_fixed_digits(input, &mut input_index, 2).unwrap_or(u32::MAX),
            b'S' => second = read_fixed_digits(input, &mut input_index, 2).unwrap_or(u32::MAX),
            b'f' => {
                if input.get(input_index) != Some(&b'.') {
                    return Err(invalid_custom_timestamp(&shown_field, raw));
                }
                input_index += 1;
                let start = input_index;
                while input.get(input_index).is_some_and(u8::is_ascii_digit) {
                    input_index += 1;
                }
                let digits = input.get(start..input_index).unwrap_or_default();
                if digits.is_empty() || digits.len() > 9 {
                    return Err(invalid_custom_timestamp(&shown_field, raw));
                }
                let fraction = std::str::from_utf8(digits)
                    .ok()
                    .and_then(|digits| digits.parse::<u32>().ok())
                    .unwrap_or(u32::MAX);
                nanosecond = fraction.saturating_mul(10_u32.pow((9 - digits.len()) as u32));
            }
            b'z' => match read_offset(input, &mut input_index) {
                Some(value) => offset = value,
                None => return Err(invalid_custom_timestamp(&shown_field, raw)),
            },
            b'%' => {
                if input.get(input_index) != Some(&b'%') {
                    return Err(invalid_custom_timestamp(&shown_field, raw));
                }
                input_index += 1;
            }
            _ => {
                return Err(format!(
                    "{shown_field} parser format contains an unsupported directive"
                ));
            }
        }
    }
    if input_index != input.len() {
        return Err(invalid_custom_timestamp(&shown_field, raw));
    }
    let (Some(year), Some(month), Some(day)) = (year.flatten(), month.flatten(), day.flatten())
    else {
        return Err(format!(
            "{shown_field} parser format must include %Y, %m, and %d"
        ));
    };
    let year = i32::try_from(year).ok();
    let month = u8::try_from(month)
        .ok()
        .and_then(|value| Month::try_from(value).ok());
    let day = u8::try_from(day).ok();
    let Some(year) = year else {
        return Err(invalid_custom_timestamp(&shown_field, raw));
    };
    let Some(month) = month else {
        return Err(invalid_custom_timestamp(&shown_field, raw));
    };
    let Some(day) = day else {
        return Err(invalid_custom_timestamp(&shown_field, raw));
    };
    let date = time::Date::from_calendar_date(year, month, day)
        .map_err(|_| invalid_custom_timestamp(&shown_field, raw))?;
    let time = time::Time::from_hms_nano(
        u8::try_from(hour).map_err(|_| invalid_custom_timestamp(&shown_field, raw))?,
        u8::try_from(minute).map_err(|_| invalid_custom_timestamp(&shown_field, raw))?,
        u8::try_from(second).map_err(|_| invalid_custom_timestamp(&shown_field, raw))?,
        nanosecond,
    )
    .map_err(|_| invalid_custom_timestamp(&shown_field, raw))?;
    let millis = PrimitiveDateTime::new(date, time)
        .assume_offset(offset)
        .unix_timestamp_nanos()
        .div_euclid(1_000_000);
    i64::try_from(millis)
        .map_err(|_| format!("field {shown_field} timestamp is outside the supported range"))
}

fn read_fixed_digits(input: &[u8], index: &mut usize, count: usize) -> Option<u32> {
    let end = index.checked_add(count)?;
    let digits = input.get(*index..end)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *index = end;
    std::str::from_utf8(digits).ok()?.parse().ok()
}

fn read_offset(input: &[u8], index: &mut usize) -> Option<UtcOffset> {
    if input.get(*index) == Some(&b'Z') {
        *index += 1;
        return Some(UtcOffset::UTC);
    }
    let sign = match input.get(*index)? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    *index += 1;
    let hours = i8::try_from(read_fixed_digits(input, index, 2)?).ok()?;
    if input.get(*index) == Some(&b':') {
        *index += 1;
    }
    let minutes = i8::try_from(read_fixed_digits(input, index, 2)?).ok()?;
    UtcOffset::from_hms(hours * sign, minutes * sign, 0).ok()
}

fn invalid_custom_timestamp(field: &str, raw: &str) -> String {
    format!(
        "field {field} does not match its configured time parser: {:?}",
        bounded_error_fragment(raw)
    )
}

/// Round a timestamp down to the requested UTC calendar boundary.
pub fn round_timestamp(unix_millis: i64, unit: TimeUnit) -> i64 {
    let fixed_period = match unit {
        TimeUnit::Millisecond => return unix_millis,
        TimeUnit::Second => MILLIS_PER_SECOND,
        TimeUnit::Minute => MILLIS_PER_MINUTE,
        TimeUnit::Hour => MILLIS_PER_HOUR,
        TimeUnit::Day => MILLIS_PER_DAY,
        TimeUnit::Week => MILLIS_PER_WEEK,
        TimeUnit::Month | TimeUnit::Quarter | TimeUnit::Year => 0,
    };
    if fixed_period > 0 {
        let origin = if unit == TimeUnit::Week {
            3 * MILLIS_PER_DAY
        } else {
            0
        };
        return (i128::from(unix_millis)
            - (i128::from(unix_millis) - i128::from(origin)).rem_euclid(i128::from(fixed_period)))
            as i64;
    }

    let Some(datetime) = datetime(unix_millis) else {
        return unix_millis;
    };
    let year = datetime.year();
    let month = datetime.month() as u8;
    let first_month = match unit {
        TimeUnit::Month => month,
        TimeUnit::Quarter => ((month - 1) / 3) * 3 + 1,
        TimeUnit::Year => 1,
        _ => unreachable!("fixed units returned above"),
    };
    let Ok(month) = Month::try_from(first_month) else {
        return unix_millis;
    };
    let Ok(date) = time::Date::from_calendar_date(year, month, 1) else {
        return unix_millis;
    };
    i64::try_from(date.midnight().assume_utc().unix_timestamp_nanos() / 1_000_000)
        .unwrap_or(unix_millis)
}

fn parse_iso8601_utc(raw: &str) -> Option<OffsetDateTime> {
    if let Some(date) = parse_iso_calendar_date(raw) {
        return Some(date.midnight().assume_utc());
    }

    let bytes = raw.as_bytes();
    if bytes.len() < 19
        || bytes
            .get(10)
            .is_none_or(|separator| !matches!(separator, b'T' | b' '))
    {
        return None;
    }
    let mut normalized = raw.to_owned();
    if bytes[10] == b' ' {
        normalized.replace_range(10..11, "T");
    }
    if normalized.ends_with('z') {
        normalized.pop();
        normalized.push('Z');
    }

    let zone_start = normalized
        .as_bytes()
        .iter()
        .enumerate()
        .skip(19)
        .find_map(|(index, byte)| matches!(byte, b'+' | b'-').then_some(index));
    if let Some(index) = zone_start {
        let zone = &normalized[index..];
        if zone.len() == 5 && zone.as_bytes()[3..].iter().all(u8::is_ascii_digit) {
            normalized.insert(index + 3, ':');
        }
    } else if !normalized.ends_with('Z') {
        normalized.push('Z');
    }

    OffsetDateTime::parse(&normalized, &Rfc3339).ok()
}

fn parse_iso_calendar_date(raw: &str) -> Option<time::Date> {
    let bytes = raw.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = raw.get(..4)?.parse::<i32>().ok()?;
    let month = raw.get(5..7)?.parse::<u8>().ok()?;
    let day = raw.get(8..10)?.parse::<u8>().ok()?;
    let month = Month::try_from(month).ok()?;
    time::Date::from_calendar_date(year, month, day).ok()
}

/// D3-compatible UTC temporal ticks.
///
/// The desired count is `ceil(plot_width / 40)`, clamped to 1..=1,000.
/// Selection uses D3's neighboring-duration ratio rule, while range generation
/// uses UTC calendar boundaries for weeks, months, and years. Intervals widen
/// when necessary so no more than 1,000 aligned ticks are emitted. Reversed
/// domains preserve direction.
pub fn temporal_ticks(min_ms: i64, max_ms: i64, plot_width: f64) -> Vec<TemporalTick> {
    let reverse = max_ms < min_ms;
    let (start_ms, stop_ms) = if reverse {
        (max_ms, min_ms)
    } else {
        (min_ms, max_ms)
    };
    if start_ms == stop_ms {
        return vec![TemporalTick {
            unix_millis: start_ms,
            label: tick_label(start_ms),
        }];
    }

    let desired_count = if plot_width.is_finite() && plot_width > 0.0 {
        (plot_width / 40.0)
            .ceil()
            .clamp(1.0, MAX_TEMPORAL_TICKS as f64) as usize
    } else {
        1
    };
    let interval = select_interval(start_ms, stop_ms, desired_count);
    let mut millis = generate_ticks(start_ms, stop_ms, interval);
    if reverse {
        millis.reverse();
    }
    millis
        .into_iter()
        .map(|unix_millis| TemporalTick {
            unix_millis,
            label: tick_label(unix_millis),
        })
        .collect()
}

/// Generate bounded ticks for a Chart.js temporal axis, applying its time unit,
/// minimum unit, tick-count controls, and display-format override.
pub fn temporal_ticks_with_options(
    min_ms: i64,
    max_ms: i64,
    plot_width: f64,
    time_options: &TimeOptions,
    tick_options: &AxisTickOptions,
) -> Vec<TemporalTick> {
    let reverse = max_ms < min_ms;
    let (start_ms, stop_ms) = if reverse {
        (max_ms, min_ms)
    } else {
        (min_ms, max_ms)
    };
    let desired_count = tick_options
        .count
        .unwrap_or_else(|| desired_tick_count(plot_width))
        .clamp(1, MAX_TEMPORAL_TICKS);
    let max_count = tick_options
        .max_ticks_limit
        .unwrap_or(MAX_TEMPORAL_TICKS)
        .clamp(1, MAX_TEMPORAL_TICKS);
    let desired_count = desired_count.min(max_count);
    let output_limit = tick_options
        .count
        .map(|count| count.clamp(1, MAX_TEMPORAL_TICKS).min(max_count))
        .unwrap_or(max_count);

    let mut interval = if let Some(unit) = time_options.unit {
        interval_for_unit(unit, normalized_step(tick_options.step_size))
    } else {
        let mut interval = select_interval_with_min_unit(
            start_ms,
            stop_ms,
            desired_count,
            time_options.min_unit.unwrap_or(TimeUnit::Millisecond),
        );
        if let Some(step_size) = tick_options.step_size {
            interval.step = interval
                .step
                .saturating_mul(normalized_step(Some(step_size)));
            interval.approximate_millis = interval
                .approximate_millis
                .saturating_mul(i64::from(normalized_step(Some(step_size))));
        }
        interval
    };

    if start_ms == stop_ms {
        return vec![TemporalTick {
            unix_millis: start_ms,
            label: format_time_tick(start_ms, interval.unit, time_options),
        }];
    }

    let mut millis = generate_ticks(start_ms, stop_ms, interval);
    while millis.len() > output_limit && interval.step < i32::MAX {
        let stride = millis.len().div_ceil(output_limit).max(2);
        interval.step = interval
            .step
            .saturating_mul(i32::try_from(stride).unwrap_or(i32::MAX));
        interval.approximate_millis = interval
            .approximate_millis
            .saturating_mul(i64::from(i32::try_from(stride).unwrap_or(i32::MAX)));
        millis = generate_ticks(start_ms, stop_ms, interval);
    }
    if reverse {
        millis.reverse();
    }
    millis
        .into_iter()
        .map(|unix_millis| TemporalTick {
            unix_millis,
            label: format_time_tick(unix_millis, interval.unit, time_options),
        })
        .collect()
}

fn desired_tick_count(plot_width: f64) -> usize {
    if plot_width.is_finite() && plot_width > 0.0 {
        (plot_width / 40.0)
            .ceil()
            .clamp(1.0, MAX_TEMPORAL_TICKS as f64) as usize
    } else {
        1
    }
}

fn normalized_step(value: Option<f64>) -> i32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round().clamp(1.0, i32::MAX as f64) as i32)
        .unwrap_or(1)
}

fn interval_for_unit(unit: TimeUnit, step: i32) -> TickInterval {
    let unit = match unit {
        TimeUnit::Millisecond => TickUnit::Millisecond,
        TimeUnit::Second => TickUnit::Second,
        TimeUnit::Minute => TickUnit::Minute,
        TimeUnit::Hour => TickUnit::Hour,
        TimeUnit::Day => TickUnit::Day,
        TimeUnit::Week => TickUnit::Week,
        TimeUnit::Month => TickUnit::Month,
        TimeUnit::Quarter => TickUnit::Quarter,
        TimeUnit::Year => TickUnit::Year,
    };
    let base = match unit {
        TickUnit::Millisecond => 1,
        TickUnit::Second => MILLIS_PER_SECOND,
        TickUnit::Minute => MILLIS_PER_MINUTE,
        TickUnit::Hour => MILLIS_PER_HOUR,
        TickUnit::Day => MILLIS_PER_DAY,
        TickUnit::Week => MILLIS_PER_WEEK,
        TickUnit::Month => APPROX_MILLIS_PER_MONTH,
        TickUnit::Quarter => 3 * APPROX_MILLIS_PER_MONTH,
        TickUnit::Year => APPROX_MILLIS_PER_YEAR,
    };
    TickInterval::new(unit, step, base.saturating_mul(i64::from(step)))
}

fn select_interval(start_ms: i64, stop_ms: i64, desired_count: usize) -> TickInterval {
    select_interval_with_min_unit(start_ms, stop_ms, desired_count, TimeUnit::Millisecond)
}

fn select_interval_with_min_unit(
    start_ms: i64,
    stop_ms: i64,
    desired_count: usize,
    min_unit: TimeUnit,
) -> TickInterval {
    let target = (i128::from(stop_ms) - i128::from(start_ms)) as f64 / desired_count.max(1) as f64;
    let min_tick_unit = interval_for_unit(min_unit, 1).unit;
    let eligible = TICK_INTERVALS
        .iter()
        .copied()
        .filter(|interval| interval.unit >= min_tick_unit)
        .collect::<Vec<_>>();
    let upper = eligible.partition_point(|interval| interval.approximate_millis as f64 <= target);
    if upper == 0 {
        if let Some(first) = eligible.first()
            && min_tick_unit != TickUnit::Millisecond
        {
            return *first;
        }
        let span_millis = (i128::from(stop_ms) - i128::from(start_ms)) as f64;
        let step = nice_tick_step(span_millis, desired_count)
            .round()
            .clamp(1.0, i32::MAX as f64) as i32;
        return TickInterval::new(TickUnit::Millisecond, step, i64::from(step));
    }
    if upper == eligible.len() {
        let span_years = target * desired_count as f64 / APPROX_MILLIS_PER_YEAR as f64;
        let step = nice_tick_step(span_years, desired_count).round().max(1.0) as i32;
        return TickInterval::new(
            TickUnit::Year,
            step,
            i64::from(step).saturating_mul(APPROX_MILLIS_PER_YEAR),
        );
    }
    let previous = eligible[upper - 1];
    let next = eligible[upper];
    if target / (previous.approximate_millis as f64) < next.approximate_millis as f64 / target {
        previous
    } else {
        next
    }
}

/// Equivalent to d3-array's positive `tickStep(0, span, count)`.
fn nice_tick_step(span: f64, count: usize) -> f64 {
    let step = span / count.max(1) as f64;
    let power = step.log10().floor();
    let error = step / 10_f64.powf(power);
    let factor = if error >= 50_f64.sqrt() {
        10.0
    } else if error >= 10_f64.sqrt() {
        5.0
    } else if error >= 2_f64.sqrt() {
        2.0
    } else {
        1.0
    };
    10_f64.powf(power) * factor
}

fn generate_ticks(start_ms: i64, stop_ms: i64, interval: TickInterval) -> Vec<i64> {
    match interval.unit {
        TickUnit::Millisecond => generate_fixed(start_ms, stop_ms, i64::from(interval.step), 0),
        TickUnit::Second => generate_fixed(
            start_ms,
            stop_ms,
            i64::from(interval.step) * MILLIS_PER_SECOND,
            0,
        ),
        TickUnit::Minute => generate_fixed(
            start_ms,
            stop_ms,
            i64::from(interval.step) * MILLIS_PER_MINUTE,
            0,
        ),
        TickUnit::Hour => generate_fixed(
            start_ms,
            stop_ms,
            i64::from(interval.step) * MILLIS_PER_HOUR,
            0,
        ),
        TickUnit::Day => generate_fixed(
            start_ms,
            stop_ms,
            i64::from(interval.step) * MILLIS_PER_DAY,
            0,
        ),
        // 1970-01-04T00:00:00Z is the first Sunday after the Unix epoch.
        TickUnit::Week => generate_fixed(
            start_ms,
            stop_ms,
            i64::from(interval.step) * MILLIS_PER_WEEK,
            3 * MILLIS_PER_DAY,
        ),
        TickUnit::Month => generate_calendar(start_ms, stop_ms, TickUnit::Month, interval.step),
        TickUnit::Quarter => generate_calendar(start_ms, stop_ms, TickUnit::Quarter, interval.step),
        TickUnit::Year => generate_calendar(start_ms, stop_ms, TickUnit::Year, interval.step),
    }
}

fn generate_fixed(start_ms: i64, stop_ms: i64, step_ms: i64, origin_ms: i64) -> Vec<i64> {
    let start = i128::from(start_ms);
    let stop = i128::from(stop_ms);
    let base_step = i128::from(step_ms);
    let origin = i128::from(origin_ms);
    let Some((_, base_count)) = fixed_tick_range(start, stop, base_step, origin) else {
        return Vec::new();
    };
    if base_count == 0 {
        return Vec::new();
    }
    let stride = (base_count + MAX_TEMPORAL_TICKS as i128 - 1) / MAX_TEMPORAL_TICKS as i128;
    let step = base_step * stride;
    let (aligned, count) =
        fixed_tick_range(start, stop, step, origin).expect("widened fixed step stays positive");

    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let current = aligned + index * step;
        // `current` is bounded by the i64-derived start/stop values here.
        out.push(current as i64);
    }
    out
}

fn fixed_tick_range(start: i128, stop: i128, step: i128, origin: i128) -> Option<(i128, i128)> {
    if step <= 0 {
        return None;
    }
    let relative = start - origin;
    let quotient = relative.div_euclid(step);
    let aligned = origin
        + if relative.rem_euclid(step) == 0 {
            quotient
        } else {
            quotient + 1
        } * step;
    let count = if aligned > stop {
        0
    } else {
        (stop - aligned).div_euclid(step) + 1
    };
    Some((aligned, count))
}

fn generate_calendar(start_ms: i64, stop_ms: i64, unit: TickUnit, step: i32) -> Vec<i64> {
    let Some(start) = datetime(start_ms) else {
        return Vec::new();
    };
    let stop_date = datetime(stop_ms)
        .map(OffsetDateTime::date)
        .unwrap_or(time::Date::MAX);
    let (ceil_index, last_index) = match unit {
        TickUnit::Month => {
            let index = i128::from(start.year()) * 12 + i128::from(u8::from(start.month())) - 1;
            let boundary = calendar_millis(TickUnit::Month, index);
            let ceil_index = if boundary.is_some_and(|value| value < start_ms) {
                index + 1
            } else {
                index
            };
            let last_index =
                i128::from(stop_date.year()) * 12 + i128::from(u8::from(stop_date.month())) - 1;
            (ceil_index, last_index)
        }
        TickUnit::Quarter => {
            let month_index = i128::from(u8::from(start.month())) - 1;
            let index = i128::from(start.year()) * 4 + month_index.div_euclid(3);
            let boundary = calendar_millis(TickUnit::Quarter, index);
            let ceil_index = if boundary.is_some_and(|value| value < start_ms) {
                index + 1
            } else {
                index
            };
            let last_month_index = i128::from(u8::from(stop_date.month())) - 1;
            let last_index = i128::from(stop_date.year()) * 4 + last_month_index.div_euclid(3);
            (ceil_index, last_index)
        }
        TickUnit::Year => {
            let year = i128::from(start.year());
            let boundary = calendar_millis(TickUnit::Year, year);
            let ceil_index = if boundary.is_some_and(|value| value < start_ms) {
                year + 1
            } else {
                year
            };
            (ceil_index, i128::from(stop_date.year()))
        }
        _ => return Vec::new(),
    };
    let base_step = i128::from(step);
    let first_index = align_calendar_index(ceil_index, base_step);
    let base_count = calendar_tick_count(first_index, last_index, base_step);
    if base_count == 0 {
        return Vec::new();
    }
    let stride = (base_count + MAX_TEMPORAL_TICKS as i128 - 1) / MAX_TEMPORAL_TICKS as i128;
    let widened_step = base_step * stride;
    let first_index = align_calendar_index(ceil_index, widened_step);
    let count = calendar_tick_count(first_index, last_index, widened_step);

    (0..count)
        .map_while(|offset| {
            let index = first_index + offset * widened_step;
            calendar_millis(unit, index)
        })
        .filter(|value| *value >= start_ms && *value <= stop_ms)
        .collect()
}

fn calendar_tick_count(first_index: i128, last_index: i128, step: i128) -> i128 {
    if step <= 0 || first_index > last_index {
        0
    } else {
        (last_index - first_index).div_euclid(step) + 1
    }
}

fn align_calendar_index(index: i128, step: i128) -> i128 {
    let remainder = index.rem_euclid(step);
    if remainder == 0 {
        index
    } else {
        index + step - remainder
    }
}

fn calendar_millis(unit: TickUnit, index: i128) -> Option<i64> {
    let (year, month) = match unit {
        TickUnit::Month => {
            let year = index.div_euclid(12);
            let month = Month::try_from((index.rem_euclid(12) + 1) as u8).ok()?;
            (year, month)
        }
        TickUnit::Quarter => {
            let year = index.div_euclid(4);
            let month = Month::try_from((index.rem_euclid(4) * 3 + 1) as u8).ok()?;
            (year, month)
        }
        TickUnit::Year => (index, Month::January),
        _ => return None,
    };
    let year = i32::try_from(year).ok()?;
    let datetime = time::Date::from_calendar_date(year, month, 1)
        .ok()?
        .midnight()
        .assume_utc();
    i64::try_from(datetime.unix_timestamp_nanos() / 1_000_000).ok()
}

fn datetime(unix_millis: i64) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(unix_millis) * 1_000_000).ok()
}

fn tick_label(unix_millis: i64) -> String {
    let Some(datetime) = datetime(unix_millis) else {
        return unix_millis.to_string();
    };
    if datetime.millisecond() != 0 {
        return format!(".{:03}", datetime.millisecond());
    }
    if datetime.second() != 0 {
        return format!(":{:02}", datetime.second());
    }
    if datetime.minute() != 0 {
        return format!("{:02}:{:02}", hour12(datetime.hour()), datetime.minute());
    }
    if datetime.hour() != 0 {
        return format!(
            "{:02} {}",
            hour12(datetime.hour()),
            if datetime.hour() < 12 { "AM" } else { "PM" }
        );
    }
    if datetime.day() != 1 {
        if datetime.weekday() == Weekday::Sunday {
            return format!(
                "{} {:02}",
                month_abbreviation(datetime.month()),
                datetime.day()
            );
        }
        return format!(
            "{} {:02}",
            weekday_abbreviation(datetime.weekday()),
            datetime.day()
        );
    }
    if datetime.month() != Month::January {
        return month_name(datetime.month()).to_string();
    }
    datetime.year().to_string()
}

fn format_time_tick(unix_millis: i64, unit: TickUnit, options: &TimeOptions) -> String {
    let time_unit = match unit {
        TickUnit::Millisecond => TimeUnit::Millisecond,
        TickUnit::Second => TimeUnit::Second,
        TickUnit::Minute => TimeUnit::Minute,
        TickUnit::Hour => TimeUnit::Hour,
        TickUnit::Day => TimeUnit::Day,
        TickUnit::Week => TimeUnit::Week,
        TickUnit::Month => TimeUnit::Month,
        TickUnit::Quarter => TimeUnit::Quarter,
        TickUnit::Year => TimeUnit::Year,
    };
    let Some(format) = options.display_formats.get(&time_unit) else {
        return tick_label(unix_millis);
    };
    let Some(datetime) = datetime(unix_millis) else {
        return unix_millis.to_string();
    };
    let mut chars = format.chars().peekable();
    let mut label = String::with_capacity(format.len());
    while let Some(character) = chars.next() {
        if character != '%' {
            label.push(character);
            continue;
        }
        let Some(mut directive) = chars.next() else {
            return tick_label(unix_millis);
        };
        let fractional_with_dot = directive == '.' && chars.peek() == Some(&'f');
        if fractional_with_dot {
            chars.next();
            directive = 'f';
        }
        match directive {
            'Y' => write!(label, "{:04}", datetime.year()).unwrap(),
            'y' => write!(label, "{:02}", datetime.year().rem_euclid(100)).unwrap(),
            'm' => write!(label, "{:02}", u8::from(datetime.month())).unwrap(),
            'b' => write!(label, "{}", month_abbreviation(datetime.month())).unwrap(),
            'B' => write!(label, "{}", month_name(datetime.month())).unwrap(),
            'd' => write!(label, "{:02}", datetime.day()).unwrap(),
            'a' => write!(label, "{}", weekday_abbreviation(datetime.weekday())).unwrap(),
            'A' => write!(label, "{}", weekday_name(datetime.weekday())).unwrap(),
            'H' => write!(label, "{:02}", datetime.hour()).unwrap(),
            'I' => write!(label, "{:02}", hour12(datetime.hour())).unwrap(),
            'M' => write!(label, "{:02}", datetime.minute()).unwrap(),
            'S' => write!(label, "{:02}", datetime.second()).unwrap(),
            'f' => {
                if fractional_with_dot {
                    label.push('.');
                }
                write!(label, "{:03}", datetime.millisecond()).unwrap();
            }
            'p' => write!(label, "{}", if datetime.hour() < 12 { "AM" } else { "PM" }).unwrap(),
            'z' => label.push_str("+0000"),
            '%' => label.push('%'),
            _ => return tick_label(unix_millis),
        }
    }
    label
}

fn hour12(hour: u8) -> u8 {
    match hour % 12 {
        0 => 12,
        value => value,
    }
}

fn weekday_abbreviation(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn month_abbreviation(month: Month) -> &'static str {
    match month {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

fn month_name(month: Month) -> &'static str {
    match month {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet};

    fn millis(raw: &str) -> i64 {
        parse_rfc3339_millis("x", raw).unwrap()
    }

    #[test]
    fn equivalent_offsets_normalize_to_same_millis() {
        let z = parse_rfc3339_millis("timestamp", "2026-07-22T19:18:38Z").unwrap();
        let offset = parse_rfc3339_millis("timestamp", "2026-07-23T04:18:38+09:00").unwrap();
        assert_eq!(z, offset);
    }

    #[test]
    fn parse_iso_calendar_date_uses_utc_midnight() {
        assert_eq!(
            parse_rfc3339_millis("date", "1970-01-02").unwrap(),
            MILLIS_PER_DAY
        );
    }

    #[test]
    fn fractional_format_directive_requires_the_documented_dot() {
        assert!(validate_time_format("parser", "%Y-%m-%dT%H:%M:%S%.f", false).is_ok());
        assert!(validate_time_format("parser", "%Y-%m-%dT%H:%M:%S%f", false).is_err());
        assert!(validate_time_format("display", "%Y-%m-%d %.f", true).is_ok());
        assert!(validate_time_format("display", "%Y-%m-%d %f", true).is_err());
    }

    #[test]
    fn configured_ticks_honor_quarter_unit_and_custom_display_format() {
        let time_options = TimeOptions {
            unit: Some(TimeUnit::Quarter),
            display_formats: BTreeMap::from([(TimeUnit::Quarter, "%Y-%m".to_owned())]),
            ..TimeOptions::default()
        };
        let ticks = AxisTickOptions::default();

        let actual = temporal_ticks_with_options(
            millis("2026-01-01T00:00:00Z"),
            millis("2026-10-01T00:00:00Z"),
            720.0,
            &time_options,
            &ticks,
        );

        assert_eq!(
            actual
                .iter()
                .map(|tick| (tick.unix_millis, tick.label.as_str()))
                .collect::<Vec<_>>(),
            [
                (millis("2026-01-01T00:00:00Z"), "2026-01"),
                (millis("2026-04-01T00:00:00Z"), "2026-04"),
                (millis("2026-07-01T00:00:00Z"), "2026-07"),
                (millis("2026-10-01T00:00:00Z"), "2026-10"),
            ]
        );
    }

    #[test]
    fn configured_ticks_respect_min_unit_and_max_ticks_limit() {
        let time_options = TimeOptions {
            min_unit: Some(TimeUnit::Day),
            ..TimeOptions::default()
        };
        let ticks = temporal_ticks_with_options(
            millis("2026-01-01T12:00:00Z"),
            millis("2026-01-04T12:00:00Z"),
            2_400.0,
            &time_options,
            &AxisTickOptions::default(),
        );

        assert_eq!(ticks.len(), 3);
        assert!(
            ticks
                .iter()
                .all(|tick| tick.unix_millis % MILLIS_PER_DAY == 0)
        );
        assert!(
            ticks
                .windows(2)
                .all(|pair| pair[1].unix_millis - pair[0].unix_millis == MILLIS_PER_DAY)
        );

        let capped = temporal_ticks_with_options(
            millis("2026-01-01T00:00:00Z"),
            millis("2026-01-10T00:00:00Z"),
            720.0,
            &TimeOptions {
                unit: Some(TimeUnit::Day),
                ..TimeOptions::default()
            },
            &AxisTickOptions {
                max_ticks_limit: Some(2),
                ..AxisTickOptions::default()
            },
        );
        assert!(capped.len() <= 2);
    }

    #[test]
    fn configured_unit_respects_count_and_max_ticks_limit() {
        let start = millis("2026-01-01T00:00:00Z");
        let stop = millis("2026-01-31T00:00:00Z");
        let time_options = TimeOptions {
            unit: Some(TimeUnit::Day),
            ..TimeOptions::default()
        };
        let count_limited = temporal_ticks_with_options(
            start,
            stop,
            1_200.0,
            &time_options,
            &AxisTickOptions {
                count: Some(2),
                ..AxisTickOptions::default()
            },
        );
        assert_eq!(count_limited.len(), 2);
        assert!(
            count_limited
                .windows(2)
                .all(|pair| { (pair[1].unix_millis - pair[0].unix_millis) % MILLIS_PER_DAY == 0 })
        );

        let max_limited = temporal_ticks_with_options(
            start,
            stop,
            1_200.0,
            &time_options,
            &AxisTickOptions {
                count: Some(10),
                max_ticks_limit: Some(2),
                ..AxisTickOptions::default()
            },
        );
        assert!(max_limited.len() <= 2);
    }

    #[test]
    fn configured_week_unit_respects_step_and_max_ticks_limit() {
        let ticks = temporal_ticks_with_options(
            millis("2026-01-01T00:00:00Z"),
            millis("2027-01-01T00:00:00Z"),
            1_200.0,
            &TimeOptions {
                unit: Some(TimeUnit::Week),
                ..TimeOptions::default()
            },
            &AxisTickOptions {
                step_size: Some(2.0),
                max_ticks_limit: Some(2),
                ..AxisTickOptions::default()
            },
        );

        assert!(!ticks.is_empty());
        assert!(ticks.len() <= 2);
        assert!(ticks.windows(2).all(|pair| {
            (pair[1].unix_millis - pair[0].unix_millis) % (2 * MILLIS_PER_WEEK) == 0
        }));
    }

    #[test]
    fn time_scale_preserves_elapsed_spacing() {
        let scale = TemporalScale::new(
            crate::ir::ScaleKind::Time,
            &[0, MILLIS_PER_DAY, 3 * MILLIS_PER_DAY],
            0.0,
            300.0,
        );
        assert!((scale.map_millis(MILLIS_PER_DAY) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn timeseries_scale_spaces_unique_timestamps_equally() {
        let scale = TemporalScale::new(
            crate::ir::ScaleKind::Timeseries,
            &[0, MILLIS_PER_DAY, 3 * MILLIS_PER_DAY],
            0.0,
            300.0,
        );
        assert!((scale.map_millis(MILLIS_PER_DAY) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn pre_epoch_sub_millisecond_timestamps_floor_to_previous_millisecond() {
        assert_eq!(
            parse_rfc3339_millis("timestamp", "1969-12-31T23:59:59.999999999Z").unwrap(),
            -1
        );
        assert_eq!(
            parse_custom_format_millis(
                "timestamp",
                "1969-12-31T23:59:59.999999999Z",
                "%Y-%m-%dT%H:%M:%S%.f%z",
            )
            .unwrap(),
            -1
        );
        assert_eq!(
            parse_rfc3339_millis("timestamp", "1970-01-01T00:00:00Z").unwrap(),
            0
        );
    }

    #[test]
    fn invalid_timestamp_error_is_bounded_and_identifies_field() {
        let err = parse_rfc3339_millis("timestamp", "not-a-date").unwrap_err();
        assert!(err.contains("timestamp"));
        assert!(err.contains("not-a-date"));
        assert!(err.len() < 160);
    }

    #[test]
    fn iso_timestamp_length_is_bounded_before_parsing() {
        let long_timestamp = format!("1970-01-01T00:00:00Z{}", "x".repeat(MAX_TIME_VALUE_BYTES));
        let err = parse_rfc3339_millis("timestamp", &long_timestamp).unwrap_err();
        assert!(err.contains("timestamp exceeds 4096 bytes"));
        assert!(err.len() < 160);
    }

    #[test]
    fn bounded_error_fragment_preserves_multibyte_boundaries() {
        let raw = format!("{}tail", "あ".repeat(27));
        let shown = bounded_error_fragment(&raw);
        assert!(shown.ends_with("..."));
        assert!(shown.is_char_boundary(shown.len()));
        assert!(!shown.contains("tail"));
    }

    #[test]
    fn dogfood_range_uses_two_day_ticks() {
        let min = millis("2026-06-05T19:55:20Z");
        let max = millis("2026-07-22T19:18:38Z");
        let ticks = temporal_ticks(min, max, 720.0);
        assert!(
            ticks
                .windows(2)
                .all(|w| w[1].unix_millis - w[0].unix_millis == 2 * MILLIS_PER_DAY)
        );
    }

    #[test]
    fn two_day_utc_ticks_remain_epoch_aligned_across_month_boundary() {
        // d3-time's UTC ticker passes `unixDay` (epoch-based), not `utcDay`,
        // into the tick interval table. A two-day interval therefore remains
        // continuous across a month boundary rather than resetting on July 1.
        let ticks = temporal_ticks(
            millis("2026-06-29T12:00:00Z"),
            millis("2026-07-05T12:00:00Z"),
            80.0,
        );
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            vec![
                millis("2026-06-30T00:00:00Z"),
                millis("2026-07-02T00:00:00Z"),
                millis("2026-07-04T00:00:00Z"),
            ]
        );
    }

    #[test]
    fn reversed_and_singleton_ranges_are_bounded() {
        let ticks = temporal_ticks(3_000, 1_000, 720.0);
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            vec![
                3_000, 2_900, 2_800, 2_700, 2_600, 2_500, 2_400, 2_300, 2_200, 2_100, 2_000, 1_900,
                1_800, 1_700, 1_600, 1_500, 1_400, 1_300, 1_200, 1_100, 1_000,
            ]
        );

        let singleton = temporal_ticks(1_234, 1_234, 720.0);
        assert_eq!(singleton.len(), 1);
        assert_eq!(singleton[0].unix_millis, 1_234);
        assert_eq!(singleton[0].label, ".234");
    }

    #[test]
    fn invalid_width_alignment_and_out_of_range_labels_are_bounded() {
        let ticks = temporal_ticks(0, 3_000, f64::NAN);
        assert!(!ticks.is_empty());
        assert!(generate_fixed(1, 999, 1_000, 0).is_empty());
        assert_eq!(tick_label(i64::MAX), i64::MAX.to_string());
    }

    #[test]
    fn short_sub_day_ticks_use_dynamic_time_labels_without_duplicates() {
        let ticks = temporal_ticks(
            millis("2026-07-15T12:00:00Z"),
            millis("2026-07-15T12:00:30Z"),
            400.0,
        );
        assert_eq!(ticks[1].label, ":05");
        assert_eq!(
            ticks
                .iter()
                .map(|tick| &tick.label)
                .collect::<HashSet<_>>()
                .len(),
            ticks.len()
        );
    }

    #[test]
    fn sub_second_domains_generate_millisecond_ticks() {
        let ticks = temporal_ticks(100, 900, 400.0);
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            vec![100, 200, 300, 400, 500, 600, 700, 800, 900]
        );
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.label.as_str())
                .collect::<Vec<_>>(),
            [
                ".100", ".200", ".300", ".400", ".500", ".600", ".700", ".800", ".900"
            ]
        );
    }

    #[test]
    fn sub_second_target_uses_millisecond_ticks_for_longer_domain() {
        let ticks = temporal_ticks(0, 1_500, 720.0);
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            (0..=1_500).step_by(100).collect::<Vec<_>>()
        );
    }

    #[test]
    fn minute_ticks_use_minute_alignment_and_labels() {
        let ticks = temporal_ticks(
            millis("2026-07-15T12:00:00Z"),
            millis("2026-07-15T12:10:00Z"),
            80.0,
        );
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.label.as_str())
                .collect::<Vec<_>>(),
            ["12 PM", "12:05", "12:10"]
        );
    }

    #[test]
    fn calendar_boundaries_use_d3_dynamic_utc_labels() {
        let ticks = temporal_ticks(
            millis("2024-07-13T00:00:00Z"),
            millis("2024-07-17T00:00:00Z"),
            160.0,
        );
        let labels = ticks
            .iter()
            .map(|tick| tick.label.as_str())
            .collect::<Vec<_>>();
        assert!(labels.contains(&"Jul 14"), "{labels:?}");
        assert!(labels.contains(&"Mon 15"), "{labels:?}");
    }

    #[test]
    fn calendar_intervals_cover_multi_month_and_multi_year_domains() {
        let months = temporal_ticks(
            millis("2026-01-01T00:00:00Z"),
            millis("2026-07-01T00:00:00Z"),
            400.0,
        );
        assert_eq!(
            months
                .iter()
                .map(|tick| tick.label.as_str())
                .collect::<Vec<_>>(),
            ["2026", "February", "March", "April", "May", "June", "July"]
        );

        let years = temporal_ticks(
            millis("2018-01-01T00:00:00Z"),
            millis("2026-01-01T00:00:00Z"),
            400.0,
        );
        assert_eq!(years.first().map(|tick| tick.label.as_str()), Some("2018"));
        assert_eq!(years.last().map(|tick| tick.label.as_str()), Some("2026"));
        assert!(years.iter().all(|tick| tick.label.len() == 4));
    }

    #[test]
    fn beyond_table_uses_nice_multiple_of_years() {
        let ticks = temporal_ticks(
            millis("1900-01-01T00:00:00Z"),
            millis("2100-01-01T00:00:00Z"),
            400.0,
        );
        let years = ticks
            .iter()
            .map(|tick| tick.label.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(years.first(), Some(&1900));
        assert_eq!(years.last(), Some(&2100));
        assert!(years.windows(2).all(|pair| pair[1] - pair[0] == 20));
    }

    #[test]
    fn nice_year_steps_cover_d3_one_five_and_ten_factors() {
        assert_eq!(nice_tick_step(12.0, 10), 1.0);
        assert_eq!(nice_tick_step(45.0, 10), 5.0);
        assert_eq!(nice_tick_step(90.0, 10), 10.0);
    }

    #[test]
    fn week_ticks_align_to_sunday() {
        let ticks = temporal_ticks(
            millis("2026-01-01T00:00:00Z"),
            millis("2026-03-01T00:00:00Z"),
            400.0,
        );
        assert!(ticks.iter().all(|tick| {
            datetime(tick.unix_millis).expect("valid tick").weekday() == Weekday::Sunday
        }));
    }

    #[test]
    fn calendar_generation_ceil_aligns_partial_months_and_years() {
        let mid_january = millis("2026-01-15T00:00:00Z");
        let april = millis("2026-04-01T00:00:00Z");
        assert_eq!(
            generate_calendar(mid_january, april, TickUnit::Month, 3),
            vec![april]
        );

        let mid_year = millis("2026-07-01T00:00:00Z");
        let year_2030 = millis("2030-01-01T00:00:00Z");
        assert_eq!(
            generate_calendar(mid_year, year_2030, TickUnit::Year, 5),
            vec![year_2030]
        );
        assert!(generate_calendar(0, 1, TickUnit::Day, 1).is_empty());
        assert_eq!(
            calendar_millis(TickUnit::Year, 2026),
            Some(millis("2026-01-01T00:00:00Z"))
        );
        assert_eq!(calendar_millis(TickUnit::Day, 0), None);
    }

    #[test]
    fn calendar_ticks_reject_datetimes_outside_time_crate_range() {
        assert!(generate_calendar(i64::MIN, i64::MAX, TickUnit::Year, 1).is_empty());
    }

    #[test]
    fn calendar_operations_cover_javascript_date_range() {
        const JS_DATE_LIMIT_MILLIS: i64 = 8_640_000_000_000_000;
        let rounded = round_timestamp(JS_DATE_LIMIT_MILLIS, TimeUnit::Year);
        assert!(rounded < JS_DATE_LIMIT_MILLIS);
        let rounded_date = datetime(rounded).expect("rounded year is representable");
        assert_eq!(rounded_date.month(), Month::January);
        assert_eq!(rounded_date.day(), 1);

        let options = TimeOptions {
            unit: Some(TimeUnit::Year),
            ..TimeOptions::default()
        };
        let ticks = temporal_ticks_with_options(
            -JS_DATE_LIMIT_MILLIS,
            JS_DATE_LIMIT_MILLIS,
            720.0,
            &options,
            &AxisTickOptions::default(),
        );
        assert!(!ticks.is_empty());
        assert!(
            ticks
                .iter()
                .all(|tick| datetime(tick.unix_millis).is_some())
        );
    }

    #[test]
    fn tick_generators_reject_invalid_steps_and_reversed_ranges() {
        assert!(generate_fixed(0, 1_000, 0, 0).is_empty());

        let january = millis("2026-01-01T00:00:00Z");
        let february = millis("2026-02-01T00:00:00Z");
        assert!(generate_calendar(february, january, TickUnit::Month, 1).is_empty());
    }

    #[test]
    fn ticks_cover_full_domain_without_twenty_four_entry_cap() {
        let min = millis("2026-01-01T00:00:00Z");
        let max = millis("2026-03-31T00:00:00Z");
        let ticks = temporal_ticks(min, max, 3_600.0);
        assert!(ticks.len() > 24);
        assert_eq!(ticks.first().map(|tick| tick.unix_millis), Some(min));
        assert_eq!(ticks.last().map(|tick| tick.unix_millis), Some(max));
    }

    #[test]
    fn temporal_ticks_cap_huge_width_fixed_ticks_and_preserve_direction() {
        let forward = temporal_ticks(0, 1_000_000, f64::MAX);
        assert_eq!(forward.len(), 501);
        assert!(
            forward
                .windows(2)
                .all(|pair| pair[0].unix_millis < pair[1].unix_millis)
        );
        let step = forward[1].unix_millis - forward[0].unix_millis;
        assert_eq!(step, 2_000);
        assert!(
            forward
                .iter()
                .all(|tick| tick.unix_millis.rem_euclid(step) == 0)
        );
        assert_eq!(forward.first().unwrap().unix_millis, 0);
        assert_eq!(forward.last().unwrap().unix_millis, 1_000_000);

        let reverse = temporal_ticks(1_000_000, 0, f64::MAX);
        assert_eq!(
            reverse
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            forward
                .iter()
                .rev()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn temporal_ticks_cap_huge_width_before_interval_selection() {
        let huge_width = temporal_ticks(0, MILLIS_PER_HOUR, f64::MAX);
        let maximum_desired_count = temporal_ticks(0, MILLIS_PER_HOUR, 40_000.0);

        assert_eq!(huge_width, maximum_desired_count);
        assert_eq!(huge_width.len(), 721);
        assert!(
            huge_width
                .windows(2)
                .all(|pair| { pair[1].unix_millis - pair[0].unix_millis == 5 * MILLIS_PER_SECOND })
        );
    }

    #[test]
    fn temporal_ticks_cap_calendar_and_extreme_domains_before_generation() {
        let min = millis("1900-01-01T00:00:00Z");
        let max = millis("2000-01-01T00:00:00Z");
        let forward = temporal_ticks(min, max, 40_000.0);
        assert_eq!(forward.len(), 601);
        assert!(
            forward
                .windows(2)
                .all(|pair| pair[0].unix_millis < pair[1].unix_millis)
        );
        assert_eq!(forward.first().unwrap().unix_millis, min);
        assert_eq!(forward.last().unwrap().unix_millis, max);
        assert!(forward.windows(2).all(|pair| {
            let first = datetime(pair[0].unix_millis).expect("calendar tick");
            let second = datetime(pair[1].unix_millis).expect("calendar tick");
            let first_index = first.year() * 12 + i32::from(u8::from(first.month())) - 1;
            let second_index = second.year() * 12 + i32::from(u8::from(second.month())) - 1;
            first.day() == 1 && second.day() == 1 && second_index - first_index == 2
        }));

        let reverse = temporal_ticks(max, min, 40_000.0);
        assert_eq!(
            reverse
                .iter()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>(),
            forward
                .iter()
                .rev()
                .map(|tick| tick.unix_millis)
                .collect::<Vec<_>>()
        );

        let extreme_min = calendar_millis(TickUnit::Year, -9_999).unwrap();
        let extreme_max = calendar_millis(TickUnit::Year, 9_999).unwrap();
        let extreme = temporal_ticks(extreme_min, extreme_max, f64::MAX);
        assert!(!extreme.is_empty());
        assert!(extreme.len() <= 1_000);
        assert!(
            extreme
                .windows(2)
                .all(|pair| pair[0].unix_millis < pair[1].unix_millis)
        );
    }

    #[test]
    fn dynamic_tick_labels_cover_every_month() {
        let cases = [
            ("2026-01-01T00:00:00Z", "2026"),
            ("2026-02-01T00:00:00Z", "February"),
            ("2026-03-01T00:00:00Z", "March"),
            ("2026-04-01T00:00:00Z", "April"),
            ("2026-05-01T00:00:00Z", "May"),
            ("2026-06-01T00:00:00Z", "June"),
            ("2026-07-01T00:00:00Z", "July"),
            ("2026-08-01T00:00:00Z", "August"),
            ("2026-09-01T00:00:00Z", "September"),
            ("2026-10-01T00:00:00Z", "October"),
            ("2026-11-01T00:00:00Z", "November"),
            ("2026-12-01T00:00:00Z", "December"),
        ];
        for (timestamp, expected) in cases {
            assert_eq!(tick_label(millis(timestamp)), expected);
        }
    }

    #[test]
    fn dynamic_label_tables_cover_weekdays_and_month_abbreviations() {
        let weekdays = [
            (Weekday::Monday, "Mon"),
            (Weekday::Tuesday, "Tue"),
            (Weekday::Wednesday, "Wed"),
            (Weekday::Thursday, "Thu"),
            (Weekday::Friday, "Fri"),
            (Weekday::Saturday, "Sat"),
            (Weekday::Sunday, "Sun"),
        ];
        for (weekday, expected) in weekdays {
            assert_eq!(weekday_abbreviation(weekday), expected);
        }

        let months = [
            (Month::January, "Jan", "January"),
            (Month::February, "Feb", "February"),
            (Month::March, "Mar", "March"),
            (Month::April, "Apr", "April"),
            (Month::May, "May", "May"),
            (Month::June, "Jun", "June"),
            (Month::July, "Jul", "July"),
            (Month::August, "Aug", "August"),
            (Month::September, "Sep", "September"),
            (Month::October, "Oct", "October"),
            (Month::November, "Nov", "November"),
            (Month::December, "Dec", "December"),
        ];
        for (month, abbreviation, name) in months {
            assert_eq!(month_abbreviation(month), abbreviation);
            assert_eq!(month_name(month), name);
        }
    }
}
