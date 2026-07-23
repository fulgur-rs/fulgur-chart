use time::{Month, OffsetDateTime, format_description::well_known::Rfc3339};

const MILLIS_PER_DAY: i64 = 86_400_000;
const INTERVALS_MILLIS: [i64; 15] = [
    1_000,
    5_000,
    15_000,
    30_000,
    60_000,
    5 * 60_000,
    15 * 60_000,
    30 * 60_000,
    60 * 60_000,
    3 * 60 * 60_000,
    6 * 60 * 60_000,
    12 * 60 * 60_000,
    MILLIS_PER_DAY,
    2 * MILLIS_PER_DAY,
    7 * MILLIS_PER_DAY,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalTick {
    pub unix_millis: i64,
    pub label: String,
}

pub fn parse_rfc3339_millis(field: &str, raw: &str) -> Result<i64, String> {
    let parsed = OffsetDateTime::parse(raw, &Rfc3339).map_err(|_| {
        let shown: String = raw.chars().take(80).collect();
        format!("field {field} contains invalid RFC 3339 timestamp: {shown:?}")
    })?;
    i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| format!("field {field} timestamp is outside the supported range"))
}

pub fn temporal_ticks(min_ms: i64, max_ms: i64, plot_width: f64) -> Vec<TemporalTick> {
    let (min_ms, max_ms) = if min_ms <= max_ms {
        (min_ms, max_ms)
    } else {
        (max_ms, min_ms)
    };
    if min_ms == max_ms {
        return vec![TemporalTick {
            unix_millis: min_ms,
            label: tick_label(min_ms, true),
        }];
    }

    let target = if plot_width.is_finite() && plot_width > 0.0 {
        (plot_width / 30.0).floor().clamp(2.0, 24.0) as usize
    } else {
        2
    };
    let interval = INTERVALS_MILLIS
        .iter()
        .copied()
        .find(|&interval| aligned_count(min_ms, max_ms, interval) <= target)
        .unwrap_or(
            *INTERVALS_MILLIS
                .last()
                .expect("interval table is non-empty"),
        );
    let start = aligned_start(min_ms, interval);
    let mut ticks = Vec::new();
    let mut next = start;
    while next <= i128::from(max_ms) && ticks.len() < 24 {
        let unix_millis = i64::try_from(next).expect("aligned tick remains within input range");
        let first_in_month = ticks.last().is_none_or(|previous: &TemporalTick| {
            month_key(previous.unix_millis) != month_key(unix_millis)
        });
        ticks.push(TemporalTick {
            unix_millis,
            label: tick_label(unix_millis, first_in_month),
        });
        next += i128::from(interval);
    }
    ticks
}

fn aligned_count(min_ms: i64, max_ms: i64, interval: i64) -> usize {
    let start = aligned_start(min_ms, interval);
    if start > i128::from(max_ms) {
        return 0;
    }
    let count = (i128::from(max_ms) - start) / i128::from(interval) + 1;
    usize::try_from(count).unwrap_or(usize::MAX)
}

fn aligned_start(min_ms: i64, interval: i64) -> i128 {
    let min = i128::from(min_ms);
    let interval = i128::from(interval);
    let quotient = min / interval;
    let remainder = min % interval;
    let ceiling = if remainder > 0 {
        quotient + 1
    } else {
        quotient
    };
    ceiling * interval
}

fn month_key(unix_millis: i64) -> Option<(i32, Month)> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(unix_millis) * 1_000_000)
        .ok()
        .map(|datetime| (datetime.year(), datetime.month()))
}

fn tick_label(unix_millis: i64, first_in_month: bool) -> String {
    let Ok(datetime) =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(unix_millis) * 1_000_000)
    else {
        return unix_millis.to_string();
    };
    let day = datetime.day();
    if first_in_month {
        format!("{} {day:02}", month_abbreviation(datetime.month()))
    } else {
        format!("{day:02}")
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn dogfood_range_uses_two_day_ticks() {
        let min = parse_rfc3339_millis("x", "2026-06-05T19:55:20Z").unwrap();
        let max = parse_rfc3339_millis("x", "2026-07-22T19:18:38Z").unwrap();
        let ticks = temporal_ticks(min, max, 720.0);
        assert!(ticks.len() <= 24);
        assert!(
            ticks
                .windows(2)
                .all(|w| w[1].unix_millis - w[0].unix_millis == 2 * 86_400_000)
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
            vec![1_000, 2_000, 3_000]
        );

        let singleton = temporal_ticks(1_234, 1_234, 720.0);
        assert_eq!(singleton.len(), 1);
        assert_eq!(singleton[0].unix_millis, 1_234);
    }
}
