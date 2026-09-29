#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ErrorExtent {
    Stderr,
    Stdev,
    Ci,
    Iqr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ErrorRangeSummary {
    pub center: f64,
    pub lower: f64,
    pub upper: f64,
}

const BOOTSTRAP_RESAMPLES: usize = 1_000;
const MAX_BOOTSTRAP_DRAWS: usize = 10_000_000;

/// Return the required number of bootstrap draws, rejecting work above the limit.
pub(super) fn validate_bootstrap_budget(sample_count: usize) -> Result<usize, String> {
    let draws = sample_count.saturating_mul(BOOTSTRAP_RESAMPLES);
    if draws > MAX_BOOTSTRAP_DRAWS {
        return Err(format!(
            "bootstrap draw budget exceeds {MAX_BOOTSTRAP_DRAWS}"
        ));
    }
    Ok(draws)
}

/// Compute the range summary required by a Vega-Lite error mark extent.
pub(super) fn summarize(
    values: &[f64],
    extent: ErrorExtent,
    seed: u64,
) -> Result<ErrorRangeSummary, String> {
    if values.is_empty() {
        return Err("error extent requires at least one sample".to_string());
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err("error extent samples must be finite".to_string());
    }

    match extent {
        ErrorExtent::Iqr => {
            let mut sorted = values.to_vec();
            sorted.sort_by(f64::total_cmp);
            let lower = type7_quantile(&sorted, 0.25);
            let center = type7_quantile(&sorted, 0.5);
            let upper = type7_quantile(&sorted, 0.75);
            checked_summary(center, lower, upper)
        }
        ErrorExtent::Stderr | ErrorExtent::Stdev | ErrorExtent::Ci => {
            if values.len() < 2 {
                return Err(format!(
                    "error extent {extent:?} requires at least two samples"
                ));
            }
            let (center, normalized_center, scale) = scaled_mean(values)?;
            match extent {
                ErrorExtent::Stderr | ErrorExtent::Stdev => {
                    let standard_deviation =
                        sample_standard_deviation(values, normalized_center, scale)?;
                    let margin = if extent == ErrorExtent::Stderr {
                        standard_deviation / (values.len() as f64).sqrt()
                    } else {
                        standard_deviation
                    };
                    checked_summary(center, center - margin, center + margin)
                }
                ErrorExtent::Ci => {
                    let draw_count = validate_bootstrap_budget(values.len())?;
                    bootstrap_interval(values, center, scale, seed, draw_count)
                }
                ErrorExtent::Iqr => unreachable!("IQR handled above"),
            }
        }
    }
}

fn scaled_mean(values: &[f64]) -> Result<(f64, f64, f64), String> {
    let scale = values.iter().map(|value| value.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return Ok((0.0, 0.0, 0.0));
    }
    let normalized_center =
        values.iter().map(|value| value / scale).sum::<f64>() / values.len() as f64;
    let center = normalized_center * scale;
    if !center.is_finite() || !normalized_center.is_finite() {
        return Err("error extent mean must be finite".to_string());
    }
    Ok((center, normalized_center, scale))
}

fn sample_standard_deviation(
    values: &[f64],
    normalized_center: f64,
    scale: f64,
) -> Result<f64, String> {
    if scale == 0.0 {
        return Ok(0.0);
    }
    let sum_squared_deviations = values
        .iter()
        .map(|value| {
            let deviation = value / scale - normalized_center;
            deviation * deviation
        })
        .sum::<f64>();
    let deviation = (sum_squared_deviations / (values.len() - 1) as f64).sqrt() * scale;
    if !deviation.is_finite() {
        return Err("error extent standard deviation must be finite".to_string());
    }
    Ok(deviation)
}

fn bootstrap_interval(
    values: &[f64],
    center: f64,
    scale: f64,
    seed: u64,
    draw_count: usize,
) -> Result<ErrorRangeSummary, String> {
    debug_assert_eq!(draw_count, values.len() * BOOTSTRAP_RESAMPLES);
    let mut rng = SplitMix64::new(seed);
    let mut means = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    for _ in 0..BOOTSTRAP_RESAMPLES {
        let normalized_sum = if scale == 0.0 {
            0.0
        } else {
            (0..values.len())
                .map(|_| values[rng.index(values.len())] / scale)
                .sum::<f64>()
        };
        let mean = if scale == 0.0 {
            0.0
        } else {
            normalized_sum / values.len() as f64 * scale
        };
        if !mean.is_finite() {
            return Err("bootstrap mean must be finite".to_string());
        }
        means.push(mean);
    }
    means.sort_by(f64::total_cmp);
    checked_summary(
        center,
        type7_quantile(&means, 0.025),
        type7_quantile(&means, 0.975),
    )
}

pub(super) fn type7_quantile(sorted: &[f64], probability: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    debug_assert!((0.0..=1.0).contains(&probability));
    let position = (sorted.len() - 1) as f64 * probability;
    let lower_index = position.floor() as usize;
    let upper_index = position.ceil() as usize;
    if lower_index == upper_index {
        return sorted[lower_index];
    }
    let fraction = position - lower_index as f64;
    sorted[lower_index] * (1.0 - fraction) + sorted[upper_index] * fraction
}

fn checked_summary(center: f64, lower: f64, upper: f64) -> Result<ErrorRangeSummary, String> {
    if !center.is_finite() || !lower.is_finite() || !upper.is_finite() {
        return Err("error extent bounds must be finite".to_string());
    }
    if lower > upper {
        return Err("error extent lower bound exceeds upper bound".to_string());
    }
    Ok(ErrorRangeSummary {
        center,
        lower,
        upper,
    })
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        ((u128::from(self.next_u64()) * len as u128) >> 64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorExtent, ErrorRangeSummary, summarize, validate_bootstrap_budget};

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn error_mark_statistics_stderr_stdev_iqr() {
        let values = [1.0, 2.0, 3.0];
        let stderr = summarize(&values, ErrorExtent::Stderr, 0).unwrap();
        let expected_margin = 1.0 / 3.0_f64.sqrt();
        assert_close(stderr.center, 2.0);
        assert_close(stderr.lower, 2.0 - expected_margin);
        assert_close(stderr.upper, 2.0 + expected_margin);

        let stdev = summarize(&values, ErrorExtent::Stdev, 0).unwrap();
        assert_close(stdev.center, 2.0);
        assert_close(stdev.lower, 1.0);
        assert_close(stdev.upper, 3.0);

        let iqr = summarize(&[0.0, 2.0, 4.0, 6.0], ErrorExtent::Iqr, 0).unwrap();
        assert_close(iqr.center, 3.0);
        assert_close(iqr.lower, 1.5);
        assert_close(iqr.upper, 4.5);
    }

    #[test]
    fn error_mark_statistics_ci_is_deterministic_per_seed() {
        let values = [-100.0, 0.0, 0.5, 1.0, 3.0, 4.0, 9.0, 17.0, 40.0];
        let first = summarize(&values, ErrorExtent::Ci, 0x1234_5678).unwrap();
        let repeated = summarize(&values, ErrorExtent::Ci, 0x1234_5678).unwrap();
        let other_seed = summarize(&values, ErrorExtent::Ci, 0x8765_4321).unwrap();

        assert_eq!(first, repeated);
        assert_ne!(first, other_seed);
        assert!(first.lower <= first.center);
        assert!(first.center <= first.upper);
    }

    #[test]
    fn error_mark_statistics_rejects_invalid_samples() {
        for extent in [ErrorExtent::Stderr, ErrorExtent::Stdev, ErrorExtent::Ci] {
            assert!(summarize(&[42.0], extent, 0).is_err());
        }
        assert!(summarize(&[], ErrorExtent::Iqr, 0).is_err());
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(summarize(&[1.0, invalid], ErrorExtent::Stderr, 0).is_err());
        }

        let singleton_iqr: ErrorRangeSummary = summarize(&[42.0], ErrorExtent::Iqr, 0).unwrap();
        assert_eq!(singleton_iqr.center, 42.0);
        assert_eq!(singleton_iqr.lower, 42.0);
        assert_eq!(singleton_iqr.upper, 42.0);
    }

    #[test]
    fn error_mark_statistics_preflights_bootstrap_draws() {
        assert_eq!(validate_bootstrap_budget(0).unwrap(), 0);
        assert_eq!(validate_bootstrap_budget(10_000).unwrap(), 10_000_000);
        assert!(validate_bootstrap_budget(10_001).is_err());
        assert!(validate_bootstrap_budget(usize::MAX).is_err());
    }
}
