use crate::frontend::vegalite_error::type7_quantile;
use crate::ir::{VegaBoxPlotExtent, VegaBoxPlotSummary};

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
