use time::{Month, OffsetDateTime, Weekday, format_description::well_known::Rfc3339};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;
const MILLIS_PER_WEEK: i64 = 7 * MILLIS_PER_DAY;
const APPROX_MILLIS_PER_MONTH: i64 = 30 * MILLIS_PER_DAY;
const APPROX_MILLIS_PER_YEAR: i64 = 365 * MILLIS_PER_DAY;
const MAX_ERROR_FRAGMENT_BYTES: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickInterval {
    unit: TickUnit,
    step: i32,
    approximate_millis: i64,
}

const TICK_INTERVALS: [TickInterval; 18] = [
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
    let parsed = OffsetDateTime::parse(raw, &Rfc3339).map_err(|_| {
        let shown = bounded_error_fragment(raw);
        format!("field {shown_field} contains invalid RFC 3339 timestamp: {shown:?}")
    })?;
    i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| format!("field {shown_field} timestamp is outside the supported range"))
}

/// D3-compatible UTC temporal ticks.
///
/// The desired count is `ceil(plot_width / 40)`. Selection uses D3's
/// neighboring-duration ratio rule, while range generation uses UTC calendar
/// boundaries for weeks, months, and years. Reversed domains preserve direction.
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
        (plot_width / 40.0).ceil().max(1.0) as usize
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

fn select_interval(start_ms: i64, stop_ms: i64, desired_count: usize) -> TickInterval {
    let target = (i128::from(stop_ms) - i128::from(start_ms)) as f64 / desired_count.max(1) as f64;
    let upper =
        TICK_INTERVALS.partition_point(|interval| interval.approximate_millis as f64 <= target);
    if upper == 0 {
        let span_millis = (i128::from(stop_ms) - i128::from(start_ms)) as f64;
        if span_millis < MILLIS_PER_SECOND as f64 {
            let step = nice_tick_step(span_millis, desired_count)
                .round()
                .clamp(1.0, i32::MAX as f64) as i32;
            return TickInterval::new(TickUnit::Millisecond, step, i64::from(step));
        }
        return TICK_INTERVALS[0];
    }
    if upper == TICK_INTERVALS.len() {
        let span_years = target * desired_count as f64 / APPROX_MILLIS_PER_YEAR as f64;
        let step = nice_tick_step(span_years, desired_count).round().max(1.0) as i32;
        return TickInterval::new(
            TickUnit::Year,
            step,
            i64::from(step).saturating_mul(APPROX_MILLIS_PER_YEAR),
        );
    }
    let previous = TICK_INTERVALS[upper - 1];
    let next = TICK_INTERVALS[upper];
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
        TickUnit::Week => generate_fixed(start_ms, stop_ms, MILLIS_PER_WEEK, 3 * MILLIS_PER_DAY),
        TickUnit::Month => generate_calendar(start_ms, stop_ms, TickUnit::Month, interval.step),
        TickUnit::Year => generate_calendar(start_ms, stop_ms, TickUnit::Year, interval.step),
    }
}

fn generate_fixed(start_ms: i64, stop_ms: i64, step_ms: i64, origin_ms: i64) -> Vec<i64> {
    let start = i128::from(start_ms);
    let stop = i128::from(stop_ms);
    let step = i128::from(step_ms);
    let origin = i128::from(origin_ms);
    let relative = start - origin;
    let quotient = relative.div_euclid(step);
    let aligned = origin
        + if relative.rem_euclid(step) == 0 {
            quotient
        } else {
            quotient + 1
        } * step;

    let mut out = Vec::new();
    let mut current = aligned;
    while current <= stop {
        // `current` is bounded by the i64-derived start/stop values here.
        out.push(current as i64);
        current += step;
    }
    out
}

fn generate_calendar(start_ms: i64, stop_ms: i64, unit: TickUnit, step: i32) -> Vec<i64> {
    let Some(start) = datetime(start_ms) else {
        return Vec::new();
    };
    let first_index = match unit {
        TickUnit::Month => {
            let index = start.year().saturating_mul(12) + i32::from(u8::from(start.month())) - 1;
            let boundary = calendar_millis(TickUnit::Month, index);
            let ceil_index = if boundary.is_some_and(|value| value < start_ms) {
                index.saturating_add(1)
            } else {
                index
            };
            align_calendar_index(ceil_index, step)
        }
        TickUnit::Year => {
            let boundary = calendar_millis(TickUnit::Year, start.year());
            let ceil_year = if boundary.is_some_and(|value| value < start_ms) {
                start.year().saturating_add(1)
            } else {
                start.year()
            };
            align_calendar_index(ceil_year, step)
        }
        _ => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut index = first_index;
    while let Some(value) = calendar_millis(unit, index) {
        if value > stop_ms {
            break;
        }
        if value >= start_ms {
            out.push(value);
        }
        index = index.saturating_add(step);
    }
    out
}

fn align_calendar_index(index: i32, step: i32) -> i32 {
    let remainder = index.rem_euclid(step);
    if remainder == 0 {
        index
    } else {
        index.saturating_add(step - remainder)
    }
}

fn calendar_millis(unit: TickUnit, index: i32) -> Option<i64> {
    let (year, month) = match unit {
        TickUnit::Month => {
            let year = index.div_euclid(12);
            let month = Month::try_from((index.rem_euclid(12) + 1) as u8).ok()?;
            (year, month)
        }
        TickUnit::Year => (index, Month::January),
        _ => return None,
    };
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
    use std::collections::HashSet;

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
    fn invalid_timestamp_error_is_bounded_and_identifies_field() {
        let err = parse_rfc3339_millis("timestamp", "not-a-date").unwrap_err();
        assert!(err.contains("timestamp"));
        assert!(err.contains("not-a-date"));
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
            vec![3_000, 2_000, 1_000]
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
    fn ticks_cover_full_domain_without_twenty_four_entry_cap() {
        let min = millis("2026-01-01T00:00:00Z");
        let max = millis("2026-03-31T00:00:00Z");
        let ticks = temporal_ticks(min, max, 3_600.0);
        assert!(ticks.len() > 24);
        assert_eq!(ticks.first().map(|tick| tick.unix_millis), Some(min));
        assert_eq!(ticks.last().map(|tick| tick.unix_millis), Some(max));
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
